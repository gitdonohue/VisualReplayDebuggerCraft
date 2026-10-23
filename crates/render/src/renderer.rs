//! A wgpu renderer for the 3D viewport.
//!
//! It draws into its own offscreen colour texture (4x MSAA, reversed-Z depth), which the host UI
//! then shows like an image. Rendering offscreen keeps this independent of the UI framework and
//! lets it run headless in tests.

use std::mem::size_of;

use bytemuck::{Pod, Zeroable};
use vrd_scene::glam::{Mat4, Vec3};
use vrd_scene::{LineSegment, SceneAssets, SceneFrame, Vertex};
use wgpu::util::DeviceExt as _;

/// The colour format of the output texture: plain RGBA8 holding sRGB-encoded colour. The shaders do
/// their lighting in linear space and encode the result themselves, so that the texture can be
/// shown by egui (which expects gamma-encoded images) on every backend, including WebGL.
pub const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const SAMPLES: u32 = 4;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    eye_near: [f32; 4],
    viewport: [f32; 4],
    light0: [f32; 4],
    light1: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct ModelUniform {
    matrix: [[f32; 4]; 4],
    tint: [f32; 4],
    mix_color: [f32; 4],
}

/// Linear to sRGB transfer function.
fn encode_srgb(c: f64) -> f64 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.0031308 { c * 12.92 } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 }
}

/// How to draw one frame.
#[derive(Debug, Clone)]
pub struct RenderParams {
    /// Size of the output in pixels.
    pub size: [u32; 2],
    pub view_proj: Mat4,
    pub eye: Vec3,
    pub near: f32,
    /// Background colour as linear RGBA.
    pub clear: [f64; 4],
}

struct GpuModel {
    buffer: wgpu::Buffer,
    vertex_count: u32,
}

struct Targets {
    size: [u32; 2],
    msaa_color: wgpu::TextureView,
    depth: wgpu::TextureView,
    /// Kept for reading the image back, which only native builds do.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    resolve: wgpu::Texture,
    resolve_view: wgpu::TextureView,
}

/// A GPU buffer that grows when more space is needed.
struct Growable {
    buffer: wgpu::Buffer,
    capacity: u64,
    usage: wgpu::BufferUsages,
    label: &'static str,
}

impl Growable {
    fn new(device: &wgpu::Device, label: &'static str, usage: wgpu::BufferUsages) -> Self {
        let capacity = 4096;
        Self { buffer: Self::create(device, label, usage, capacity), capacity, usage, label }
    }

    fn create(device: &wgpu::Device, label: &'static str, usage: wgpu::BufferUsages, size: u64) -> wgpu::Buffer {
        device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size,
            usage: usage | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })
    }

    /// Returns true if the buffer was replaced (so bind groups that use it must be rebuilt).
    fn write(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, data: &[u8]) -> bool {
        let needed = data.len() as u64;
        let mut replaced = false;
        if needed > self.capacity {
            self.capacity = needed.next_power_of_two();
            self.buffer = Self::create(device, self.label, self.usage, self.capacity);
            replaced = true;
        }
        if !data.is_empty() {
            queue.write_buffer(&self.buffer, 0, data);
        }
        replaced
    }
}

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    globals: wgpu::Buffer,
    globals_bind: wgpu::BindGroup,
    model_layout: wgpu::BindGroupLayout,
    model_stride: u64,
    models: Growable,
    model_bind: wgpu::BindGroup,
    solid_pipeline: wgpu::RenderPipeline,
    translucent_pipeline: wgpu::RenderPipeline,
    line_pipeline: wgpu::RenderPipeline,
    static_models: Vec<GpuModel>,
    dynamic_solid: Growable,
    dynamic_translucent: Growable,
    lines: Growable,
    targets: Option<Targets>,
    generation: u64,
}

impl Renderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("vrd shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX | wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(size_of::<Globals>() as u64),
                },
                count: None,
            }],
        });
        let model_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("model layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(size_of::<ModelUniform>() as u64),
                },
                count: None,
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("vrd pipeline layout"),
            bind_group_layouts: &[&globals_layout, &model_layout],
            push_constant_ranges: &[],
        });

        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals_bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() }],
        });

        let alignment = u64::from(device.limits().min_uniform_buffer_offset_alignment);
        let model_stride = (size_of::<ModelUniform>() as u64).div_ceil(alignment) * alignment;
        let models = Growable::new(device, "model uniforms", wgpu::BufferUsages::UNIFORM);
        let model_bind = Self::model_bind_group(device, &model_layout, &models.buffer);

        let depth_stencil = |write: bool| wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: write,
            // Depth is reversed: nearer surfaces have larger values.
            depth_compare: wgpu::CompareFunction::Greater,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        };
        let multisample = wgpu::MultisampleState { count: SAMPLES, mask: !0, alpha_to_coverage_enabled: false };
        let alpha_blend = Some(wgpu::BlendState::ALPHA_BLENDING);

        let mesh_buffers = [wgpu::VertexBufferLayout {
            array_stride: size_of::<Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x4],
        }];
        let mesh_pipeline = |label: &str, fragment: &str, write_depth: bool, blend: Option<wgpu::BlendState>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_mesh"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &mesh_buffers,
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: Some(depth_stencil(write_depth)),
                multisample,
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fragment),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: COLOR_FORMAT,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview: None,
                cache: None,
            })
        };
        let solid_pipeline = mesh_pipeline("solid", "fs_solid", true, None);
        let translucent_pipeline = mesh_pipeline("translucent", "fs_translucent", false, alpha_blend);

        let line_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("lines"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_line"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: size_of::<LineSegment>() as u64,
                    step_mode: wgpu::VertexStepMode::Instance,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x4, 1 => Float32x4, 2 => Float32x4],
                }],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: Some(depth_stencil(false)),
            multisample,
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_line"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: COLOR_FORMAT,
                    blend: alpha_blend,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });

        Self {
            device: device.clone(),
            queue: queue.clone(),
            globals,
            globals_bind,
            model_layout,
            model_stride,
            models,
            model_bind,
            solid_pipeline,
            translucent_pipeline,
            line_pipeline,
            static_models: Vec::new(),
            dynamic_solid: Growable::new(device, "dynamic solid", wgpu::BufferUsages::VERTEX),
            dynamic_translucent: Growable::new(device, "dynamic translucent", wgpu::BufferUsages::VERTEX),
            lines: Growable::new(device, "lines", wgpu::BufferUsages::VERTEX),
            targets: None,
            generation: 0,
        }
    }

    fn model_bind_group(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        buffer: &wgpu::Buffer,
    ) -> wgpu::BindGroup {
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("model"),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(size_of::<ModelUniform>() as u64),
                }),
            }],
        })
    }

    /// Uploads a capture's registration geometry, replacing whatever was loaded before.
    pub fn set_assets(&mut self, assets: &SceneAssets) {
        self.static_models = assets
            .models
            .iter()
            .map(|m| GpuModel {
                buffer: self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("static model"),
                    contents: bytemuck::cast_slice(&m.vertices),
                    usage: wgpu::BufferUsages::VERTEX,
                }),
                vertex_count: m.vertices.len() as u32,
            })
            .collect();
    }

    /// Changes whenever the output texture is replaced (for example after a resize), so the host
    /// knows to re-register it.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// The most recently rendered image (single-sampled, sRGB-encoded RGBA8).
    pub fn output(&self) -> Option<&wgpu::TextureView> {
        self.targets.as_ref().map(|t| &t.resolve_view)
    }

    pub fn output_size(&self) -> Option<[u32; 2]> {
        self.targets.as_ref().map(|t| t.size)
    }

    fn ensure_targets(&mut self, size: [u32; 2]) {
        if self.targets.as_ref().is_some_and(|t| t.size == size) {
            return;
        }
        let extent = wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 };
        let make = |label: &str, format, samples, usage| {
            self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: extent,
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let msaa = make("msaa color", COLOR_FORMAT, SAMPLES, wgpu::TextureUsages::RENDER_ATTACHMENT);
        let depth = make("depth", DEPTH_FORMAT, SAMPLES, wgpu::TextureUsages::RENDER_ATTACHMENT);
        let resolve = make(
            "viewport output",
            COLOR_FORMAT,
            1,
            wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
        );
        let view = |t: &wgpu::Texture| t.create_view(&wgpu::TextureViewDescriptor::default());
        self.targets =
            Some(Targets { size, msaa_color: view(&msaa), depth: view(&depth), resolve_view: view(&resolve), resolve });
        self.generation += 1;
    }

    /// Draws a frame into the output texture.
    pub fn render(&mut self, frame: &SceneFrame, params: &RenderParams) {
        let max = self.device.limits().max_texture_dimension_2d;
        let size = [params.size[0].clamp(1, max), params.size[1].clamp(1, max)];
        self.ensure_targets(size);

        self.queue.write_buffer(
            &self.globals,
            0,
            bytemuck::bytes_of(&Globals {
                view_proj: params.view_proj.to_cols_array_2d(),
                eye_near: [params.eye.x, params.eye.y, params.eye.z, params.near],
                viewport: [size[0] as f32, size[1] as f32, 0.0, 0.0],
                light0: [0.35, -0.45, 0.82, 0.0],
                light1: [-0.5, 0.4, -0.6, 0.0],
            }),
        );

        // Slot 0 is the identity, used by geometry that is already in world space.
        let mut uniforms = vec![0u8; self.model_stride as usize * (frame.instances.len() + 1)];
        let mut put = |slot: usize, u: ModelUniform| {
            let at = slot * self.model_stride as usize;
            uniforms[at..at + size_of::<ModelUniform>()].copy_from_slice(bytemuck::bytes_of(&u));
        };
        put(0, ModelUniform { matrix: Mat4::IDENTITY.to_cols_array_2d(), tint: [1.0; 4], mix_color: [0.0; 4] });
        for (i, inst) in frame.instances.iter().enumerate() {
            put(i + 1, ModelUniform { matrix: inst.matrix.to_cols_array_2d(), tint: inst.tint, mix_color: inst.mix });
        }
        if self.models.write(&self.device, &self.queue, &uniforms) {
            self.model_bind = Self::model_bind_group(&self.device, &self.model_layout, &self.models.buffer);
        }
        self.dynamic_solid.write(&self.device, &self.queue, bytemuck::cast_slice(&frame.solid));
        self.dynamic_translucent.write(&self.device, &self.queue, bytemuck::cast_slice(&frame.translucent));
        self.lines.write(&self.device, &self.queue, bytemuck::cast_slice(&frame.lines));

        let Some(targets) = &self.targets else {
            return;
        };
        let mut encoder =
            self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("viewport") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("viewport"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &targets.msaa_color,
                    depth_slice: None,
                    resolve_target: Some(&targets.resolve_view),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: encode_srgb(params.clear[0]),
                            g: encode_srgb(params.clear[1]),
                            b: encode_srgb(params.clear[2]),
                            a: params.clear[3],
                        }),
                        store: wgpu::StoreOp::Discard,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &targets.depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(0.0), store: wgpu::StoreOp::Discard }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_bind_group(0, &self.globals_bind, &[]);

            let stride = self.model_stride as u32;
            let is_translucent = |tint: &[f32; 4]| tint[3] < 0.999;

            // Opaque: world-space geometry, then registration geometry.
            pass.set_pipeline(&self.solid_pipeline);
            if !frame.solid.is_empty() {
                pass.set_bind_group(1, &self.model_bind, &[0]);
                pass.set_vertex_buffer(0, self.dynamic_solid.buffer.slice(..));
                pass.draw(0..frame.solid.len() as u32, 0..1);
            }
            for (i, inst) in frame.instances.iter().enumerate().filter(|(_, i)| !is_translucent(&i.tint)) {
                if let Some(model) = self.static_models.get(inst.model) {
                    pass.set_bind_group(1, &self.model_bind, &[(i as u32 + 1) * stride]);
                    pass.set_vertex_buffer(0, model.buffer.slice(..));
                    pass.draw(0..model.vertex_count, 0..1);
                }
            }

            // Lines are depth tested against the opaque surfaces but never write depth.
            if !frame.lines.is_empty() {
                pass.set_pipeline(&self.line_pipeline);
                pass.set_bind_group(1, &self.model_bind, &[0]);
                pass.set_vertex_buffer(0, self.lines.buffer.slice(..));
                pass.draw(0..6, 0..frame.lines.len() as u32);
            }

            // Translucent last so it blends over everything behind it.
            pass.set_pipeline(&self.translucent_pipeline);
            if !frame.translucent.is_empty() {
                pass.set_bind_group(1, &self.model_bind, &[0]);
                pass.set_vertex_buffer(0, self.dynamic_translucent.buffer.slice(..));
                pass.draw(0..frame.translucent.len() as u32, 0..1);
            }
            for (i, inst) in frame.instances.iter().enumerate().filter(|(_, i)| is_translucent(&i.tint)) {
                if let Some(model) = self.static_models.get(inst.model) {
                    pass.set_bind_group(1, &self.model_bind, &[(i as u32 + 1) * stride]);
                    pass.set_vertex_buffer(0, model.buffer.slice(..));
                    pass.draw(0..model.vertex_count, 0..1);
                }
            }
        }
        self.queue.submit(Some(encoder.finish()));
    }

    /// Copies the rendered image back to the CPU as tightly packed RGBA8 (sRGB-encoded) rows.
    /// This blocks until the GPU is done, so it is meant for tests and screenshots.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn read_pixels(&self) -> Option<Vec<u8>> {
        let targets = self.targets.as_ref()?;
        let [width, height] = targets.size;
        let padded = (width * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: u64::from(padded) * u64::from(height),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder =
            self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("readback") });
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &targets.resolve,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(padded),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        self.queue.submit(Some(encoder.finish()));

        let slice = buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |result| {
            result.expect("mapping the readback buffer");
        });
        self.device.poll(wgpu::PollType::wait_indefinitely()).ok()?;

        let data = slice.get_mapped_range();
        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        for row in data.chunks(padded as usize).take(height as usize) {
            pixels.extend_from_slice(&row[..(width * 4) as usize]);
        }
        drop(data);
        buffer.unmap();
        Some(pixels)
    }
}
