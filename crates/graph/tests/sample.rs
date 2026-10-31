use vrd_format::{EntityDef, EntityOp, Record};
use vrd_graph::{GraphData, Stacking, layout, trace};
use vrd_model::{LoadInfo, Replay};

fn sample() -> Replay {
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/sample.vrd")).unwrap();
    Replay::from_bytes(&bytes).unwrap()
}

fn id(r: &Replay, name: &str) -> i32 {
    r.entities().find(|e| e.name == name).unwrap().id
}

#[test]
fn the_sample_has_the_series_the_old_tool_graphed() {
    let r = sample();
    let g = GraphData::build(&r);
    let stats = g.series(id(&r, "SystemStats"));
    let names: Vec<&str> = stats.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(names, ["DrawCalls", "FPS", "GPUTime", "Triangles", "timestepMS"]);
    assert_eq!(g.series(id(&r, "PhysicsSceneQueriesSystem")).len(), 12);
    assert!(g.series(id(&r, "NavMesh")).is_empty());
    assert!(g.entities().count() >= 8);

    // Every series is in time order and holds real values.
    for e in g.entities() {
        for s in g.series(e) {
            assert!(s.times.windows(2).all(|w| w[0] < w[1]), "{}", s.name);
            assert!(s.values.iter().all(|v| v.is_finite()));
            assert!(s.valid.0 <= s.valid.1);
        }
    }
}

#[test]
fn a_short_lived_entity_stops_where_it_was_unregistered() {
    let r = sample();
    let g = GraphData::build(&r);
    let trooper = id(&r, "imperial_stormtrooper#4");
    let health = &g.series(trooper)[0];
    let (first, last) = r.lifetime(trooper).unwrap();
    assert_eq!(health.valid.0, r.time_for_frame(first));
    assert_eq!(health.valid.1, r.time_for_frame(last + 1));
    assert_eq!(health.value_at(r.time_for_frame(last + 5)), None);
    let t = trace(health, 0.0, r.total_time(), 1000.0);
    let x_end = (health.valid.1 / r.total_time() * 1000.0) as f32;
    assert!(t.line.iter().all(|p| p.0 <= x_end + 0.01));
}

#[test]
fn physics_queries_stack_by_depth() {
    let r = sample();
    let g = GraphData::build(&r);
    let series = g.series(id(&r, "PhysicsSceneQueriesSystem"));
    let list: Vec<_> = series.iter().enumerate().map(|(i, s)| (i, s.name.as_str(), s.depth)).collect();
    let l = layout(&[list], Stacking::default());
    assert_eq!(l.bands.len(), 2);
    assert_eq!(l.bands[0].title, "Physics_Queries");
    assert_eq!(l.bands[1].members.len(), 11);
}

#[test]
fn colours_follow_names_and_are_stable() {
    let r = sample();
    let a = GraphData::build(&r);
    let b = GraphData::build(&r);
    let names = ["FPS", "health", "speed", "Physics_Queries"];
    let ia: Vec<usize> = names.iter().map(|n| a.name_index(n)).collect();
    let ib: Vec<usize> = names.iter().map(|n| b.name_index(n)).collect();
    assert_eq!(ia, ib);
    let mut unique = ia.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), names.len(), "different names, different indices");
}

#[test]
fn values_on_the_same_frame_keep_the_last_one() {
    let def = Record::Entity {
        frame: 0,
        id: 1,
        op: EntityOp::Define { parent: None, def: EntityDef { id: 1, name: "A".into(), ..EntityDef::default() } },
    };
    let value = |frame, v| Record::Entity { frame, id: 1, op: EntityOp::Value { label: "x".into(), value: v } };
    let r = Replay::from_records(
        vec![def, value(0, 1.0), value(0, 2.0), Record::FrameStep { total_time: 1.0 }, value(1, 3.0)],
        LoadInfo::default(),
    );
    let g = GraphData::build(&r);
    let s = &g.series(1)[0];
    assert_eq!(s.values, [2.0, 3.0]);
    assert_eq!(s.value_at(0.5), Some(2.0));
}
