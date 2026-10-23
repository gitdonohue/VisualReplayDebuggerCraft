//! The CPU side of the 3D viewport: camera, tessellation and scene building. No GPU code.

mod build;
mod camera;
pub mod mesh;

pub use build::{
    FrameRequest, Label, Marker, ModelInstance, Pickable, SceneAssets, SceneFrame, StaticModel, ViewOptions,
    build_frame, focus_target, follow_pose, frame_all, grid, pick,
};
pub use camera::{Axis, Camera, Projected, default_camera_forward};
pub use glam;
pub use mesh::{LineSegment, Vertex, linear_color, linear_rgba, transform_matrix};
pub use vrd_model::Color;
