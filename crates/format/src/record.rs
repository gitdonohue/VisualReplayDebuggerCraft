use crate::{Color, Point, Transform};

/// Static description of an entity, as written when it is registered.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct EntityDef {
    pub id: i32,
    pub name: String,
    pub path: String,
    pub type_name: String,
    pub category: String,
    pub initial_transform: Transform,
    /// Insertion-ordered key/value pairs.
    pub static_params: Vec<(String, String)>,
    pub creation_frame: i32,
}

/// What happened to an entity at some frame.
#[derive(Debug, Clone, PartialEq)]
pub enum EntityOp {
    /// Register (or re-register) the entity. `parent` is set for the "with parent" block variant.
    Define {
        parent: Option<i32>,
        def: EntityDef,
    },
    Undefine,
    SetPosition(Point),
    SetTransform(Transform),
    Log {
        category: String,
        message: String,
        color: Color,
    },
    /// A string-valued dynamic parameter.
    Parameter {
        label: String,
        value: String,
    },
    /// A numeric dynamic parameter.
    Value {
        label: String,
        value: f32,
    },
    Line {
        category: String,
        start: Point,
        end: Point,
        color: Color,
    },
    Circle {
        category: String,
        position: Point,
        up: Point,
        radius: f32,
        color: Color,
    },
    Sphere {
        category: String,
        center: Point,
        radius: f32,
        color: Color,
    },
    Capsule {
        category: String,
        p1: Point,
        p2: Point,
        radius: f32,
        color: Color,
    },
    Mesh {
        category: String,
        verts: Vec<Point>,
        color: Color,
    },
    Box {
        category: String,
        transform: Transform,
        dimensions: Point,
        color: Color,
    },
}

/// One block of a capture.
#[derive(Debug, Clone, PartialEq)]
pub enum Record {
    /// An empty marker block some writers emit at the start of compressed streams.
    Header,
    /// Closes the current frame. `total_time` is the time at which the next frame starts.
    FrameStep {
        total_time: f32,
    },
    Entity {
        frame: i32,
        id: i32,
        op: EntityOp,
    },
}

pub(crate) mod block {
    pub const FRAME_STEP: i32 = 1;
    pub const ENTITY_DEF: i32 = 2;
    pub const ENTITY_UNDEF: i32 = 3;
    pub const SET_POS: i32 = 4;
    pub const SET_TRANSFORM: i32 = 5;
    pub const LOG: i32 = 6;
    pub const PARAMETER: i32 = 7;
    pub const VALUE: i32 = 8;
    pub const LINE: i32 = 9;
    pub const CIRCLE: i32 = 10;
    pub const SPHERE: i32 = 11;
    pub const CAPSULE: i32 = 12;
    pub const MESH: i32 = 13;
    pub const BOX: i32 = 14;
    pub const ENTITY_DEF_WITH_PARENT: i32 = 15;
    pub const HEADER: i32 = 0xFF;
}
