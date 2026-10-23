use vrd_format::{Color, EntityDef, EntityOp, Point, Record, Transform};
use vrd_model::{LoadInfo, Replay, Shape};

fn ent(frame: i32, id: i32, op: EntityOp) -> Record {
    Record::Entity { frame, id, op }
}

fn define(frame: i32, id: i32, name: &str, parent: Option<i32>) -> Record {
    ent(
        frame,
        id,
        EntityOp::Define {
            parent,
            def: EntityDef {
                id,
                name: name.into(),
                path: format!("World.{name}"),
                category: "Actors".into(),
                creation_frame: frame,
                static_params: vec![("Team".into(), "Blue".into())],
                ..EntityDef::default()
            },
        },
    )
}

fn step(t: f32) -> Record {
    Record::FrameStep { total_time: t }
}

fn build(records: Vec<Record>) -> Replay {
    Replay::from_records(records, LoadInfo::default())
}

#[test]
fn frame_time_mapping() {
    let r = build(vec![step(0.125), step(0.25), step(0.5)]);
    assert_eq!(r.frame_count(), 4);
    assert_eq!(r.last_frame(), 3);
    assert_eq!(r.total_time(), 0.5);

    assert_eq!(r.frame_for_time(-1.0), 0);
    assert_eq!(r.frame_for_time(f64::NAN), 0);
    assert_eq!(r.frame_for_time(0.0), 0);
    assert_eq!(r.frame_for_time(0.05), 0);
    assert_eq!(r.frame_for_time(0.125), 1);
    assert_eq!(r.frame_for_time(0.3), 2);
    assert_eq!(r.frame_for_time(99.0), 3);

    assert_eq!(r.time_for_frame(-4), 0.0);
    assert_eq!(r.time_for_frame(2), 0.25);
    assert_eq!(r.time_for_frame(99), 0.5);
}

#[test]
fn empty_capture_is_one_frame() {
    let r = build(vec![]);
    assert_eq!(r.frame_count(), 1);
    assert_eq!(r.frame_for_time(10.0), 0);
    assert_eq!(r.total_time(), 0.0);
}

#[test]
fn transforms_use_the_latest_state_and_merge_positions() {
    let rot = vrd_format::Quat { x: 0.0, y: 1.0, z: 0.0, w: 0.0 };
    let r = build(vec![
        define(0, 1, "A", None),
        ent(0, 1, EntityOp::SetPosition(Point::new(1.0, 0.0, 0.0))),
        step(1.0),
        step(2.0),
        ent(2, 1, EntityOp::SetTransform(Transform { translation: Point::new(5.0, 0.0, 0.0), rotation: rot })),
        step(3.0),
        ent(3, 1, EntityOp::SetPosition(Point::new(9.0, 0.0, 0.0))),
    ]);

    assert_eq!(r.position_at(1, 0).unwrap().x, 1.0);
    assert_eq!(r.position_at(1, 1).unwrap().x, 1.0);
    assert_eq!(r.position_at(1, 2).unwrap().x, 5.0);
    let last = r.transform_at(1, 3).unwrap();
    assert_eq!(last.translation.x, 9.0);
    assert_eq!(last.rotation, rot, "a position update keeps the previous rotation");
    assert!(r.transform_at(42, 0).is_none());
}

#[test]
fn unmoved_entities_report_their_initial_transform() {
    let r = build(vec![define(0, 1, "A", None), step(1.0)]);
    assert_eq!(r.transform_at(1, 1), Some(Transform::IDENTITY));
}

#[test]
fn lifetime_follows_register_and_unregister() {
    let r = build(vec![
        step(1.0),
        define(1, 1, "A", None),
        step(2.0),
        step(3.0),
        ent(3, 1, EntityOp::Undefine),
        step(4.0),
        step(5.0),
    ]);
    assert_eq!(r.lifetime(1), Some((1, 3)));
    assert!(!r.is_alive_at(1, 0));
    assert!(r.is_alive_at(1, 1));
    assert!(r.is_alive_at(1, 3));
    assert!(!r.is_alive_at(1, 4));

    let alive_forever = build(vec![define(0, 7, "B", None), step(1.0), step(2.0)]);
    assert_eq!(alive_forever.lifetime(7), Some((0, 2)));
}

#[test]
fn parameters_are_resolved_per_name_at_a_frame() {
    let param = |frame, label: &str, value: &str| {
        ent(frame, 1, EntityOp::Parameter { label: label.into(), value: value.into() })
    };
    let value = |frame, label: &str, value| ent(frame, 1, EntityOp::Value { label: label.into(), value });
    let r = build(vec![
        define(0, 1, "A", None),
        param(0, "State", "Idle"),
        value(0, "Speed", 1.0),
        step(1.0),
        param(1, "State", "Run"),
        param(1, "State", "Run"),
        param(1, "Target", "B"),
        value(1, "Speed", 4.0),
        step(2.0),
    ]);

    assert_eq!(r.params_at(1, 0), [("State", "Idle")]);
    assert_eq!(r.params_at(1, 1), [("State", "Run"), ("Target", "B")]);
    assert_eq!(r.values_at(1, 0), [("Speed", 1.0)]);
    assert_eq!(r.values_at(1, 5), [("Speed", 4.0)]);
    assert_eq!(r.entity(1).unwrap().params["State"].len(), 2, "redundant sets are dropped");
    assert!(r.param_names().contains("Speed") && r.param_names().contains("Target"));

    let props = r.properties_at(1, 1);
    let keys: Vec<_> = props.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(keys, ["Name", "Path", "Id", "Position", "Team", "State", "Target", "Speed"]);
}

#[test]
fn logs_are_cleaned_and_queryable_by_frame() {
    let log = |frame, msg: &str| {
        ent(frame, 1, EntityOp::Log { category: "AI".into(), message: msg.into(), color: Color::RED })
    };
    let r = build(vec![
        define(0, 1, "A", None),
        log(0, "first\r\nline"),
        step(1.0),
        log(1, "second"),
        log(1, "third"),
        step(2.0),
        step(3.0),
        log(3, "fourth"),
    ]);

    assert_eq!(r.logs().len(), 4);
    assert_eq!(r.logs()[0].message, "first  line");
    assert_eq!(r.logs_in(1, 1).len(), 2);
    assert_eq!(r.logs_in(2, 2).len(), 0);
    assert_eq!(r.logs_in(0, 99).len(), 4);
    assert_eq!(r.logs_in(5, 1).len(), 0);
    assert_eq!(r.entity(1).unwrap().log_frames, [0, 1, 3]);
    assert!(r.log_categories().contains("AI"));
    assert!(r.log_colors().contains(&Color::RED));
}

#[test]
fn creation_draws_are_separated_from_per_frame_draws() {
    let sphere = |frame, category: &str| {
        ent(
            frame,
            1,
            EntityOp::Sphere { category: category.into(), center: Point::ZERO, radius: 1.0, color: Color::BLUE },
        )
    };
    let r = build(vec![
        define(0, 1, "A", None),
        sphere(0, ""),
        sphere(0, "Debug"),
        step(1.0),
        sphere(1, "Debug"),
        sphere(1, ""),
    ]);

    assert_eq!(r.draws().len(), 4);
    assert_eq!(r.draws_at(0).len(), 2);
    assert_eq!(r.draws_at(1).len(), 2);
    assert_eq!(r.draws_at(9).len(), 0);
    let creation: Vec<_> = r.creation_draws(1).collect();
    assert_eq!(creation.len(), 1, "only the empty-category draw at the registration frame");
    assert!(matches!(creation[0].shape, Shape::Sphere { .. }));
    assert_eq!(r.draw_categories().iter().collect::<Vec<_>>(), ["Debug"]);
}

#[test]
fn hierarchy_handles_out_of_order_missing_and_cyclic_parents() {
    let r = build(vec![
        define(0, 3, "Grandchild", Some(2)),
        define(0, 2, "Child", Some(1)),
        define(0, 1, "Root", None),
        define(0, 4, "Orphan", Some(99)),
        define(0, 5, "LoopA", Some(6)),
        define(0, 6, "LoopB", Some(5)),
        define(0, 7, "SelfParent", Some(7)),
    ]);

    assert_eq!(r.children_of(1), [2]);
    assert_eq!(r.children_of(2), [3]);
    assert_eq!(r.children_of(3), [] as [i32; 0]);
    assert_eq!(r.root_entities(), [1, 4, 5, 6, 7]);
}

#[test]
fn redefinition_overrides_descriptive_fields_but_keeps_history() {
    let r = build(vec![
        define(0, 1, "Old", None),
        ent(0, 1, EntityOp::SetPosition(Point::new(3.0, 0.0, 0.0))),
        step(1.0),
        define(1, 1, "New", None),
    ]);
    let e = r.entity(1).unwrap();
    assert_eq!(e.name, "New");
    assert_eq!(e.creation_frame, 0);
    assert_eq!(e.registration_frame, 1);
    assert_eq!(r.position_at(1, 1).unwrap().x, 3.0);
    assert_eq!(r.entity_count(), 1);
}

#[test]
fn records_for_unknown_entities_create_placeholders() {
    let r = build(vec![ent(0, 9, EntityOp::SetPosition(Point::new(1.0, 2.0, 3.0)))]);
    let e = r.entity(9).unwrap();
    assert!(!e.defined);
    assert_eq!(e.display_name(), "Entity 9");
    assert_eq!(r.root_entities(), [9]);
}

#[test]
fn loads_the_original_sample_capture() {
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/sample.vrd")).unwrap();
    let r = Replay::from_bytes(&bytes).unwrap();

    assert!(r.info().compressed);
    assert!(r.frame_count() > 10, "{} frames", r.frame_count());
    assert!(r.total_time() > 0.0);
    assert!(r.entity_count() > 0);
    assert!(!r.root_entities().is_empty());

    // Frame times never go backwards.
    assert!(r.frame_times().windows(2).all(|w| w[0] <= w[1]));
    // Every entity in the tree is reachable from the roots exactly once.
    let mut seen = 0;
    let mut stack: Vec<i32> = r.root_entities().to_vec();
    while let Some(id) = stack.pop() {
        seen += 1;
        stack.extend_from_slice(r.children_of(id));
    }
    assert_eq!(seen, r.entity_count());
    // Every log lies inside the capture.
    assert!(r.logs().iter().all(|l| l.frame >= 0 && l.frame <= r.last_frame()));
}
