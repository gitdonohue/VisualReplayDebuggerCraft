use std::io;

/// Everything that can go wrong while reading or writing a capture.
#[derive(Debug, thiserror::Error)]
pub enum FormatError {
    #[error("the input is empty")]
    Empty,
    #[error("invalid block type {0}; probably not a valid replay file")]
    InvalidBlockType(i32),
    #[error("malformed 7-bit encoded integer")]
    BadVarInt,
    #[error("string of {0} bytes exceeds the supported maximum")]
    StringTooLong(usize),
    #[error("invalid element count {0}")]
    BadCount(i32),
    #[error("i/o error: {0}")]
    Io(#[from] io::Error),
}
