//! The 3D viewport panel: toolbar, camera interaction, picking, overlays, and showing the image the
//! GPU renderer produces.

use egui::{Align2, Color32, FontId, PointerButton, Pos2, Rect, RichText, Sense, Stroke, TextureId, Vec2};
use vrd_model::Replay;
use vrd_render::{RenderParams, Renderer};
use vrd_scene::glam::{self, Vec3};
use vrd_scene::{
    Axis, Camera, FrameRequest, SceneAssets, SceneFrame, ViewOptions, build_frame, default_camera_forward,
};

use crate::theme::text_color;

const FOLLOW_FOV: f32 = 60.0;
const ORBIT_FOV: f32 = 45.0;

struct Gpu {
    state: egui_wgpu::RenderState,
    renderer: Renderer,
    /// The registered egui texture and the renderer generation it was registered for.
    texture: Option<(TextureId, u64)>,
}

/// State of the 3D viewport.
pub struct Viewport {
    pub options: ViewOptions,
    camera: Camera,
    assets: SceneAssets,
    gpu: Option<Gpu>,
    has_replay: bool,
    /// Where the 3D view was last drawn, in points.
    rect: Rect,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            options: ViewOptions::default(),
            camera: Camera::default(),
            assets: SceneAssets::default(),
            gpu: None,
            has_replay: false,
            rect: Rect::NOTHING,
        }
    }
}

/// What the viewport asks the app to do.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ViewportAction {
    pub select: Option<i32>,
}

impl Viewport {
    /// Enables rendering. Without a GPU the viewport still builds its scene but shows a notice.
    pub fn attach_gpu(&mut self, state: egui_wgpu::RenderState) {
        let mut renderer = Renderer::new(&state.device, &state.queue);
        renderer.set_assets(&self.assets);
        self.gpu = Some(Gpu { state, renderer, texture: None });
    }

    pub fn has_gpu(&self) -> bool {
        self.gpu.is_some()
    }

    pub fn rect(&self) -> Rect {
        self.rect
    }

    pub fn camera(&self) -> &Camera {
        &self.camera
    }

    /// Prepares the viewport for a newly loaded capture.
    pub fn set_replay(&mut self, replay: &Replay) {
        self.assets = SceneAssets::build(replay);
        if let Some(gpu) = &mut self.gpu {
            gpu.renderer.set_assets(&self.assets);
        }
        self.options.hidden_draw_categories.clear();
        self.options.hidden_colors.clear();
        self.options.hidden_entities.clear();
        self.options.follow_selection = false;
        self.options.follow_camera = self.assets.camera_entity().is_some();
        self.options.up = None;
        self.options.camera_forward = None;
        self.camera = Camera { up_axis: self.assets.detected_up(), ..Camera::default() };
        let (center, radius) = vrd_scene::frame_all(replay, &self.assets, 0);
        self.camera.focus(center, radius);
        self.has_replay = true;
    }

    /// Moves the camera to look at an entity.
    pub fn focus(&mut self, replay: &Replay, entity: i32, frame: i32) {
        if let Some((center, radius)) = vrd_scene::focus_target(replay, &self.assets, entity, frame) {
            self.camera.fov_y = ORBIT_FOV.to_radians();
            self.camera.focus(center, radius);
            self.options.follow_camera = false;
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui, replay: &Replay, frame: i32, selected: Option<i32>) -> ViewportAction {
        let mut action = ViewportAction::default();
        self.toolbar(ui, replay, frame, selected);

        let size = ui.available_size().max(Vec2::splat(16.0));
        let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
        self.rect = rect;
        let ppp = ui.pixels_per_point();

        self.apply_up_axis(replay, frame);
        self.interact(ui, &response, replay, frame, selected);
        self.apply_follow(replay, frame, selected);

        let scene = build_frame(&FrameRequest {
            replay,
            assets: &self.assets,
            frame,
            selected,
            options: &self.options,
            camera: &self.camera,
            dark_mode: ui.visuals().dark_mode,
        });

        let viewport_px = glam::Vec2::new(rect.width() * ppp, rect.height() * ppp);
        if let Some(pos) = response.interact_pointer_pos().filter(|_| response.clicked() || response.double_clicked()) {
            let cursor = to_glam((pos - rect.min) * ppp);
            if let Some(hit) = vrd_scene::pick(&self.camera, viewport_px, cursor, &scene.pickables) {
                action.select = Some(hit);
                if response.double_clicked() {
                    self.focus(replay, hit, frame);
                }
            }
        }

        self.present(ui, rect, ppp, &scene);
        self.overlay(ui, rect, ppp, viewport_px, &scene);
        action
    }

    fn toolbar(&mut self, ui: &mut egui::Ui, replay: &Replay, frame: i32, selected: Option<i32>) {
        let has_camera = self.assets.camera_entity().is_some();
        ui.horizontal_wrapped(|ui| {
            let o = &mut self.options;
            let was_following = o.follow_camera;
            ui.add_enabled_ui(has_camera, |ui| {
                ui.toggle_value(&mut o.follow_camera, "Camera").on_hover_text(if has_camera {
                    "Follow the main camera"
                } else {
                    "This capture has no entity named \"maincamera\""
                });
            });
            if o.follow_camera && !was_following {
                o.follow_selection = false;
            }
            let was_following_selection = o.follow_selection;
            ui.toggle_value(&mut o.follow_selection, "Follow")
                .on_hover_text("Keep the selected entity in the centre of the view");
            if o.follow_selection && !was_following_selection {
                o.follow_camera = false;
            }
            if ui
                .add_enabled(selected.is_some(), egui::Button::new("Focus"))
                .on_hover_text("Focus on the selection (F, or double-click an entity)")
                .clicked()
                && let Some(id) = selected
            {
                self.focus(replay, id, frame);
            }
            let o = &mut self.options;

            ui.separator();
            ui.toggle_value(&mut o.show_all_names, "Names").on_hover_text("Show every entity's name");
            ui.toggle_value(&mut o.show_all_paths, "Paths").on_hover_text("Show every entity's path");
            ui.separator();
            ui.toggle_value(&mut o.show_draws, "Draws").on_hover_text("Show per-frame draw primitives");
            ui.toggle_value(&mut o.show_entity_geometry, "Geometry").on_hover_text("Show entity geometry");
            ui.toggle_value(&mut o.show_entity_circles, "Circles").on_hover_text("Circle each entity position");
            ui.toggle_value(&mut o.show_entity_axes, "Axes").on_hover_text("Show entity axes");
            ui.toggle_value(&mut o.semitransparent, "Ghost").on_hover_text("Draw primitives semi-transparent");
            ui.toggle_value(&mut o.show_grid, "Grid").on_hover_text("Show the ground grid");
            ui.separator();

            ui.menu_button("Draw categories", |ui| {
                category_menu(
                    ui,
                    "categories",
                    replay.draw_categories().iter().map(|c| (c.as_str(), c)),
                    &mut o.hidden_draw_categories,
                )
            });
            ui.menu_button("Draw colors", |ui| {
                let dark = ui.visuals().dark_mode;
                ui.horizontal(|ui| {
                    if ui.small_button("All").clicked() {
                        o.hidden_colors.clear();
                    }
                    if ui.small_button("None").clicked() {
                        o.hidden_colors = replay.draw_colors().clone();
                    }
                });
                ui.separator();
                for &color in replay.draw_colors() {
                    let mut shown = !o.hidden_colors.contains(&color);
                    let name = color.name().unwrap_or("?");
                    if ui.checkbox(&mut shown, RichText::new(name).color(text_color(color, dark))).changed() {
                        if shown {
                            o.hidden_colors.remove(&color);
                        } else {
                            o.hidden_colors.insert(color);
                        }
                    }
                }
            });
            ui.menu_button("View", |ui| {
                ui.horizontal(|ui| {
                    ui.label("Axis length");
                    let mut auto = o.axis_length.is_none();
                    if ui.checkbox(&mut auto, "auto").changed() {
                        o.axis_length = if auto { None } else { Some(self.assets.unit() * 1.5) };
                    }
                    if let Some(length) = &mut o.axis_length {
                        let speed = *length * 0.02;
                        ui.add(egui::DragValue::new(length).speed(speed).range(1.0e-4..=1.0e9));
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Draw history (frames)");
                    ui.add(egui::DragValue::new(&mut o.draw_history_frames).speed(0.2).range(0..=10_000));
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Up axis");
                    let detected = self.assets.detected_up();
                    egui::ComboBox::from_id_salt("up_axis")
                        .selected_text(match o.up {
                            None => format!("Auto ({})", detected.name()),
                            Some(a) => a.name().to_string(),
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut o.up, None, format!("Auto ({})", detected.name()));
                            for axis in Axis::ALL {
                                ui.selectable_value(&mut o.up, Some(axis), axis.name());
                            }
                        });
                });
                ui.horizontal(|ui| {
                    ui.label("Camera looks along");
                    let auto = default_camera_forward(self.camera.up_axis);
                    egui::ComboBox::from_id_salt("camera_forward")
                        .selected_text(
                            o.camera_forward.map_or_else(|| format!("Auto ({})", axis_name(auto)), axis_name),
                        )
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut o.camera_forward, None, format!("Auto ({})", axis_name(auto)));
                            for dir in [Vec3::X, Vec3::NEG_X, Vec3::Y, Vec3::NEG_Y, Vec3::Z, Vec3::NEG_Z] {
                                ui.selectable_value(&mut o.camera_forward, Some(dir), axis_name(dir));
                            }
                        });
                });
                ui.separator();
                if ui.button("Show all entities").clicked() {
                    o.hidden_entities.clear();
                    ui.close();
                }
                if ui.button("Frame everything").clicked() {
                    let (center, radius) = vrd_scene::frame_all(replay, &self.assets, frame);
                    o.follow_camera = false;
                    self.camera.fov_y = ORBIT_FOV.to_radians();
                    self.camera.focus(center, radius);
                    ui.close();
                }
            });
        });
    }

    /// Mouse and keyboard camera control.
    fn interact(
        &mut self,
        ui: &egui::Ui,
        response: &egui::Response,
        replay: &Replay,
        frame: i32,
        selected: Option<i32>,
    ) {
        let mut moved = false;
        let delta = response.drag_delta();
        if delta != Vec2::ZERO {
            if response.dragged_by(PointerButton::Primary) {
                self.camera.orbit(delta.x, delta.y);
                moved = true;
            } else if response.dragged_by(PointerButton::Secondary) || response.dragged_by(PointerButton::Middle) {
                self.camera.pan(delta.x, delta.y, response.rect.height());
                moved = true;
            }
        }
        if response.hovered() {
            let (scroll, pinch) = ui.input(|i| (i.smooth_scroll_delta.y, i.zoom_delta()));
            if scroll != 0.0 {
                self.camera.zoom(scroll);
                moved = true;
            }
            if pinch != 1.0 {
                self.camera.distance = (self.camera.distance / pinch).clamp(1.0, 1.0e9);
                moved = true;
            }
            if ui.input(|i| i.key_pressed(egui::Key::F))
                && let Some(id) = selected
            {
                self.focus(replay, id, frame);
            }
        }
        if moved && self.options.follow_camera {
            self.options.follow_camera = false;
            self.camera.fov_y = ORBIT_FOV.to_radians();
        }
    }

    /// Keeps the camera's up axis in step with the option (or the detected axis), reframing when it changes.
    fn apply_up_axis(&mut self, replay: &Replay, frame: i32) {
        let up = self.options.up.unwrap_or_else(|| self.assets.detected_up());
        if self.camera.up_axis != up {
            self.camera.up_axis = up;
            if !self.options.follow_camera {
                let (center, radius) = vrd_scene::frame_all(replay, &self.assets, frame);
                self.camera.focus(center, radius);
            }
        }
    }

    fn apply_follow(&mut self, replay: &Replay, frame: i32, selected: Option<i32>) {
        if self.options.follow_camera {
            let forward = self.options.camera_forward.unwrap_or_else(|| default_camera_forward(self.camera.up_axis));
            if let Some((position, look)) = vrd_scene::follow_pose(replay, &self.assets, frame, forward) {
                self.camera.fov_y = FOLLOW_FOV.to_radians();
                self.camera.set_pose(position, look, self.assets.unit());
            }
        } else if self.options.follow_selection
            && let Some(p) = selected.and_then(|id| replay.position_at(id, frame))
        {
            self.camera.target = Vec3::new(p.x, p.y, p.z);
        }
    }

    /// Renders the scene on the GPU and paints the result into `rect`.
    fn present(&mut self, ui: &egui::Ui, rect: Rect, ppp: f32, scene: &SceneFrame) {
        let dark = ui.visuals().dark_mode;
        let Some(gpu) = &mut self.gpu else {
            ui.painter().rect_filled(rect, 0.0, if dark { Color32::from_gray(28) } else { Color32::from_gray(211) });
            ui.painter().text(
                rect.center(),
                Align2::CENTER_CENTER,
                "3D rendering is not available (no GPU renderer)",
                FontId::proportional(16.0),
                ui.visuals().weak_text_color(),
            );
            return;
        };

        let size = [(rect.width() * ppp).round().max(1.0) as u32, (rect.height() * ppp).round().max(1.0) as u32];
        let background =
            if dark { vrd_scene::linear_rgba(28, 28, 28, 1.0) } else { vrd_scene::linear_rgba(211, 211, 211, 1.0) };
        gpu.renderer.render(
            scene,
            &RenderParams {
                size,
                view_proj: self.camera.view_proj(size[0] as f32 / size[1] as f32),
                eye: self.camera.eye(),
                near: self.camera.near(),
                clear: background.map(f64::from),
            },
        );

        let Some(view) = gpu.renderer.output() else {
            return;
        };
        let generation = gpu.renderer.generation();
        let mut egui_renderer = gpu.state.renderer.write();
        match &mut gpu.texture {
            Some((_, registered)) if *registered == generation => {}
            Some((id, registered)) => {
                egui_renderer.update_egui_texture_from_wgpu_texture(&gpu.state.device, view, wgpu_filter(), *id);
                *registered = generation;
            }
            None => {
                let id = egui_renderer.register_native_texture(&gpu.state.device, view, wgpu_filter());
                gpu.texture = Some((id, generation));
            }
        }
        if let Some((id, _)) = gpu.texture {
            ui.painter().image(id, rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
        }
    }

    /// Screen-space decorations: entity circles, names and an orientation gizmo.
    fn overlay(&self, ui: &egui::Ui, rect: Rect, ppp: f32, viewport_px: glam::Vec2, scene: &SceneFrame) {
        let painter = ui.painter_at(rect);
        let dark = ui.visuals().dark_mode;
        let project = |p: Vec3| {
            self.camera.project(p, viewport_px).map(|proj| rect.min + Vec2::new(proj.screen.x, proj.screen.y) / ppp)
        };

        for marker in &scene.markers {
            if let Some(at) = project(marker.position) {
                let (color, radius) = if marker.selected {
                    (Color32::from_rgb(235, 40, 40), 9.0)
                } else {
                    (Color32::from_rgb(40, 90, 235), 7.0)
                };
                painter.circle_stroke(at, radius, Stroke::new(if marker.selected { 2.0_f32 } else { 1.5_f32 }, color));
            }
        }

        let (fg, bg) = if dark {
            (Color32::from_gray(235), Color32::from_black_alpha(150))
        } else {
            (Color32::from_gray(20), Color32::from_white_alpha(170))
        };
        for label in &scene.labels {
            let Some(at) = project(label.position) else {
                continue;
            };
            let color = if label.selected { Color32::from_rgb(235, 60, 60) } else { fg };
            let galley = painter.layout_no_wrap(label.text.clone(), FontId::proportional(13.0), color);
            let origin = at + Vec2::new(10.0, -10.0 - galley.size().y);
            painter.rect_filled(Rect::from_min_size(origin, galley.size()).expand(2.0), 3.0, bg);
            painter.galley(origin, galley, color);
        }

        // A small axis gizmo in the corner shows how the world is oriented.
        let gizmo = rect.left_bottom() + Vec2::new(44.0, -44.0);
        let (right, up, forward) = (self.camera.right(), self.camera.up(), self.camera.forward());
        for (axis, color, name) in [
            (Vec3::X, Color32::from_rgb(230, 50, 50), "X"),
            (Vec3::Y, Color32::from_rgb(40, 190, 40), "Y"),
            (Vec3::Z, Color32::from_rgb(60, 100, 255), "Z"),
        ] {
            let end = gizmo + Vec2::new(axis.dot(right), -axis.dot(up)) * 30.0;
            // Axes pointing away from the viewer are drawn fainter.
            let facing = (-axis.dot(forward)).clamp(-1.0, 1.0) * 0.5 + 0.5;
            let color = color.gamma_multiply(0.45 + 0.55 * facing);
            painter.line_segment([gizmo, end], Stroke::new(2.0_f32, color));
            let away = end - gizmo;
            let away = if away.length() > 0.5 { away.normalized() } else { Vec2::ZERO };
            painter.text(end + away * 8.0, Align2::CENTER_CENTER, name, FontId::proportional(11.0), color);
        }
    }
}

/// "+X", "-Z", ... for a unit axis vector.
fn axis_name(v: Vec3) -> String {
    let (sign, name) = [(Vec3::X, "X"), (Vec3::Y, "Y"), (Vec3::Z, "Z")]
        .into_iter()
        .find_map(|(axis, name)| match v.dot(axis) {
            d if d > 0.5 => Some(("+", name)),
            d if d < -0.5 => Some(("-", name)),
            _ => None,
        })
        .unwrap_or(("", "?"));
    format!("{sign}{name}")
}

fn to_glam(v: Vec2) -> glam::Vec2 {
    glam::Vec2::new(v.x, v.y)
}

fn wgpu_filter() -> vrd_render::wgpu::FilterMode {
    vrd_render::wgpu::FilterMode::Linear
}

/// A checkbox list of named items that can be hidden, with All/None buttons.
fn category_menu<'a>(
    ui: &mut egui::Ui,
    id: &str,
    items: impl Iterator<Item = (&'a str, &'a String)> + Clone,
    hidden: &mut std::collections::BTreeSet<String>,
) {
    ui.horizontal(|ui| {
        if ui.small_button("All").clicked() {
            hidden.clear();
        }
        if ui.small_button("None").clicked() {
            hidden.extend(items.clone().map(|(_, k)| k.clone()));
        }
    });
    ui.separator();
    egui::ScrollArea::vertical().id_salt(id).max_height(320.0).show(ui, |ui| {
        let mut any = false;
        for (label, key) in items {
            any = true;
            let mut shown = !hidden.contains(key);
            let text = if label.is_empty() { "(none)" } else { label };
            if ui.checkbox(&mut shown, text).changed() {
                if shown {
                    hidden.remove(key);
                } else {
                    hidden.insert(key.clone());
                }
            }
        }
        if !any {
            ui.weak("This capture has no draw categories.");
        }
    });
}
