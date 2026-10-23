use vrd_format::{
    Color, Compression, EntityDef, EntityOp, FormatError, Point, Quat, Record, Transform, decode, encode,
};

fn sample_records() -> Vec<Record> {
    let xform = Transform { translation: Point::new(1.0, 2.0, 3.0), rotation: Quat { x: 0.0, y: 0.5, z: 0.0, w: 0.5 } };
    let entity = |frame, id, op| Record::Entity { frame, id, op };
    vec![
        Record::Header,
        entity(
            0,
            1,
            EntityOp::Define {
                parent: None,
                def: EntityDef {
                    id: 1,
                    name: "Player".into(),
                    path: "World.Player".into(),
                    type_name: "Character".into(),
                    category: "Actors".into(),
                    initial_transform: xform,
                    static_params: vec![("Team".into(), "Blue".into()), ("HP".into(), "100".into())],
                    creation_frame: 0,
                },
            },
        ),
        entity(
            0,
            2,
            EntityOp::Define {
                parent: Some(1),
                def: EntityDef { id: 2, name: "Sword".into(), creation_frame: 0, ..EntityDef::default() },
            },
        ),
        entity(0, 1, EntityOp::SetPosition(Point::new(0.5, 0.0, -1.5))),
        entity(0, 1, EntityOp::SetTransform(xform)),
        entity(0, 1, EntityOp::Log { category: "AI".into(), message: "hello".into(), color: Color::GREEN }),
        entity(0, 1, EntityOp::Parameter { label: "State".into(), value: "Idle".into() }),
        entity(0, 1, EntityOp::Value { label: "Speed".into(), value: 3.25 }),
        entity(
            0,
            1,
            EntityOp::Line {
                category: "Nav".into(),
                start: Point::ZERO,
                end: Point::new(1.0, 1.0, 1.0),
                color: Color::RED,
            },
        ),
        entity(
            0,
            1,
            EntityOp::Circle {
                category: "Nav".into(),
                position: Point::ZERO,
                up: Point::new(0.0, 1.0, 0.0),
                radius: 2.0,
                color: Color::BLUE,
            },
        ),
        entity(
            0,
            1,
            EntityOp::Sphere {
                category: String::new(),
                center: Point::new(0.0, 1.0, 0.0),
                radius: 0.5,
                color: Color::YELLOW,
            },
        ),
        entity(
            0,
            1,
            EntityOp::Capsule {
                category: "Body".into(),
                p1: Point::ZERO,
                p2: Point::new(0.0, 2.0, 0.0),
                radius: 0.3,
                color: Color::WHITE,
            },
        ),
        entity(
            0,
            1,
            EntityOp::Mesh {
                category: String::new(),
                verts: vec![Point::ZERO, Point::new(1.0, 0.0, 0.0), Point::new(0.0, 1.0, 0.0)],
                color: Color::BLACK,
            },
        ),
        entity(
            0,
            1,
            EntityOp::Box {
                category: "Bounds".into(),
                transform: xform,
                dimensions: Point::new(1.0, 2.0, 3.0),
                color: Color(130),
            },
        ),
        Record::FrameStep { total_time: 0.016 },
        entity(1, 1, EntityOp::SetPosition(Point::new(1.0, 0.0, 0.0))),
        Record::FrameStep { total_time: 0.033 },
        entity(2, 2, EntityOp::Undefine),
    ]
}

#[test]
fn round_trips_uncompressed() {
    let records = sample_records();
    let bytes = encode(&records, Compression::None).unwrap();
    let decoded = decode(&bytes).unwrap();
    assert!(!decoded.compressed);
    assert!(!decoded.truncated);
    assert_eq!(decoded.records, records);
}

#[test]
fn round_trips_compressed() {
    let records = sample_records();
    let bytes = encode(&records, Compression::Deflate).unwrap();
    let decoded = decode(&bytes).unwrap();
    assert!(decoded.compressed);
    assert!(!decoded.truncated);
    assert_eq!(decoded.records, records);
}

#[test]
fn var_ints_round_trip_at_the_edges() {
    let values = [0, 1, 127, 128, 300, 16_383, 16_384, i32::MAX, -1, i32::MIN];
    let records: Vec<_> = values.iter().map(|&v| Record::Entity { frame: v, id: v, op: EntityOp::Undefine }).collect();
    let bytes = encode(&records, Compression::None).unwrap();
    assert_eq!(decode(&bytes).unwrap().records, records);
}

#[test]
fn truncated_stream_keeps_complete_blocks() {
    let records = sample_records();
    let bytes = encode(&records, Compression::None).unwrap();
    for cut in [bytes.len() - 1, bytes.len() - 5, bytes.len() / 2] {
        let decoded = decode(&bytes[..cut]).unwrap();
        assert!(decoded.truncated, "cut at {cut}");
        assert!(decoded.records.len() < records.len());
        assert_eq!(decoded.records[..], records[..decoded.records.len()]);
    }
}

#[test]
fn truncated_compressed_stream_keeps_a_prefix() {
    let records = sample_records();
    let bytes = encode(&records, Compression::Deflate).unwrap();
    let decoded = decode(&bytes[..bytes.len() - 4]).unwrap();
    assert!(decoded.truncated);
    assert_eq!(decoded.records[..], records[..decoded.records.len()]);
}

#[test]
fn rejects_non_captures() {
    assert!(matches!(decode(&[]), Err(FormatError::Empty)));
    // 0x00 is not a valid block type and the data is not a valid deflate stream start either.
    assert!(decode(&[0u8; 16]).is_err());
    assert!(decode(b"this is plainly not a replay file").is_err());
}

#[test]
fn reads_the_original_sample_capture() {
    let bytes = std::fs::read(concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/sample.vrd")).unwrap();
    let decoded = decode(&bytes).unwrap();
    assert!(decoded.compressed);
    assert!(decoded.records.len() > 1000, "{} records", decoded.records.len());

    let steps = decoded.records.iter().filter(|r| matches!(r, Record::FrameStep { .. })).count();
    assert!(steps > 10, "{steps} frame steps");

    let defs =
        decoded.records.iter().filter(|r| matches!(r, Record::Entity { op: EntityOp::Define { .. }, .. })).count();
    assert!(defs > 0, "no entity definitions");

    // Whatever we read must re-encode and decode to the same thing.
    let again = decode(&encode(&decoded.records, Compression::Deflate).unwrap()).unwrap();
    assert_eq!(again.records, decoded.records);
}
