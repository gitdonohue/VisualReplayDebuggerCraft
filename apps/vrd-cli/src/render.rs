//! `vrd-cli render`: draw a frame of a capture to a PNG with the same renderer the viewport uses.

use std::path::Path;

use vrd_model::Replay;
use vrd_render::{RenderParams, Renderer};
use vrd_scene::{Camera, FrameRequest, SceneAssets, ViewOptions, build_frame};

pub const USAGE: &str = "\
    vrd-cli render <file> --out <image.png> [options]
        --frame N            frame to draw (default 0)
        --size WxH           image size in pixels (default 1280x720)
        --camera MODE        follow (the maincamera entity), all (frame every entity),
                             or entity:ID (look at one entity). Default: follow if there is a camera, else all
        --yaw DEG --pitch DEG  orbit angles for the `all` and `entity:ID` cameras
        --up x|y|z           world up axis (default: detected from the data)
        --select ID          highlight an entity
        --paths              show every entity's path
        --dark               dark background";

#[derive(Debug)]
enum CameraMode {
    Auto,
    Follow,
    All,
    Entity(i32),
}

pub fn run(args: &[String]) -> Result<(), String> {
    let path = args.first().filter(|a| !a.starts_with("--")).ok_or(USAGE)?;
    let flag = |name: &str| args.iter().position(|a| a == name);
    let value = |name: &str| -> Result<Option<&String>, String> {
        match flag(name) {
            Some(i) => args.get(i + 1).map(Some).ok_or_else(|| format!("{name} needs a value")),
            None => Ok(None),
        }
    };
    let number = |name: &str| -> Result<Option<f32>, String> {
        value(name)?.map(|v| v.parse::<f32>().map_err(|_| format!("{name} needs a number, got `{v}`"))).transpose()
    };

    let out = value("--out")?.ok_or("--out is required")?;
    let frame = number("--frame")?.unwrap_or(0.0) as i32;
    let (width, height) = match value("--size")? {
        Some(s) => {
            let (w, h) = s.split_once('x').ok_or("--size must look like 1280x720")?;
            (w.parse::<u32>().map_err(|_| "bad --size width")?, h.parse::<u32>().map_err(|_| "bad --size height")?)
        }
        None => (1280, 720),
    };
    let mode = match value("--camera")?.map(String::as_str) {
        None => CameraMode::Auto,
        Some("follow") => CameraMode::Follow,
        Some("all") => CameraMode::All,
        Some(other) => match other.strip_prefix("entity:").and_then(|id| id.parse().ok()) {
            Some(id) => CameraMode::Entity(id),
            None => return Err(format!("unknown --camera `{other}`")),
        },
    };
    let dark = flag("--dark").is_some();

    let bytes = std::fs::read(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    let replay = Replay::from_bytes(&bytes).map_err(|e| format!("{path}: {e}"))?;
    let frame = frame.clamp(0, replay.last_frame());
    let assets = SceneAssets::build(&replay);

    let up = match value("--up")?.map(|s| s.to_ascii_lowercase()).as_deref() {
        None => assets.detected_up(),
        Some("x") => vrd_scene::Axis::X,
        Some("y") => vrd_scene::Axis::Y,
        Some("z") => vrd_scene::Axis::Z,
        Some(other) => return Err(format!("--up must be x, y or z, got `{other}`")),
    };
    let mut camera = Camera { up_axis: up, ..Camera::default() };
    if let Some(yaw) = number("--yaw")? {
        camera.yaw = yaw.to_radians();
    }
    if let Some(pitch) = number("--pitch")? {
        camera.pitch = pitch.to_radians();
    }
    let mode = match mode {
        CameraMode::Auto if assets.camera_entity().is_some() => CameraMode::Follow,
        CameraMode::Auto => CameraMode::All,
        other => other,
    };
    match mode {
        CameraMode::Follow => {
            let (position, look) =
                vrd_scene::follow_pose(&replay, &assets, frame, vrd_scene::default_camera_forward(up))
                    .ok_or("this capture has no `maincamera` entity")?;
            camera.fov_y = 60f32.to_radians();
            camera.set_pose(position, look, assets.unit());
        }
        CameraMode::Entity(id) => {
            let (center, radius) =
                vrd_scene::focus_target(&replay, &assets, id, frame).ok_or(format!("entity {id} has no position"))?;
            camera.focus(center, radius * 3.0);
        }
        CameraMode::All | CameraMode::Auto => {
            let (center, radius) = vrd_scene::frame_all(&replay, &assets, frame);
            camera.focus(center, radius);
        }
    }

    let options = ViewOptions { up: Some(up), show_all_paths: flag("--paths").is_some(), ..ViewOptions::default() };
    let selected = number("--select")?.map(|id| id as i32);
    let scene = build_frame(&FrameRequest {
        replay: &replay,
        assets: &assets,
        frame,
        selected,
        options: &options,
        camera: &camera,
        dark_mode: dark,
    });

    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
        .map_err(|e| format!("no graphics adapter: {e}"))?;
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
        .map_err(|e| format!("cannot open the GPU: {e}"))?;
    let mut renderer = Renderer::new(&device, &queue);
    renderer.set_assets(&assets);
    let background =
        if dark { vrd_scene::linear_rgba(28, 28, 28, 1.0) } else { vrd_scene::linear_rgba(211, 211, 211, 1.0) };
    renderer.render(
        &scene,
        &RenderParams {
            size: [width, height],
            view_proj: camera.view_proj(width as f32 / height as f32),
            eye: camera.eye(),
            near: camera.near(),
            clear: background.map(f64::from),
        },
    );
    let [w, h] = renderer.output_size().ok_or("nothing was rendered")?;
    let pixels = renderer.read_pixels().ok_or("could not read the image back")?;
    write_png(Path::new(out), w, h, &pixels)?;
    println!(
        "wrote {out} ({w}x{h}): frame {frame}, {} instances, {} solid + {} translucent vertices, {} lines",
        scene.instances.len(),
        scene.solid.len(),
        scene.translucent.len(),
        scene.lines.len()
    );
    Ok(())
}

fn write_png(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<(), String> {
    let file = std::fs::File::create(path).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .and_then(|mut w| w.write_image_data(rgba))
        .map_err(|e| format!("cannot encode the image: {e}"))
}
