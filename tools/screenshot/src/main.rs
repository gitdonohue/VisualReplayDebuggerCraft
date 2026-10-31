//! Renders the whole UI to a PNG without a window, after scripted interactions, for looking at UI
//! changes (the 3D viewport included) from the command line:
//!
//! ```text
//! cargo run --release --manifest-path tools/screenshot/Cargo.toml -- testdata/sample.vrd out.png --tab graphs \
//!     --select PhysicsSceneQueriesSystem --frame 600
//! ```
//!
//! Options: `--size WxH`, `--select NAME`, `--pin NAME` (repeatable), `--tab logs|graphs|timelines`,
//! `--frame N`, `--zoom FACTOR` (narrow the graphs' time window around the playhead),
//! `--overlay` (graphs not split per entity), `--clip` (clip spikes), `--flat` / `--changes-only`\n//! (timelines), `--filter TEXT`, `--dark`,
//! `--hover X,Y` (pointer at fractions of the graph area, to show the hover read-out).

use egui_kittest::Harness;
use egui_kittest::wgpu::{WgpuTestRenderer, create_render_state, default_wgpu_setup};
use vrd_ui_egui::{BottomTab, ReplayApp};

fn main() {
    if let Err(e) = run() {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (capture, out) = match (args.first(), args.get(1)) {
        (Some(c), Some(o)) if !c.starts_with("--") && !o.starts_with("--") => (c.clone(), o.clone()),
        _ => return Err("usage: screenshot <capture.vrd> <out.png> [options]".into()),
    };
    let values = |name: &str| -> Vec<String> {
        args.iter().enumerate().filter(|(_, a)| *a == name).filter_map(|(i, _)| args.get(i + 1).cloned()).collect()
    };
    let value = |name: &str| values(name).into_iter().next();
    let flag = |name: &str| args.iter().any(|a| a == name);

    let (width, height) = match value("--size") {
        Some(s) => {
            let (w, h) = s.split_once('x').ok_or("--size looks like 1600x1000")?;
            (w.parse::<f32>().map_err(|_| "bad width")?, h.parse::<f32>().map_err(|_| "bad height")?)
        }
        None => (1600.0, 1000.0),
    };

    let bytes = std::fs::read(&capture).map_err(|e| format!("cannot read {capture}: {e}"))?;
    let mut app = ReplayApp::new();
    app.open_bytes(capture.rsplit(['/', '\\']).next().unwrap_or(&capture), &bytes);
    let replay = app.replay().ok_or("the capture did not load")?;
    let id_of = |name: &str| -> Result<i32, String> {
        replay
            .entities()
            .find(|e| e.name == name || e.id.to_string() == name)
            .map(|e| e.id)
            .ok_or_else(|| format!("no entity named {name}"))
    };
    let pins: Vec<i32> = values("--pin").iter().map(|n| id_of(n)).collect::<Result<_, _>>()?;
    let select = value("--select").map(|n| id_of(&n)).transpose()?;

    for id in pins {
        app.toggle_pin(id);
    }
    if let Some(id) = select {
        app.select(Some(id));
    }
    if let Some(frame) = value("--frame") {
        app.seek_frame(frame.parse().map_err(|_| "--frame needs a number")?);
    }
    match value("--tab").as_deref() {
        Some("graphs") => app.set_bottom_tab(BottomTab::Graphs),
        Some("timelines") => app.set_bottom_tab(BottomTab::Timelines),
        Some("logs") | None => app.set_bottom_tab(BottomTab::Logs),
        Some(other) => return Err(format!("unknown tab {other}")),
    }
    if flag("--overlay") {
        app.graphs_mut().stacking.by_entity = false;
    }
    if flag("--flat") {
        app.timelines_mut().by_depth = false;
    }
    if flag("--changes-only") {
        app.timelines_mut().changes_only = true;
    }
    if flag("--clip") {
        app.graphs_mut().ignore_outliers = true;
    }
    if let Some(filter) = value("--filter") {
        app.graphs_mut().filter = filter.clone();
        app.timelines_mut().filter = filter;
    }
    let zoom = value("--zoom").map(|z| z.parse::<f64>().map_err(|_| "--zoom needs a number")).transpose()?;

    // The harness renderer and the app's 3D renderer share one device so the viewport shows up.
    let render_state = create_render_state(default_wgpu_setup());
    let renderer = WgpuTestRenderer::from_render_state(render_state.clone());
    app.attach_wgpu(render_state);

    let dark = flag("--dark");
    let mut harness = Harness::builder()
        .with_size(egui::vec2(width, height))
        .with_pixels_per_point(1.0)
        .renderer(renderer)
        .build_state(
            move |ctx, app: &mut ReplayApp| {
                ctx.set_visuals(if dark { egui::Visuals::dark() } else { egui::Visuals::light() });
                app.ui(ctx);
            },
            app,
        );
    for _ in 0..3 {
        harness.step();
    }
    if let Some(z) = zoom {
        let app = harness.state_mut();
        let t = app.playback().time();
        app.time_window_mut().zoom(1.0 / z, t);
    }
    for _ in 0..3 {
        harness.step();
    }
    if let Some(hover) = value("--hover") {
        let (fx, fy) = hover.split_once(',').ok_or("--hover looks like 0.6,0.5 (fractions of the graph area)")?;
        let (fx, fy): (f32, f32) = (fx.parse().map_err(|_| "bad --hover x")?, fy.parse().map_err(|_| "bad --hover y")?);
        let app = harness.state();
        let r =
            if app.bottom_tab() == BottomTab::Timelines { app.timelines().plot_rect() } else { app.graphs().rect() };
        let at = egui::pos2(r.left() + fx * r.width(), r.top() + fy * r.height());
        harness.event(egui::Event::PointerMoved(at));
        for _ in 0..3 {
            harness.step();
        }
    }
    let image = harness.render()?;
    image.save(&out).map_err(|e| format!("cannot write {out}: {e}"))?;
    let app = harness.state();
    let (what, (entities, rows)) = match app.bottom_tab() {
        BottomTab::Graphs => ("graphs", app.graphs().shown()),
        BottomTab::Timelines => ("timelines", app.timelines().shown()),
        BottomTab::Logs => ("logs", (0, 0)),
    };
    println!("wrote {out} ({}x{}); {what}: {entities} entities, {rows} rows", image.width(), image.height());
    Ok(())
}
