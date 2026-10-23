//! Reader and writer for the VisualReplayDebugger `.vrd` capture format.
//!
//! A capture is a flat stream of blocks. The stream is either raw-deflate compressed or, when it
//! starts with the little-endian int32 `0xFF`, uncompressed. The stream never needs to be closed
//! to be valid, so a truncated capture decodes up to its last complete block.
//!
//! See `docs/format.md` for the byte-level description.

mod color;
mod error;
mod reader;
mod record;
mod types;
mod writer;

pub use color::Color;
pub use error::FormatError;
pub use reader::{Decoded, RecordReader, decode};
pub use record::{EntityDef, EntityOp, Record};
pub use types::{Point, Quat, Transform};
pub use writer::{Compression, RecordWriter, encode};
