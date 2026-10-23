/// A 3D point or vector, single precision as stored in captures.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Point {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Point {
    pub const ZERO: Point = Point { x: 0.0, y: 0.0, z: 0.0 };

    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }
}

/// A rotation quaternion, stored as x, y, z, w.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Quat {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

impl Quat {
    pub const IDENTITY: Quat = Quat { x: 0.0, y: 0.0, z: 0.0, w: 1.0 };
}

impl Default for Quat {
    fn default() -> Self {
        Self::IDENTITY
    }
}

/// A translation plus a rotation.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Transform {
    pub translation: Point,
    pub rotation: Quat,
}

impl Transform {
    pub const IDENTITY: Transform = Transform { translation: Point::ZERO, rotation: Quat::IDENTITY };
}
