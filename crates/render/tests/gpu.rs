//! Renders real frames on whatever GPU (or software adapter) is available and checks the pixels.
//! Each test skips itself, with a note, if the machine has no adapter at all.

use vrd_render::{RenderParams, Renderer};
use vrd_scene::glam::{Mat4, Vec3};
use vrd_scene::mesh::{self, linear_color};
use vrd_scene::{Camera, LineSegment, ModelInstance, SceneFrame};

const SIZE: u32 = 256;
const CLEAR: [f64; 4] = [0.2, 0.2, 0.2, 1.0];

struct Gpu {
    renderer: Renderer,
}

fn gpu() -> Option<Gpu> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default())).ok()?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).ok()?;
    eprintln!("using adapter: {}", adapter.get_info().name);
    Some(Gpu { renderer: Renderer::new(&device, &queue) })
}

macro_rules! gpu_or_skip {
    () => {
        match gpu() {
            Some(g) => g,
            None => {
                eprintln!("no GPU adapter available; skipping");
                return;
            }
        }
    };
}

fn camera() -> Camera {
    Camera { target: Vec3::ZERO, yaw: 0.0, pitch: 0.0, distance: 1000.0, ..Camera::default() }
}

fn params(cam: &Camera, size: [u32; 2]) -> RenderParams {
    RenderParams {
        size,
        view_proj: cam.view_proj(size[0] as f32 / size[1] as f32),
        eye: cam.eye(),
        near: cam.near(),
        clear: CLEAR,
    }
}

fn render(gpu: &mut Gpu, frame: &SceneFrame) -> Vec<u8> {
    gpu.renderer.render(frame, &params(&camera(), [SIZE, SIZE]));
    gpu.renderer.read_pixels().expect("readback")
}

fn px(pixels: &[u8], x: u32, y: u32) -> [u8; 3] {
    let i = ((y * SIZE + x) * 4) as usize;
    [pixels[i], pixels[i + 1], pixels[i + 2]]
}

fn brightness(p: [u8; 3]) -> u32 {
    p.iter().map(|&c| u32::from(c)).sum()
}

fn sphere_frame(center: Vec3, radius: f32, color: [f32; 4]) -> SceneFrame {
    let mut f = SceneFrame::default();
    mesh::sphere(&mut f.solid, center, radius, color);
    f
}

const RED: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const BLUE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];
const MID: u32 = SIZE / 2;

#[test]
fn an_empty_scene_is_the_clear_colour() {
    let mut g = gpu_or_skip!();
    let img = render(&mut g, &SceneFrame::default());
    assert_eq!(img.len(), (SIZE * SIZE * 4) as usize);
    // Linear 0.2 encodes to sRGB 124.
    for p in [px(&img, 0, 0), px(&img, MID, MID), px(&img, SIZE - 1, SIZE - 1)] {
        assert!(p.iter().all(|&c| (i32::from(c) - 124).abs() <= 2), "{p:?}");
    }
    // Alpha is fully opaque everywhere.
    assert!(img.chunks(4).all(|p| p[3] == 255));
}

#[test]
fn a_lit_sphere_is_red_and_brighter_on_top() {
    let mut g = gpu_or_skip!();
    let img = render(&mut g, &sphere_frame(Vec3::ZERO, 100.0, RED));

    let centre = px(&img, MID, MID);
    assert!(centre[0] > 120 && centre[1] < 40 && centre[2] < 40, "centre {centre:?}");

    // The key light comes from above, so the upper half must be brighter than the lower half.
    // A sphere of radius 100 at distance 1000 and a 45 degree fov spans about 31 px.
    let top = px(&img, MID, MID - 22);
    let bottom = px(&img, MID, MID + 22);
    assert!(top[0] > 120 && bottom[0] > 60, "sphere covers both probes: {top:?} {bottom:?}");
    assert!(top[0] > bottom[0] + 20, "top {top:?} should be lit more than bottom {bottom:?}");

    // And it is round, not a full-screen quad.
    assert!(px(&img, MID + 60, MID).iter().all(|&c| (i32::from(c) - 124).abs() <= 2));
}

#[test]
fn nearer_geometry_hides_farther_geometry_in_either_draw_order() {
    let mut g = gpu_or_skip!();
    // The camera sits on +X looking along -X, so larger x is nearer.
    let near_blue = (Vec3::new(150.0, 0.0, 0.0), BLUE);
    let far_red = (Vec3::new(-150.0, 0.0, 0.0), RED);
    for order in [[near_blue, far_red], [far_red, near_blue]] {
        let mut f = SceneFrame::default();
        for (c, color) in order {
            mesh::sphere(&mut f.solid, c, 100.0, color);
        }
        let p = px(&render(&mut g, &f), MID, MID);
        assert!(p[2] > p[0] + 60, "blue sphere must cover red: {p:?}");
    }
}

#[test]
fn lines_are_drawn_with_width_and_hidden_behind_solids() {
    let mut g = gpu_or_skip!();
    let mut f = SceneFrame::default();
    // A white 4px line across the whole view at the target's height.
    f.lines.push(LineSegment::new(Vec3::new(0.0, -1000.0, 0.0), Vec3::new(0.0, 1000.0, 0.0), [1.0; 4], 4.0));
    let img = render(&mut g, &f);
    assert!(brightness(px(&img, MID + 80, MID)) > 650, "on the line: {:?}", px(&img, MID + 80, MID));
    assert!(brightness(px(&img, MID + 80, MID + 10)) < 400, "off the line");
    assert!(brightness(px(&img, MID + 80, MID - 10)) < 400, "off the line");

    // Put a sphere in front of the line: the line must not show through it.
    let mut hidden = sphere_frame(Vec3::new(300.0, 0.0, 0.0), 100.0, BLUE);
    hidden.lines = f.lines.clone();
    let img = render(&mut g, &hidden);
    let p = px(&img, MID, MID);
    assert!(p[2] > p[0] + 40 && brightness(p) < 500, "line hidden by the sphere: {p:?}");
    assert!(brightness(px(&img, MID + 80, MID)) > 650, "still visible beside the sphere");
}

#[test]
fn a_line_that_passes_behind_the_camera_is_clipped_not_garbled() {
    let mut g = gpu_or_skip!();
    let mut f = SceneFrame::default();
    // From in front of the camera to far behind it.
    f.lines.push(LineSegment::new(Vec3::new(0.0, 0.0, 0.0), Vec3::new(5000.0, 0.0, 0.0), [1.0; 4], 3.0));
    // Entirely behind the camera.
    f.lines.push(LineSegment::new(
        Vec3::new(2000.0, -50.0, 0.0),
        Vec3::new(3000.0, 50.0, 0.0),
        [1.0, 0.0, 0.0, 1.0],
        3.0,
    ));
    let img = render(&mut g, &f);
    let lit = img.chunks(4).filter(|p| p[0] > 200 && p[1] > 200 && p[2] > 200).count();
    let red = img.chunks(4).filter(|p| p[0] > 200 && p[1] < 60 && p[2] < 60).count();
    // The first line runs straight at the camera, so it collapses to (almost) a point.
    assert!(lit < 2000, "no giant artefact: {lit} white pixels");
    assert_eq!(red, 0, "geometry behind the camera is invisible");
}

#[test]
fn translucent_geometry_blends_with_what_is_behind_it() {
    let mut g = gpu_or_skip!();
    let mut f = SceneFrame::default();
    mesh::sphere(&mut f.translucent, Vec3::ZERO, 100.0, [1.0, 0.0, 0.0, 0.5]);
    let img = render(&mut g, &f);
    let p = px(&img, MID, MID);
    // Half red over mid-gray: red channel up, green/blue dragged down but not black.
    assert!(p[0] > 140, "{p:?}");
    assert!(p[1] > 30 && p[1] < 124, "{p:?}");
}

#[test]
fn registration_models_are_placed_by_their_instance_matrix() {
    let mut g = gpu_or_skip!();
    let mut assets = vrd_scene::SceneAssets::default();
    let mut vertices = Vec::new();
    mesh::sphere(&mut vertices, Vec3::ZERO, 60.0, linear_color(vrd_model_color(), 1.0));
    assets.models.push(vrd_scene::StaticModel {
        entity: 1,
        color: vrd_model_color(),
        vertices,
        center: Vec3::ZERO,
        radius: 60.0,
    });
    g.renderer.set_assets(&assets);

    let mut f = SceneFrame::default();
    // Along +Y is screen-right for this camera.
    f.instances.push(ModelInstance {
        model: 0,
        matrix: Mat4::from_translation(Vec3::new(0.0, 200.0, 0.0)),
        tint: [1.0; 4],
        mix: [0.0; 4],
    });
    let img = render(&mut g, &f);
    let moved = px(&img, MID + 50, MID);
    let origin = px(&img, MID, MID);
    assert!(
        moved[1] > moved[0] + 80 && moved[1] > moved[2] + 80,
        "lime model drawn at the translated position: {moved:?}"
    );
    assert!(origin.iter().all(|&c| (i32::from(c) - 124).abs() <= 2), "and nothing left at the origin: {origin:?}");

    // A highlighted instance shifts towards the highlight colour.
    f.instances[0].mix = [1.0, 0.45, 0.05, 0.9];
    let hi = px(&render(&mut g, &f), MID + 50, MID);
    assert!(hi[0] > moved[0] + 40, "blended towards orange: {hi:?} vs {moved:?}");

    // An instance that refers to a model that was never uploaded is ignored, not a crash.
    f.instances.push(ModelInstance { model: 99, matrix: Mat4::IDENTITY, tint: [1.0; 4], mix: [0.0; 4] });
    render(&mut g, &f);
}

fn vrd_model_color() -> vrd_scene::Color {
    vrd_scene::Color::from_name("Lime").unwrap()
}

#[test]
fn resizing_replaces_the_output_and_bumps_the_generation() {
    let mut g = gpu_or_skip!();
    let before = g.renderer.generation();
    assert_eq!(before, 0);
    g.renderer.render(&SceneFrame::default(), &params(&camera(), [64, 32]));
    let first = g.renderer.generation();
    assert_eq!(g.renderer.output_size(), Some([64, 32]));
    g.renderer.render(&SceneFrame::default(), &params(&camera(), [64, 32]));
    assert_eq!(g.renderer.generation(), first, "same size reuses the target");
    g.renderer.render(&SceneFrame::default(), &params(&camera(), [128, 96]));
    assert!(g.renderer.generation() > first);
    assert_eq!(g.renderer.read_pixels().unwrap().len(), 128 * 96 * 4);
    // Degenerate sizes are clamped rather than rejected by the driver.
    g.renderer.render(&SceneFrame::default(), &params(&camera(), [0, 0]));
    assert_eq!(g.renderer.output_size(), Some([1, 1]));
}

#[test]
fn many_primitives_and_growth_do_not_break_the_buffers() {
    let mut g = gpu_or_skip!();
    let mut f = SceneFrame::default();
    for i in 0..400 {
        let p = Vec3::new(0.0, ((i % 20) as f32 - 10.0) * 25.0, ((i / 20) as f32 - 10.0) * 25.0);
        mesh::sphere(&mut f.solid, p, 10.0, RED);
        f.lines.push(LineSegment::new(p, p + Vec3::Y * 20.0, [1.0; 4], 1.0));
    }
    let img = render(&mut g, &f);
    assert!(img.chunks(4).any(|p| p[0] > 100 && p[1] < 60));
    // Shrinking again afterwards still works.
    render(&mut g, &SceneFrame::default());
}
