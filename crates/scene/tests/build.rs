use vrd_format::{Color, EntityDef, EntityOp, Point, Quat, Record, Transform};
use vrd_model::{LoadInfo, Replay};
use vrd_scene::glam::{Vec2, Vec3};
use vrd_scene::{
    Axis, Camera, FrameRequest, SceneAssets, SceneFrame, ViewOptions, build_frame, focus_target, follow_pose,
    frame_all, pick,
};

fn ent(frame: i32, id: i32, op: EntityOp) -> Record {
    Record::Entity { frame, id, op }
}

fn define(id: i32, name: &str) -> Record {
    ent(0, id, EntityOp::Define { parent: None, def: EntityDef { id, name: name.into(), ..EntityDef::default() } })
}

fn at(x: f32, y: f32, z: f32) -> EntityOp {
    EntityOp::SetPosition(Point::new(x, y, z))
}

fn sphere(frame: i32, id: i32, category: &str, color: Color) -> Record {
    ent(frame, id, EntityOp::Sphere { category: category.into(), center: Point::ZERO, radius: 10.0, color })
}

fn step(t: f32) -> Record {
    Record::FrameStep { total_time: t }
}

fn build(records: Vec<Record>) -> (Replay, SceneAssets) {
    let replay = Replay::from_records(records, LoadInfo::default());
    let assets = SceneAssets::build(&replay);
    (replay, assets)
}

fn frame_of(
    replay: &Replay,
    assets: &SceneAssets,
    frame: i32,
    selected: Option<i32>,
    options: &ViewOptions,
) -> SceneFrame {
    build_frame(&FrameRequest {
        replay,
        assets,
        frame,
        selected,
        options,
        camera: &Camera::default(),
        dark_mode: false,
    })
}

fn no_grid() -> ViewOptions {
    ViewOptions { show_grid: false, ..ViewOptions::default() }
}

#[test]
fn creation_geometry_follows_the_entity_transform() {
    let (r, a) = build(vec![
        define(1, "Ball"),
        sphere(0, 1, "", Color::RED),
        at(100.0, 0.0, 0.0).into_record(0, 1),
        step(1.0),
        at(300.0, 0.0, 0.0).into_record(1, 1),
    ]);
    assert_eq!(a.models.len(), 1);
    let f0 = frame_of(&r, &a, 0, None, &no_grid());
    let f1 = frame_of(&r, &a, 1, None, &no_grid());
    assert_eq!(f0.instances.len(), 1);
    assert_eq!(f0.instances[0].matrix.transform_point3(Vec3::ZERO), Vec3::new(100.0, 0.0, 0.0));
    assert_eq!(f1.instances[0].matrix.transform_point3(Vec3::ZERO), Vec3::new(300.0, 0.0, 0.0));
    assert!(f0.solid.is_empty(), "registration geometry is an instance, not per-frame geometry");
}

trait IntoRecord {
    fn into_record(self, frame: i32, id: i32) -> Record;
}
impl IntoRecord for EntityOp {
    fn into_record(self, frame: i32, id: i32) -> Record {
        ent(frame, id, self)
    }
}

#[test]
fn entities_without_transforms_stay_in_world_space_and_always_show() {
    let (r, a) = build(vec![define(1, "Level"), sphere(0, 1, "", Color::GREEN), step(1.0), step(2.0)]);
    let f = frame_of(&r, &a, 2, None, &no_grid());
    assert_eq!(f.instances.len(), 1);
    assert_eq!(f.instances[0].matrix, vrd_scene::glam::Mat4::IDENTITY);
    assert!(f.markers.is_empty() && f.pickables.is_empty(), "no position, nothing to mark");
}

#[test]
fn destroyed_entities_disappear() {
    let (r, a) = build(vec![
        define(1, "Temp"),
        sphere(0, 1, "", Color::RED),
        at(0.0, 0.0, 0.0).into_record(0, 1),
        step(1.0),
        step(2.0),
        ent(2, 1, EntityOp::Undefine),
        step(3.0),
    ]);
    let alive = frame_of(&r, &a, 1, None, &no_grid());
    let gone = frame_of(&r, &a, 3, None, &no_grid());
    assert_eq!(alive.instances.len(), 1);
    assert_eq!(alive.markers.len(), 1);
    assert!(gone.instances.is_empty() && gone.markers.is_empty() && gone.pickables.is_empty());
}

#[test]
fn per_frame_draws_show_only_on_their_frame_unless_history_is_on() {
    let (r, a) = build(vec![
        define(1, "A"),
        sphere(0, 1, "Debug", Color::RED),
        step(1.0),
        step(2.0),
        sphere(2, 1, "Debug", Color::RED),
        step(3.0),
    ]);
    let o = no_grid();
    assert!(!frame_of(&r, &a, 0, None, &o).solid.is_empty());
    assert!(frame_of(&r, &a, 1, None, &o).solid.is_empty());
    let both = frame_of(&r, &a, 2, None, &ViewOptions { draw_history_frames: 5, ..no_grid() });
    let single = frame_of(&r, &a, 2, None, &o);
    assert_eq!(both.solid.len(), single.solid.len() * 2);
}

#[test]
fn draw_filters_apply() {
    let (r, a) = build(vec![
        define(1, "A"),
        define(2, "B"),
        sphere(0, 1, "Keep", Color::RED),
        sphere(0, 1, "Hide", Color::GREEN),
        sphere(0, 1, "Keep", Color::BLUE),
        sphere(0, 2, "Keep", Color::RED),
    ]);
    let count = |o: &ViewOptions| frame_of(&r, &a, 0, None, o).solid.len() / (12 * 24 * 6);
    assert_eq!(count(&no_grid()), 4);
    assert_eq!(count(&ViewOptions { hidden_draw_categories: ["Hide".to_string()].into(), ..no_grid() }), 3);
    assert_eq!(count(&ViewOptions { hidden_colors: [Color::RED].into(), ..no_grid() }), 2);
    assert_eq!(count(&ViewOptions { hidden_entities: [2].into(), ..no_grid() }), 3);
    assert_eq!(count(&ViewOptions { show_draws: false, ..no_grid() }), 0);
}

#[test]
fn transparent_colour_draws_nothing_and_semitransparent_goes_to_the_blended_pass() {
    let (r, a) = build(vec![define(1, "A"), sphere(0, 1, "x", Color(44)), sphere(0, 1, "x", Color::RED)]);
    let solid = frame_of(&r, &a, 0, None, &no_grid());
    assert!(!solid.solid.is_empty() && solid.translucent.is_empty());
    let translucent = frame_of(&r, &a, 0, None, &ViewOptions { semitransparent: true, ..no_grid() });
    assert!(translucent.solid.is_empty());
    assert_eq!(translucent.translucent.len(), solid.solid.len());
    assert!(translucent.translucent.iter().all(|v| v.color[3] < 0.5));
}

#[test]
fn lines_and_circles_are_world_space_per_frame() {
    let (r, a) = build(vec![
        define(1, "A"),
        ent(
            0,
            1,
            EntityOp::Line {
                category: "x".into(),
                start: Point::ZERO,
                end: Point::new(0.0, 0.0, 10.0),
                color: Color::RED,
            },
        ),
        ent(
            0,
            1,
            EntityOp::Circle {
                category: "x".into(),
                position: Point::ZERO,
                up: Point::new(0.0, 0.0, 1.0),
                radius: 5.0,
                color: Color::RED,
            },
        ),
    ]);
    let f = frame_of(&r, &a, 0, None, &no_grid());
    assert_eq!(f.lines.len(), 1 + 40);
}

#[test]
fn creation_lines_are_transformed_by_the_entity() {
    let half = std::f32::consts::FRAC_1_SQRT_2;
    let (r, a) = build(vec![
        define(1, "Sensor"),
        ent(
            0,
            1,
            EntityOp::Line {
                category: String::new(),
                start: Point::ZERO,
                end: Point::new(10.0, 0.0, 0.0),
                color: Color::RED,
            },
        ),
        ent(
            0,
            1,
            EntityOp::SetTransform(Transform {
                translation: Point::new(100.0, 0.0, 0.0),
                rotation: Quat { x: 0.0, y: 0.0, z: half, w: half },
            }),
        ),
    ]);
    let o = ViewOptions { show_entity_axes: false, ..no_grid() };
    let f = frame_of(&r, &a, 0, None, &o);
    assert_eq!(f.lines.len(), 1);
    let (a, b) = (Vec3::from(f.lines[0].a), Vec3::from(f.lines[0].b));
    assert!((a - Vec3::new(100.0, 0.0, 0.0)).length() < 1e-3);
    assert!((b - Vec3::new(100.0, 10.0, 0.0)).length() < 1e-3, "rotated a quarter turn about Z: {b:?}");
}

#[test]
fn selection_highlights_labels_and_paths() {
    let (r, a) = build(vec![
        define(1, "A"),
        define(2, "B"),
        sphere(0, 1, "", Color::RED),
        at(0.0, 0.0, 0.0).into_record(0, 1),
        at(0.0, 0.0, 0.0).into_record(0, 2),
        step(1.0),
        at(10.0, 0.0, 0.0).into_record(1, 1),
        step(2.0),
        at(20.0, 0.0, 0.0).into_record(2, 1),
    ]);
    let o = ViewOptions { show_entity_axes: false, ..no_grid() };

    let none = frame_of(&r, &a, 1, None, &o);
    assert!(none.labels.is_empty(), "names are off by default");
    assert!(none.lines.is_empty(), "paths are off by default");
    assert_eq!(none.instances[0].mix[3], 0.0);

    let sel = frame_of(&r, &a, 1, Some(1), &o);
    assert_eq!(sel.labels.len(), 1);
    assert_eq!(sel.labels[0].text, "A");
    assert!(sel.instances[0].mix[3] > 0.0);
    assert_eq!(sel.markers.iter().filter(|m| m.selected).count(), 1);
    // Past segment solid, future segment faint.
    let alphas: Vec<f32> = sel.lines.iter().map(|l| l.color[3]).collect();
    assert_eq!(alphas.len(), 2);
    assert!(alphas.iter().any(|&a| a > 0.9) && alphas.iter().any(|&a| a < 0.5), "{alphas:?}");

    let all = frame_of(&r, &a, 1, None, &ViewOptions { show_all_names: true, show_all_paths: true, ..o.clone() });
    assert_eq!(all.labels.len(), 2);
    assert_eq!(all.lines.len(), 1, "only the past segment of the moving entity");
}

#[test]
fn hidden_entities_vanish_unless_selected() {
    let (r, a) = build(vec![define(1, "A"), sphere(0, 1, "", Color::RED), at(0.0, 0.0, 0.0).into_record(0, 1)]);
    let o = ViewOptions { hidden_entities: [1].into(), ..no_grid() };
    let hidden = frame_of(&r, &a, 0, None, &o);
    assert!(hidden.instances.is_empty() && hidden.markers.is_empty() && hidden.pickables.is_empty());
    let selected = frame_of(&r, &a, 0, Some(1), &o);
    assert!(selected.instances.is_empty(), "geometry stays hidden");
    assert_eq!(selected.labels.len(), 1, "but the selection is still labelled");
}

#[test]
fn the_grid_adapts_to_the_camera() {
    let near = Camera { distance: 500.0, ..Camera::default() };
    let far = Camera { distance: 500_000.0, ..Camera::default() };
    let (mut a, mut b) = (Vec::new(), Vec::new());
    vrd_scene::grid(&near, false, 1.0, 0.0, &mut a);
    vrd_scene::grid(&far, false, 1.0, 0.0, &mut b);
    assert_eq!(a.len(), b.len(), "same line count at any zoom");
    let span = |v: &[vrd_scene::LineSegment]| v.iter().map(|l| Vec3::from(l.a).x.abs()).fold(0.0, f32::max);
    assert!(span(&b) > span(&a) * 100.0);
    assert!(a.iter().chain(&b).all(|l| l.a.iter().chain(&l.b).all(|c| c.is_finite())));
}

#[test]
fn main_camera_pose_is_read_from_the_entity() {
    let half = std::f32::consts::FRAC_1_SQRT_2;
    let (r, a) = build(vec![
        define(7, "maincamera"),
        ent(
            0,
            7,
            EntityOp::SetTransform(Transform {
                translation: Point::new(1.0, 2.0, 3.0),
                rotation: Quat { x: 0.0, y: 0.0, z: half, w: half },
            }),
        ),
    ]);
    assert_eq!(a.camera_entity(), Some(7));
    let (pos, look) = follow_pose(&r, &a, 0, Vec3::NEG_X).unwrap();
    assert_eq!(pos, Vec3::new(1.0, 2.0, 3.0));
    // The entity faces -X; a quarter turn about Z turns that into -Y.
    assert!((look - Vec3::NEG_Y).length() < 1e-4, "{look:?}");

    let (r2, a2) = build(vec![define(1, "other"), at(0.0, 0.0, 0.0).into_record(0, 1)]);
    assert!(follow_pose(&r2, &a2, 0, Vec3::NEG_X).is_none());
}

#[test]
fn picking_chooses_the_entity_under_the_cursor() {
    let (r, a) = build(vec![
        define(1, "Left"),
        define(2, "Right"),
        at(0.0, -200.0, 0.0).into_record(0, 1),
        at(0.0, 200.0, 0.0).into_record(0, 2),
    ]);
    let cam = Camera { yaw: 0.0, pitch: 0.0, distance: 1500.0, ..Camera::default() };
    let f = frame_of(&r, &a, 0, None, &no_grid());
    let vp = Vec2::new(800.0, 600.0);
    let screen =
        |id: i32| cam.project(f.pickables.iter().find(|p| p.entity == id).unwrap().position, vp).unwrap().screen;

    assert_eq!(pick(&cam, vp, screen(1) + Vec2::new(3.0, -2.0), &f.pickables), Some(1));
    assert_eq!(pick(&cam, vp, screen(2), &f.pickables), Some(2));
    assert_eq!(pick(&cam, vp, Vec2::new(400.0, 580.0), &f.pickables), None);
    assert_eq!(pick(&cam, vp, screen(1), &[]), None);
}

#[test]
fn focus_and_frame_all() {
    let (r, a) = build(vec![
        define(1, "A"),
        define(2, "B"),
        sphere(0, 1, "", Color::RED),
        at(0.0, 0.0, 0.0).into_record(0, 1),
        at(1000.0, 0.0, 0.0).into_record(0, 2),
    ]);
    let (c, radius) = frame_all(&r, &a, 0);
    assert!((c - Vec3::new(500.0, 0.0, 0.0)).length() < 1e-3);
    assert!(radius >= 500.0);
    let (pos, rad) = focus_target(&r, &a, 1, 0).unwrap();
    assert_eq!(pos, Vec3::ZERO);
    assert!(rad >= a.unit() && rad >= 10.0);
    assert!(focus_target(&r, &a, 99, 0).is_none());
    assert_eq!(frame_all(&Replay::default(), &SceneAssets::default(), 0).0, Vec3::ZERO);
}

#[test]
fn the_real_capture_builds_finite_geometry_on_every_sampled_frame() {
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/sample.vrd")).unwrap();
    let r = Replay::from_bytes(&bytes).unwrap();
    let a = SceneAssets::build(&r);
    let o = ViewOptions { show_all_names: true, show_all_paths: true, ..ViewOptions::default() };

    let mut any_instances = false;
    let mut any_draws = false;
    for frame in (0..=r.last_frame()).step_by(37) {
        let f = frame_of(&r, &a, frame, r.entities().next().map(|e| e.id), &o);
        any_instances |= !f.instances.is_empty();
        any_draws |= !f.solid.is_empty() || f.lines.len() > 100;
        for v in f.solid.iter().chain(&f.translucent) {
            assert!(v.position.iter().chain(&v.normal).all(|c| c.is_finite()));
        }
        for l in &f.lines {
            assert!(l.a.iter().chain(&l.b).all(|c| c.is_finite()));
        }
        for inst in &f.instances {
            assert!(inst.model < a.models.len() && inst.matrix.is_finite());
        }
    }
    assert!(any_instances || a.models.is_empty());
    assert!(any_draws);
    let (c, radius) = frame_all(&r, &a, 0);
    assert!(c.is_finite() && radius.is_finite() && radius > 0.0);
}

#[test]
fn up_axis_is_the_one_positions_vary_least_along() {
    let walk = |id: i32, up: Axis, len: f32| {
        // Walk around a ground plane perpendicular to `up`, with a little bobbing along it.
        let (h1, h2) = up.horizontal();
        let mut records = vec![define(id, "Walker")];
        for i in 0..20 {
            let a = i as f32 * 0.3;
            let p = h1 * (a.cos() * len) + h2 * (a.sin() * len) + up.vector() * (0.5 + 0.1 * a.sin());
            records.push(ent(i, id, at(p.x, p.y, p.z)));
            records.push(step(i as f32 + 1.0));
        }
        build(records).1.detected_up()
    };
    assert_eq!(walk(1, Axis::Z, 50.0), Axis::Z);
    assert_eq!(walk(1, Axis::Y, 50.0), Axis::Y);
    assert_eq!(walk(1, Axis::X, 50.0), Axis::X);
}

#[test]
fn ambiguous_or_empty_scenes_default_to_z_up() {
    assert_eq!(build(vec![]).1.detected_up(), Axis::Z);
    let cube = build(vec![define(1, "A"), at(0.0, 0.0, 0.0).into_record(0, 1), at(10.0, 10.0, 10.0).into_record(1, 1)]);
    assert_eq!(cube.1.detected_up(), Axis::Z);
}

#[test]
fn the_ground_sits_at_the_origin_unless_the_data_is_far_from_it() {
    let (_, near) = build(vec![
        define(1, "A"),
        sphere(0, 1, "", Color::RED),
        at(0.0, 0.5, 0.0).into_record(0, 1),
        at(30.0, 0.5, 30.0).into_record(1, 1),
    ]);
    assert_eq!(near.ground_height(Axis::Y), 0.0);

    let (_, far) =
        build(vec![define(1, "A"), at(0.0, 0.0, -25.0).into_record(0, 1), at(5.0, 5.0, -7.0).into_record(1, 1)]);
    assert_eq!(far.ground_height(Axis::Z), -25.0, "the capture lives well below the origin");
    assert_eq!(build(vec![]).1.ground_height(Axis::Z), 0.0);
}

#[test]
fn the_grid_lies_in_the_plane_perpendicular_to_up() {
    for up in Axis::ALL {
        let cam = Camera { up_axis: up, ..Camera::default() };
        let mut lines = Vec::new();
        vrd_scene::grid(&cam, false, 1.0, 3.0, &mut lines);
        for l in &lines {
            for p in [Vec3::from(l.a), Vec3::from(l.b)] {
                assert!((p.dot(up.vector()) - 3.0).abs() < 1e-3, "{up:?}: {p:?} is off the ground plane");
            }
        }
        // The two origin axes are the horizontal ones, in their own colours.
        let coloured = lines.iter().filter(|l| l.width == 1.5).count();
        assert_eq!(coloured, 2, "{up:?}");
    }
}

#[test]
fn camera_forward_follows_the_up_convention() {
    use vrd_scene::default_camera_forward;
    assert_eq!(default_camera_forward(Axis::Z), Vec3::NEG_X);
    assert_eq!(default_camera_forward(Axis::Y), Vec3::NEG_Z);
}
fn define_at(id: i32, at: Point) -> Record {
    ent(
        0,
        id,
        EntityOp::Define {
            parent: None,
            def: EntityDef {
                id,
                name: format!("E{id}"),
                initial_transform: Transform { translation: at, rotation: Quat::IDENTITY },
                ..EntityDef::default()
            },
        },
    )
}

fn world_centre(f: &SceneFrame, a: &SceneAssets, index: usize) -> Vec3 {
    let inst = &f.instances[index];
    inst.matrix.transform_point3(a.models[inst.model].center)
}

#[test]
fn registration_geometry_recorded_in_world_space_follows_the_entity() {
    // The sample capture records, at registration, a capsule around the entity's world position.
    let here = Point::new(100.0, 0.0, 0.0);
    let (r, a) = build(vec![
        define_at(1, here),
        ent(0, 1, EntityOp::Sphere { category: String::new(), center: here, radius: 10.0, color: Color::RED }),
        at(100.0, 0.0, 0.0).into_record(0, 1),
        step(1.0),
        at(300.0, 0.0, 0.0).into_record(1, 1),
    ]);
    assert!(a.models[0].center.length() < 1e-3, "stored relative to the entity: {:?}", a.models[0].center);
    let o = no_grid();
    assert!((world_centre(&frame_of(&r, &a, 0, None, &o), &a, 0) - Vec3::new(100.0, 0.0, 0.0)).length() < 1e-3);
    assert!((world_centre(&frame_of(&r, &a, 1, None, &o), &a, 0) - Vec3::new(300.0, 0.0, 0.0)).length() < 1e-3);
}

#[test]
fn registration_geometry_recorded_relative_to_the_entity_stays_relative() {
    let (r, a) = build(vec![
        define_at(1, Point::new(100.0, 0.0, 0.0)),
        ent(
            0,
            1,
            EntityOp::Sphere {
                category: String::new(),
                center: Point::new(2.0, 0.0, 0.0),
                radius: 1.0,
                color: Color::RED,
            },
        ),
        at(100.0, 0.0, 0.0).into_record(0, 1),
        step(1.0),
        at(300.0, 0.0, 0.0).into_record(1, 1),
    ]);
    assert!((a.models[0].center - Vec3::new(2.0, 0.0, 0.0)).length() < 1e-3);
    let o = no_grid();
    assert!((world_centre(&frame_of(&r, &a, 0, None, &o), &a, 0) - Vec3::new(102.0, 0.0, 0.0)).length() < 1e-3);
    assert!((world_centre(&frame_of(&r, &a, 1, None, &o), &a, 0) - Vec3::new(302.0, 0.0, 0.0)).length() < 1e-3);
}

#[test]
fn world_space_registration_lines_follow_a_rotating_entity_too() {
    let half = std::f32::consts::FRAC_1_SQRT_2;
    let here = Point::new(10.0, 0.0, 0.0);
    let (r, a) = build(vec![
        define_at(1, here),
        // A line from the entity out along +X by 5, recorded in world space.
        ent(
            0,
            1,
            EntityOp::Line { category: String::new(), start: here, end: Point::new(15.0, 0.0, 0.0), color: Color::RED },
        ),
        ent(0, 1, EntityOp::SetTransform(Transform { translation: here, rotation: Quat::IDENTITY })),
        step(1.0),
        ent(
            1,
            1,
            EntityOp::SetTransform(Transform {
                translation: Point::new(0.0, 20.0, 0.0),
                rotation: Quat { x: 0.0, y: 0.0, z: half, w: half },
            }),
        ),
    ]);
    let o = ViewOptions { show_entity_axes: false, ..no_grid() };
    let (f0, f1) = (frame_of(&r, &a, 0, None, &o), frame_of(&r, &a, 1, None, &o));
    let ends = |f: &SceneFrame| (Vec3::from(f.lines[0].a), Vec3::from(f.lines[0].b));
    let (a0, b0) = ends(&f0);
    assert!((a0 - Vec3::new(10.0, 0.0, 0.0)).length() < 1e-3 && (b0 - Vec3::new(15.0, 0.0, 0.0)).length() < 1e-3);
    // After a quarter turn about Z the +X line points along +Y, from the entity's new position.
    let (a1, b1) = ends(&f1);
    assert!((a1 - Vec3::new(0.0, 20.0, 0.0)).length() < 1e-3, "{a1:?}");
    assert!((b1 - Vec3::new(0.0, 25.0, 0.0)).length() < 1e-3, "{b1:?}");
}

#[test]
fn the_real_capture_attaches_characters_to_their_entities() {
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/sample.vrd")).unwrap();
    let r = Replay::from_bytes(&bytes).unwrap();
    let a = SceneAssets::build(&r);
    assert_eq!(a.detected_up(), Axis::Y, "the sample is a Y-up capture");
    // The player's capsule is about a body's height, centred near the entity, not 10 m away.
    let player = r.entities().find(|e| e.name == "PlayerCharacter").unwrap().id;
    assert!(a.entity_radius(player) < 5.0, "radius {}", a.entity_radius(player));
    assert!(a.unit() > 0.3 && a.unit() < 5.0, "unit {}", a.unit());
    let f = frame_of(&r, &a, 600, None, &no_grid());
    let at_player = r.position_at(player, 600).map(|p| Vec3::new(p.x, p.y, p.z)).unwrap();
    let nearest = f
        .instances
        .iter()
        .filter(|i| a.models[i.model].entity == player)
        .map(|i| i.matrix.transform_point3(a.models[i.model].center).distance(at_player))
        .fold(f32::MAX, f32::min);
    assert!(nearest < 3.0, "the player's geometry is {nearest} from the player");
}
