//! An indexed, queryable model of a replay capture.
//!
//! [`Replay`] turns the flat record stream from `vrd-format` into per-entity timelines, a global
//! log list and draw list, and a frame/time mapping. Everything is keyed by frame number; frame
//! `f` spans the times `frame_times[f] .. frame_times[f + 1]`.

mod build;
mod frame_list;
mod replay;

pub use frame_list::FrameList;
pub use replay::{DrawCommand, Entity, LoadInfo, LogEntry, Replay, Shape};

pub use vrd_format::{Color, Point, Quat, Transform};

/// Errors from loading a capture.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error(transparent)]
    Format(#[from] vrd_format::FormatError),
}
