//! GPU rendering for the 3D viewport. See [`Renderer`].

mod renderer;

pub use renderer::{COLOR_FORMAT, RenderParams, Renderer};
pub use wgpu;
