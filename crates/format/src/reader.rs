use std::io::{self, BufReader, Read};

use flate2::read::DeflateDecoder;

use crate::record::block;
use crate::{Color, EntityDef, EntityOp, FormatError, Point, Quat, Record, Transform};

const MAX_STRING_LEN: usize = 16 << 20;
const RAW_MARKER: u32 = block::HEADER as u32;

/// The result of decoding a capture.
#[derive(Debug, Clone, Default)]
pub struct Decoded {
    pub records: Vec<Record>,
    /// Whether the stream was deflate-compressed.
    pub compressed: bool,
    /// The stream ended in the middle of a block, or became unreadable after valid data.
    /// This is normal for a capture that was still being written.
    pub truncated: bool,
    /// Why decoding stopped early, if it did not stop at a clean end of stream.
    pub warning: Option<String>,
}

/// Decodes a whole capture held in memory.
///
/// Errors are only returned when the input does not look like a capture at all. Damage after the
/// first valid block yields the records read so far with [`Decoded::truncated`] set.
pub fn decode(bytes: &[u8]) -> Result<Decoded, FormatError> {
    if bytes.is_empty() {
        return Err(FormatError::Empty);
    }

    let raw = bytes.len() >= 4 && u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) == RAW_MARKER;
    let mut out = Decoded { compressed: !raw, ..Decoded::default() };

    let source: Box<dyn Read + '_> = if raw { Box::new(&bytes[4..]) } else { Box::new(DeflateDecoder::new(bytes)) };
    let mut reader = RecordReader::new(source);

    loop {
        match reader.read_record() {
            Ok(Some(record)) => out.records.push(record),
            Ok(None) => break,
            Err(e)
                if out.records.is_empty()
                    && !matches!(&e, FormatError::Io(io) if io.kind() == io::ErrorKind::UnexpectedEof) =>
            {
                return Err(e);
            }
            Err(e) => {
                out.truncated = true;
                if !matches!(&e, FormatError::Io(io) if io.kind() == io::ErrorKind::UnexpectedEof) {
                    out.warning = Some(e.to_string());
                }
                break;
            }
        }
    }
    Ok(out)
}

/// Reads blocks one at a time from an already-decompressed byte stream.
pub struct RecordReader<R: Read> {
    r: BufReader<R>,
}

impl<R: Read> RecordReader<R> {
    pub fn new(inner: R) -> Self {
        Self { r: BufReader::with_capacity(64 << 10, inner) }
    }

    /// Reads the next block. Returns `Ok(None)` at a clean end of stream (between blocks).
    pub fn read_record(&mut self) -> Result<Option<Record>, FormatError> {
        let Some(block_type) = self.first_var_i32()? else {
            return Ok(None);
        };

        let record = match block_type {
            block::HEADER => Record::Header,
            block::FRAME_STEP => Record::FrameStep { total_time: self.f32()? },
            block::ENTITY_DEF..=block::BOX | block::ENTITY_DEF_WITH_PARENT => self.entity_block(block_type)?,
            other => return Err(FormatError::InvalidBlockType(other)),
        };
        Ok(Some(record))
    }

    fn entity_block(&mut self, block_type: i32) -> Result<Record, FormatError> {
        let frame = self.var_i32()?;
        let id = self.var_i32()?;

        let op = match block_type {
            block::ENTITY_DEF | block::ENTITY_DEF_WITH_PARENT => {
                let parent = if block_type == block::ENTITY_DEF_WITH_PARENT { Some(self.var_i32()?) } else { None };
                EntityOp::Define { parent, def: self.entity_def()? }
            }
            block::ENTITY_UNDEF => EntityOp::Undefine,
            block::SET_POS => EntityOp::SetPosition(self.point()?),
            block::SET_TRANSFORM => EntityOp::SetTransform(self.transform()?),
            block::LOG => EntityOp::Log { category: self.string()?, message: self.string()?, color: self.color()? },
            block::PARAMETER => EntityOp::Parameter { label: self.string()?, value: self.string()? },
            block::VALUE => EntityOp::Value { label: self.string()?, value: self.f32()? },
            block::LINE => EntityOp::Line {
                category: self.string()?,
                start: self.point()?,
                end: self.point()?,
                color: self.color()?,
            },
            block::CIRCLE => EntityOp::Circle {
                category: self.string()?,
                position: self.point()?,
                up: self.point()?,
                radius: self.f32()?,
                color: self.color()?,
            },
            block::SPHERE => EntityOp::Sphere {
                category: self.string()?,
                center: self.point()?,
                radius: self.f32()?,
                color: self.color()?,
            },
            block::CAPSULE => EntityOp::Capsule {
                category: self.string()?,
                p1: self.point()?,
                p2: self.point()?,
                radius: self.f32()?,
                color: self.color()?,
            },
            block::MESH => {
                let category = self.string()?;
                let count = self.i32()?;
                if count < 0 {
                    return Err(FormatError::BadCount(count));
                }
                // Do not trust the count for allocation: a corrupt value must not exhaust memory.
                let mut verts = Vec::with_capacity((count as usize).min(1 << 16));
                for _ in 0..count {
                    verts.push(self.point()?);
                }
                EntityOp::Mesh { category, verts, color: self.color()? }
            }
            block::BOX => EntityOp::Box {
                category: self.string()?,
                transform: self.transform()?,
                dimensions: self.point()?,
                color: self.color()?,
            },
            _ => unreachable!("filtered by the caller"),
        };
        Ok(Record::Entity { frame, id, op })
    }

    fn entity_def(&mut self) -> Result<EntityDef, FormatError> {
        let id = self.var_i32()?;
        let name = self.string()?;
        let path = self.string()?;
        let type_name = self.string()?;
        let category = self.string()?;
        let initial_transform = self.transform()?;
        let count = self.var_i32()?;
        if count < 0 {
            return Err(FormatError::BadCount(count));
        }
        let mut static_params = Vec::with_capacity((count as usize).min(1024));
        for _ in 0..count {
            static_params.push((self.string()?, self.string()?));
        }
        let creation_frame = self.var_i32()?;
        Ok(EntityDef { id, name, path, type_name, category, initial_transform, static_params, creation_frame })
    }

    fn u8(&mut self) -> io::Result<u8> {
        let mut b = [0u8; 1];
        self.r.read_exact(&mut b)?;
        Ok(b[0])
    }

    /// Like `var_i32`, but a stream that ends before the first byte is `None` rather than an error.
    fn first_var_i32(&mut self) -> Result<Option<i32>, FormatError> {
        let mut b = [0u8; 1];
        loop {
            match self.r.read(&mut b) {
                Ok(0) => return Ok(None),
                Ok(_) => break,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e.into()),
            }
        }
        self.finish_var_i32(b[0]).map(Some)
    }

    fn var_i32(&mut self) -> Result<i32, FormatError> {
        let first = self.u8()?;
        self.finish_var_i32(first)
    }

    fn finish_var_i32(&mut self, first: u8) -> Result<i32, FormatError> {
        let mut result = u32::from(first & 0x7F);
        let mut byte = first;
        let mut shift = 7;
        while byte & 0x80 != 0 {
            if shift >= 35 {
                return Err(FormatError::BadVarInt);
            }
            byte = self.u8()?;
            result |= u32::from(byte & 0x7F) << shift;
            shift += 7;
        }
        Ok(result as i32)
    }

    fn i32(&mut self) -> io::Result<i32> {
        let mut b = [0u8; 4];
        self.r.read_exact(&mut b)?;
        Ok(i32::from_le_bytes(b))
    }

    fn f32(&mut self) -> io::Result<f32> {
        let mut b = [0u8; 4];
        self.r.read_exact(&mut b)?;
        Ok(f32::from_le_bytes(b))
    }

    fn string(&mut self) -> Result<String, FormatError> {
        let len = self.var_i32()?;
        if len < 0 {
            return Err(FormatError::BadCount(len));
        }
        let len = len as usize;
        if len > MAX_STRING_LEN {
            return Err(FormatError::StringTooLong(len));
        }
        let mut buf = Vec::with_capacity(len.min(4096));
        let read = (&mut self.r).take(len as u64).read_to_end(&mut buf)?;
        if read < len {
            return Err(io::Error::from(io::ErrorKind::UnexpectedEof).into());
        }
        // Captures are written as ASCII; tolerate anything else rather than rejecting the file.
        Ok(String::from_utf8_lossy(&buf).into_owned())
    }

    fn point(&mut self) -> io::Result<Point> {
        Ok(Point { x: self.f32()?, y: self.f32()?, z: self.f32()? })
    }

    fn quat(&mut self) -> io::Result<Quat> {
        Ok(Quat { x: self.f32()?, y: self.f32()?, z: self.f32()?, w: self.f32()? })
    }

    fn transform(&mut self) -> io::Result<Transform> {
        Ok(Transform { translation: self.point()?, rotation: self.quat()? })
    }

    fn color(&mut self) -> Result<Color, FormatError> {
        Ok(Color(self.var_i32()? as u32))
    }
}
