use vrd_ui_egui::ReplayApp;

struct DesktopApp(ReplayApp);

impl eframe::App for DesktopApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.0.ui(ctx);
    }
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default()
            .with_title("Visual Replay Debugger")
            .with_inner_size([1400.0, 850.0])
            .with_min_inner_size([800.0, 500.0])
            .with_drag_and_drop(true),
        ..eframe::NativeOptions::default()
    };

    let path = std::env::args_os().nth(1).map(std::path::PathBuf::from);

    eframe::run_native(
        "Visual Replay Debugger",
        options,
        Box::new(move |cc| {
            let mut app = ReplayApp::new();
            if let Some(state) = cc.wgpu_render_state.clone() {
                app.attach_wgpu(state);
            }
            if let Some(path) = &path {
                app.open_path(path);
            }
            Ok(Box::new(DesktopApp(app)))
        }),
    )
}
