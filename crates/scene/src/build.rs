//! Turns a [`Replay`] at a given frame into drawable geometry.
//!
//! This is the CPU half of the 3D viewport. It has no GPU dependency, so everything here is unit
//! tested. The renderer in `vrd-render` consumes [`SceneAssets`] (uploaded once per capture) and
//! [`SceneFrame`] (rebuilt every frame).

use std::collections::{BTreeMap, BTreeSet};

use glam::{Mat4, Vec2, Vec3};
use vrd_model::{Color, Replay, Shape};

use crate::camera::{Axis, Camera};
use crate::mesh::{self, LineSegment, Vertex, circle_segments, linear_color, linear_rgba, to_vec3, transform_matrix};

/// Opacity of "semitransparent draws", matching the original tool (alpha 100 of 255).
const TRANSLUCENT_ALPHA: f32 = 100.0 / 255.0;
const HIGHLIGHT: [f32; 4] = [1.0, 0.45, 0.05, 0.45];

/// What the viewport shows.
#[derive(Debug, Clone, PartialEq)]
pub struct ViewOptions {
    pub follow_camera: bool,
    pub follow_selection: bool,
    pub show_all_names: bool,
    pub show_all_paths: bool,
    pub show_draws: bool,
    pub show_entity_geometry: bool,
    pub show_entity_circles: bool,
    pub show_entity_axes: bool,
    pub semitransparent: bool,
    pub show_grid: bool,
    /// Also show draws issued up to this many frames before the current one.
    pub draw_history_frames: u32,
    /// Which world axis is up; `None` picks the axis the capture's positions vary least along.
    pub up: Option<Axis>,
    /// The local axis cameras (the `maincamera` entity) look along; `None` follows the up axis's convention.
    pub camera_forward: Option<Vec3>,
    /// Length of the entity axes in world units; `None` picks a length from the capture's scale.
    pub axis_length: Option<f32>,
    pub hidden_draw_categories: BTreeSet<String>,
    pub hidden_colors: BTreeSet<Color>,
    pub hidden_entities: BTreeSet<i32>,
}

impl Default for ViewOptions {
    fn default() -> Self {
        Self {
            follow_camera: false,
            follow_selection: false,
            show_all_names: false,
            show_all_paths: false,
            show_draws: true,
            show_entity_geometry: true,
            show_entity_circles: true,
            show_entity_axes: true,
            semitransparent: false,
            show_grid: true,
            draw_history_frames: 0,
            up: None,
            camera_forward: None,
            axis_length: None,
            hidden_draw_categories: BTreeSet::new(),
            hidden_colors: BTreeSet::new(),
            hidden_entities: BTreeSet::new(),
        }
    }
}

/// Geometry attached to an entity at registration, in the entity's local space.
#[derive(Debug, Clone)]
pub struct StaticModel {
    pub entity: i32,
    pub color: Color,
    pub vertices: Vec<Vertex>,
    pub center: Vec3,
    pub radius: f32,
}

#[derive(Debug, Clone)]
enum CreationLine {
    Segment { a: Vec3, b: Vec3, color: Color },
    Circle { center: Vec3, normal: Vec3, radius: f32, color: Color },
}

/// Everything that is fixed for the lifetime of a loaded capture.
#[derive(Debug, Clone, Default)]
pub struct SceneAssets {
    pub models: Vec<StaticModel>,
    models_by_entity: BTreeMap<i32, Vec<usize>>,
    lines_by_entity: BTreeMap<i32, Vec<CreationLine>>,
    camera_entity: Option<i32>,
    unit: f32,
    detected_up: Axis,
    /// Bounds of every recorded entity position.
    position_bounds: Option<(Vec3, Vec3)>,
}

impl SceneAssets {
    pub fn build(replay: &Replay) -> Self {
        let mut assets = SceneAssets {
            camera_entity: replay.entities().find(|e| e.name == "maincamera").map(|e| e.id),
            ..SceneAssets::default()
        };

        for entity in replay.entities() {
            let draws: Vec<_> = replay.creation_draws(entity.id).collect();
            if draws.is_empty() {
                continue;
            }
            let to_local = registration_space(entity.initial_transform, &draws);

            for draw in draws {
                let color = linear_color(draw.color, 1.0);
                let mut vertices = Vec::new();
                match &draw.shape {
                    Shape::Sphere { center, radius } => mesh::sphere(&mut vertices, to_vec3(*center), *radius, color),
                    Shape::Capsule { p1, p2, radius } => {
                        mesh::capsule(&mut vertices, to_vec3(*p1), to_vec3(*p2), *radius, color)
                    }
                    Shape::Box { transform, dimensions } => {
                        mesh::cuboid(&mut vertices, transform, to_vec3(*dimensions), color)
                    }
                    Shape::Mesh { verts } => {
                        let points: Vec<Vec3> = verts.iter().map(|p| to_vec3(*p)).collect();
                        mesh::triangle_soup(&mut vertices, &points, color);
                    }
                    Shape::Line { start, end } => {
                        assets.lines_by_entity.entry(entity.id).or_default().push(CreationLine::Segment {
                            a: to_local.transform_point3(to_vec3(*start)),
                            b: to_local.transform_point3(to_vec3(*end)),
                            color: draw.color,
                        });
                        continue;
                    }
                    Shape::Circle { position, up, radius } => {
                        assets.lines_by_entity.entry(entity.id).or_default().push(CreationLine::Circle {
                            center: to_local.transform_point3(to_vec3(*position)),
                            normal: to_local.transform_vector3(to_vec3(*up)),
                            radius: *radius,
                            color: draw.color,
                        });
                        continue;
                    }
                }
                if vertices.is_empty() {
                    continue;
                }
                mesh::transform_vertices(&mut vertices, to_local);
                let (center, radius) = bounding_sphere(vertices.iter().map(|v| Vec3::from(v.position)));
                assets.models_by_entity.entry(entity.id).or_default().push(assets.models.len());
                assets.models.push(StaticModel { entity: entity.id, color: draw.color, vertices, center, radius });
            }
        }
        assets.unit = typical_size(replay, &assets);
        assets.position_bounds = position_bounds(replay);
        assets.detected_up = assets.position_bounds.map_or(Axis::Z, |(lo, hi)| detect_up(hi - lo));
        assets
    }

    /// The up axis guessed from the data: entities mostly move across a ground plane, so the axis
    /// with the smallest spread of positions is taken to be vertical.
    pub fn detected_up(&self) -> Axis {
        self.detected_up
    }

    /// Where a ground grid should sit along `up`: the origin, unless the recorded positions lie
    /// well away from it, in which case the lowest position.
    pub fn ground_height(&self, up: Axis) -> f32 {
        let Some((lo, hi)) = self.position_bounds else {
            return 0.0;
        };
        let (lo, hi) = (lo.dot(up.vector()), hi.dot(up.vector()));
        let margin = self.unit() * 5.0;
        if lo - margin <= 0.0 && hi + margin >= 0.0 { 0.0 } else { lo }
    }

    /// A length that is typical for this capture's entities (a character's size, say). The
    /// capture format has no units, so this is what scales default axes, focus distances and so on.
    pub fn unit(&self) -> f32 {
        if self.unit > 0.0 { self.unit } else { 1.0 }
    }

    /// The entity named `maincamera`, which the viewport can follow.
    pub fn camera_entity(&self) -> Option<i32> {
        self.camera_entity
    }

    /// Radius, around the entity's origin, that contains its registration geometry.
    pub fn entity_radius(&self, entity: i32) -> f32 {
        self.models_by_entity
            .get(&entity)
            .into_iter()
            .flatten()
            .map(|&i| self.models[i].center.length() + self.models[i].radius)
            .fold(0.0, f32::max)
    }
}

/// A representative point of a shape, used to tell which space its coordinates are in.
fn anchor(shape: &Shape) -> Vec3 {
    match shape {
        Shape::Sphere { center, .. } => to_vec3(*center),
        Shape::Capsule { p1, p2, .. } => (to_vec3(*p1) + to_vec3(*p2)) * 0.5,
        Shape::Box { transform, .. } => to_vec3(transform.translation),
        Shape::Line { start, end } => (to_vec3(*start) + to_vec3(*end)) * 0.5,
        Shape::Circle { position, .. } => to_vec3(*position),
        Shape::Mesh { verts } if verts.is_empty() => Vec3::ZERO,
        Shape::Mesh { verts } => verts.iter().map(|p| to_vec3(*p)).sum::<Vec3>() / verts.len() as f32,
    }
}

/// The matrix that turns an entity's registration geometry into entity-local coordinates.
///
/// Captures disagree about this: some record registration draws relative to the entity, others
/// (such as the sample capture) in world space at the moment of registration. Geometry that sits
/// around the entity's initial position, and not around the origin, is taken to be in world space
/// and is brought into the entity's frame so that it follows the entity when it moves.
fn registration_space(initial: vrd_model::Transform, draws: &[&vrd_model::DrawCommand]) -> Mat4 {
    let initial = transform_matrix(&initial);
    let origin = initial.transform_point3(Vec3::ZERO);
    let centroid = draws.iter().map(|d| anchor(&d.shape)).sum::<Vec3>() / draws.len() as f32;
    if origin.distance(centroid) < centroid.length() { initial.inverse() } else { Mat4::IDENTITY }
}

fn position_bounds(replay: &Replay) -> Option<(Vec3, Vec3)> {
    let mut bounds: Option<(Vec3, Vec3)> = None;
    for e in replay.entities() {
        for t in e.transforms.values() {
            let p = to_vec3(t.translation);
            if !p.is_finite() {
                continue;
            }
            bounds = Some(bounds.map_or((p, p), |(lo, hi)| (lo.min(p), hi.max(p))));
        }
    }
    bounds
}

/// The axis with the smallest extent. Near-ties go to Z, then Y, so ambiguous scenes keep the
/// original tool's convention.
fn detect_up(extent: Vec3) -> Axis {
    let mut best = Axis::Z;
    for axis in [Axis::Y, Axis::X] {
        if extent.dot(axis.vector()) < extent.dot(best.vector()) * 0.9 {
            best = axis;
        }
    }
    best
}

/// The median size of entity geometry, or failing that a twentieth of the spread of positions.
fn typical_size(replay: &Replay, assets: &SceneAssets) -> f32 {
    let mut sizes: Vec<f32> = assets
        .models_by_entity
        .keys()
        .map(|&id| assets.entity_radius(id))
        .filter(|r| r.is_finite() && *r > 0.0)
        .collect();
    sizes.sort_by(f32::total_cmp);
    let size = sizes.get(sizes.len() / 2).copied().or_else(|| {
        let positions: Vec<Vec3> =
            replay.entities().flat_map(|e| e.transforms.values().iter().map(|t| to_vec3(t.translation))).collect();
        let (center, radius) = bounding_sphere(positions.iter().copied());
        (radius > 0.0).then_some(radius * 2.0 / 20.0).filter(|_| center.is_finite())
    });
    size.filter(|s| s.is_finite() && *s > 0.0).unwrap_or(1.0).clamp(1.0e-3, 1.0e6)
}

fn bounding_sphere(points: impl Iterator<Item = Vec3> + Clone) -> (Vec3, f32) {
    let (lo, hi) =
        points.clone().fold((Vec3::splat(f32::MAX), Vec3::splat(f32::MIN)), |(lo, hi), p| (lo.min(p), hi.max(p)));
    if lo.x > hi.x {
        return (Vec3::ZERO, 0.0);
    }
    let center = (lo + hi) * 0.5;
    let radius = points.map(|p| p.distance(center)).fold(0.0, f32::max);
    (center, radius)
}

/// One use of a [`StaticModel`] this frame.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelInstance {
    pub model: usize,
    pub matrix: Mat4,
    /// Multiplies the model's colour; alpha below 1 puts the instance in the translucent pass.
    pub tint: [f32; 4],
    /// RGB colour to blend towards, with the blend amount in alpha.
    pub mix: [f32; 4],
}

/// A screen-space circle at an entity position.
#[derive(Debug, Clone, PartialEq)]
pub struct Marker {
    pub position: Vec3,
    pub selected: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Label {
    pub position: Vec3,
    pub text: String,
    pub selected: bool,
}

/// An entity that can be picked with the mouse.
#[derive(Debug, Clone, PartialEq)]
pub struct Pickable {
    pub entity: i32,
    pub position: Vec3,
    pub radius: f32,
}

/// Everything to draw for one frame.
#[derive(Debug, Clone, Default)]
pub struct SceneFrame {
    /// Opaque triangles in world space.
    pub solid: Vec<Vertex>,
    /// Alpha-blended triangles in world space.
    pub translucent: Vec<Vertex>,
    pub lines: Vec<LineSegment>,
    pub instances: Vec<ModelInstance>,
    pub markers: Vec<Marker>,
    pub labels: Vec<Label>,
    pub pickables: Vec<Pickable>,
}

pub struct FrameRequest<'a> {
    pub replay: &'a Replay,
    pub assets: &'a SceneAssets,
    pub frame: i32,
    pub selected: Option<i32>,
    pub options: &'a ViewOptions,
    pub camera: &'a Camera,
    pub dark_mode: bool,
}

const AXIS_X: [f32; 4] = [1.0, 0.0, 0.0, 1.0];
const AXIS_Y: [f32; 4] = [0.0, 0.8, 0.0, 1.0];
const AXIS_Z: [f32; 4] = [0.1, 0.2, 1.0, 1.0];

pub fn build_frame(req: &FrameRequest) -> SceneFrame {
    let FrameRequest { replay, assets, frame, selected, options, .. } = *req;
    let mut out = SceneFrame::default();

    if options.show_grid {
        grid(req.camera, req.dark_mode, assets.unit(), assets.ground_height(req.camera.up_axis), &mut out.lines);
    }

    for entity in replay.entities() {
        let id = entity.id;
        let is_selected = selected == Some(id);
        let has_transforms = !entity.transforms.is_empty();
        let alive = !has_transforms || replay.is_alive_at(id, frame);
        let visible = !options.hidden_entities.contains(&id);

        // Entities that never move keep their geometry in world space.
        let matrix = if has_transforms {
            replay.transform_at(id, frame).map_or(Mat4::IDENTITY, |t| transform_matrix(&t))
        } else {
            Mat4::IDENTITY
        };

        if has_transforms && (options.show_all_paths || is_selected) && entity.creation_frame <= frame {
            path(replay, id, frame, is_selected, &mut out.lines);
        }

        if !alive || !(visible || is_selected) {
            continue;
        }

        if options.show_entity_geometry && visible {
            for &model in assets.models_by_entity.get(&id).into_iter().flatten() {
                let m = &assets.models[model];
                if options.hidden_colors.contains(&m.color) {
                    continue;
                }
                out.instances.push(ModelInstance {
                    model,
                    matrix,
                    tint: [1.0, 1.0, 1.0, if options.semitransparent { TRANSLUCENT_ALPHA } else { 1.0 }],
                    mix: if is_selected { HIGHLIGHT } else { [0.0; 4] },
                });
            }
        }

        if visible {
            for line in assets.lines_by_entity.get(&id).into_iter().flatten() {
                match line {
                    CreationLine::Segment { a, b, color } => out.lines.push(LineSegment::new(
                        matrix.transform_point3(*a),
                        matrix.transform_point3(*b),
                        linear_color(*color, 1.0),
                        2.0,
                    )),
                    CreationLine::Circle { center, normal, radius, color } => circle_segments(
                        matrix.transform_point3(*center),
                        matrix.transform_vector3(*normal),
                        *radius,
                        linear_color(*color, 1.0),
                        2.0,
                        &mut out.lines,
                    ),
                }
            }
        }

        if !has_transforms {
            continue;
        }
        let position = matrix.transform_point3(Vec3::ZERO);

        if visible {
            out.pickables.push(Pickable { entity: id, position, radius: assets.entity_radius(id) });
            if options.show_entity_circles {
                out.markers.push(Marker { position, selected: is_selected });
            }
            if options.show_entity_axes {
                let l = options.axis_length.unwrap_or(assets.unit() * 1.5);
                for (dir, color) in [(Vec3::X, AXIS_X), (Vec3::Y, AXIS_Y), (Vec3::Z, AXIS_Z)] {
                    out.lines.push(LineSegment::new(position, matrix.transform_point3(dir * l), color, 2.0));
                }
            }
        }
        if options.show_all_names || is_selected {
            out.labels.push(Label { position, text: entity.display_name(), selected: is_selected });
        }
    }

    if options.show_draws {
        per_frame_draws(req, &mut out);
    }
    out
}

fn per_frame_draws(req: &FrameRequest, out: &mut SceneFrame) {
    let FrameRequest { replay, frame, options, .. } = *req;
    let start = frame.saturating_sub(options.draw_history_frames as i32).max(0);
    let alpha = if options.semitransparent { TRANSLUCENT_ALPHA } else { 1.0 };

    for d in replay.draws_in(start, frame) {
        if d.is_creation
            || d.color.rgba()[3] == 0
            || options.hidden_draw_categories.contains(&d.category)
            || options.hidden_colors.contains(&d.color)
            || options.hidden_entities.contains(&d.entity)
        {
            continue;
        }
        let color = linear_color(d.color, alpha);
        let tris = if options.semitransparent { &mut out.translucent } else { &mut out.solid };
        match &d.shape {
            Shape::Line { start, end } => {
                out.lines.push(LineSegment::new(to_vec3(*start), to_vec3(*end), linear_color(d.color, 1.0), 2.0))
            }
            Shape::Circle { position, up, radius } => circle_segments(
                to_vec3(*position),
                to_vec3(*up),
                *radius,
                linear_color(d.color, 1.0),
                2.0,
                &mut out.lines,
            ),
            Shape::Sphere { center, radius } => mesh::sphere(tris, to_vec3(*center), *radius, color),
            Shape::Capsule { p1, p2, radius } => mesh::capsule(tris, to_vec3(*p1), to_vec3(*p2), *radius, color),
            Shape::Box { transform, dimensions } => mesh::cuboid(tris, transform, to_vec3(*dimensions), color),
            Shape::Mesh { verts } => {
                let points: Vec<Vec3> = verts.iter().map(|p| to_vec3(*p)).collect();
                mesh::triangle_soup(tris, &points, color);
            }
        }
    }
}

/// An entity's trail. Segments after `frame` are faint, and only drawn for the selected entity.
fn path(replay: &Replay, id: i32, frame: i32, selected: bool, lines: &mut Vec<LineSegment>) {
    let Some(entity) = replay.entity(id) else {
        return;
    };
    let past = if selected { [1.0, 0.1, 0.1, 1.0] } else { linear_rgba(150, 150, 150, 0.8) };
    let future = [1.0, 0.1, 0.1, 0.25];
    let mut prev: Option<Vec3> = None;
    for (f, t) in entity.transforms.iter() {
        let p = to_vec3(t.translation);
        if let Some(a) = prev {
            if f <= frame {
                lines.push(LineSegment::new(a, p, past, if selected { 2.5 } else { 1.5 }));
            } else if selected {
                lines.push(LineSegment::new(a, p, future, 1.5));
            } else {
                break;
            }
        }
        prev = Some(p);
    }
}

/// A ground grid perpendicular to the camera's up axis, at height `ground`. Its spacing follows the
/// camera. The two horizontal world axes through the origin are drawn in their own colours.
pub fn grid(camera: &Camera, dark_mode: bool, unit: f32, ground: f32, out: &mut Vec<LineSegment>) {
    const HALF_LINES: i32 = 30;
    let up = camera.up_axis;
    let (h1, h2) = up.horizontal();
    let at = |a: f32, b: f32| h1 * a + h2 * b + up.vector() * ground;

    let height = (camera.eye().dot(up.vector()) - ground).abs();
    let scale = camera.distance.max(height).max(1.0e-3);
    let step = 10f32.powf((scale / 6.0).log10().floor()).max(unit * 1.0e-3).max(1.0e-4);
    let c1 = (camera.target.dot(h1) / step).round() * step;
    let c2 = (camera.target.dot(h2) / step).round() * step;
    let extent = HALF_LINES as f32 * step;

    let (minor, major) = if dark_mode {
        (linear_rgba(120, 120, 120, 0.25), linear_rgba(150, 150, 150, 0.5))
    } else {
        (linear_rgba(90, 90, 90, 0.25), linear_rgba(60, 60, 60, 0.5))
    };
    let cell1 = (c1 / step).round() as i64;
    let cell2 = (c2 / step).round() as i64;
    let color_for = |cell: i64| if cell % 10 == 0 { major } else { minor };
    for i in -HALF_LINES..=HALF_LINES {
        let offset = i as f32 * step;
        out.push(LineSegment::new(
            at(c1 + offset, c2 - extent),
            at(c1 + offset, c2 + extent),
            color_for(cell1 + i64::from(i)),
            1.0,
        ));
        out.push(LineSegment::new(
            at(c1 - extent, c2 + offset),
            at(c1 + extent, c2 + offset),
            color_for(cell2 + i64::from(i)),
            1.0,
        ));
    }

    let axis = extent.max(unit);
    let axis_color = |a: Axis| match a {
        Axis::X => [0.9, 0.1, 0.1, 0.8],
        Axis::Y => [0.1, 0.7, 0.1, 0.8],
        Axis::Z => [0.15, 0.3, 0.95, 0.8],
    };
    let horizontal_axes = Axis::ALL.into_iter().filter(|&a| a != up);
    for (axis_id, (along, across)) in horizontal_axes.zip([(h1, h2), (h2, h1)]) {
        let _ = across;
        out.push(LineSegment::new(at(0.0, 0.0) - along * axis, at(0.0, 0.0) + along * axis, axis_color(axis_id), 1.5));
    }
}

/// The pose of the `maincamera` entity at `frame`: position and the direction it faces, given the
/// local axis its engine considers "forward".
pub fn follow_pose(replay: &Replay, assets: &SceneAssets, frame: i32, forward: Vec3) -> Option<(Vec3, Vec3)> {
    let t = replay.transform_at(assets.camera_entity()?, frame)?;
    let m = transform_matrix(&t);
    Some((m.transform_point3(Vec3::ZERO), m.transform_vector3(forward).normalize_or_zero()))
}

/// Where to look to see `entity` at `frame`: its position and a radius that frames its geometry.
pub fn focus_target(replay: &Replay, assets: &SceneAssets, entity: i32, frame: i32) -> Option<(Vec3, f32)> {
    let position = to_vec3(replay.position_at(entity, frame)?);
    Some((position, assets.entity_radius(entity).max(assets.unit())))
}

/// A sphere containing every entity that has a position at `frame`.
pub fn frame_all(replay: &Replay, assets: &SceneAssets, frame: i32) -> (Vec3, f32) {
    let points: Vec<Vec3> = replay
        .entities()
        .filter(|e| !e.transforms.is_empty() && replay.is_alive_at(e.id, frame))
        .filter_map(|e| replay.position_at(e.id, frame))
        .map(to_vec3)
        .collect();
    let (center, radius) = bounding_sphere(points.iter().copied());
    (center, radius.max(assets.unit() * 5.0))
}

/// The entity under the cursor, if any. `cursor` and `viewport` are in pixels.
pub fn pick(camera: &Camera, viewport: Vec2, cursor: Vec2, pickables: &[Pickable]) -> Option<i32> {
    const MIN_RADIUS_PX: f32 = 12.0;
    let mut best: Option<(f32, f32, i32)> = None;
    for p in pickables {
        let Some(proj) = camera.project(p.position, viewport) else {
            continue;
        };
        let radius_px = (p.radius * camera.pixels_per_unit(proj.distance, viewport.y)).max(MIN_RADIUS_PX);
        let d = proj.screen.distance(cursor);
        if d > radius_px {
            continue;
        }
        // Prefer the closest to the cursor; break near ties with the one nearer the camera.
        let better = match best {
            None => true,
            Some((bd, bz, _)) => d < bd - 2.0 || (d <= bd + 2.0 && proj.distance < bz),
        };
        if better {
            best = Some((d, proj.distance, p.entity));
        }
    }
    best.map(|(_, _, id)| id)
}
