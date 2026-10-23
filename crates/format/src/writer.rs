use std::io::{self, Write};

use flate2::Compression as Level;
use flate2::write::DeflateEncoder;

use crate::record::block;
use crate::{Color, EntityDef, EntityOp, Point, Record, Transform};

/// How [`encode`] packs the stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    /// Uncompressed, introduced by the int32 `0xFF` marker.
    None,
    /// Raw deflate, as written by the original serializers.
    Deflate,
}

/// Encodes records into a complete capture.
pub fn encode(records: &[Record], compression: Compression) -> io::Result<Vec<u8>> {
    match compression {
        Compression::None => {
            let mut out = Vec::new();
            out.extend_from_slice(&(block::HEADER as u32).to_le_bytes());
            let mut w = RecordWriter::new(&mut out);
            for r in records {
                w.write_record(r)?;
            }
            Ok(out)
        }
        Compression::Deflate => {
            let mut enc = DeflateEncoder::new(Vec::new(), Level::fast());
            {
                let mut w = RecordWriter::new(&mut enc);
                for r in records {
                    w.write_record(r)?;
                }
            }
            enc.finish()
        }
    }
}

/// Writes blocks to an output stream. It adds no container framing; see [`encode`] for that.
pub struct RecordWriter<W: Write> {
    w: W,
}

impl<W: Write> RecordWriter<W> {
    pub fn new(w: W) -> Self {
        Self { w }
    }

    pub fn into_inner(self) -> W {
        self.w
    }

    pub fn write_record(&mut self, record: &Record) -> io::Result<()> {
        match record {
            Record::Header => self.var_i32(block::HEADER),
            Record::FrameStep { total_time } => {
                self.var_i32(block::FRAME_STEP)?;
                self.f32(*total_time)
            }
            Record::Entity { frame, id, op } => self.entity(*frame, *id, op),
        }
    }

    fn entity(&mut self, frame: i32, id: i32, op: &EntityOp) -> io::Result<()> {
        let block_type = match op {
            EntityOp::Define { parent: None, .. } => block::ENTITY_DEF,
            EntityOp::Define { parent: Some(_), .. } => block::ENTITY_DEF_WITH_PARENT,
            EntityOp::Undefine => block::ENTITY_UNDEF,
            EntityOp::SetPosition(_) => block::SET_POS,
            EntityOp::SetTransform(_) => block::SET_TRANSFORM,
            EntityOp::Log { .. } => block::LOG,
            EntityOp::Parameter { .. } => block::PARAMETER,
            EntityOp::Value { .. } => block::VALUE,
            EntityOp::Line { .. } => block::LINE,
            EntityOp::Circle { .. } => block::CIRCLE,
            EntityOp::Sphere { .. } => block::SPHERE,
            EntityOp::Capsule { .. } => block::CAPSULE,
            EntityOp::Mesh { .. } => block::MESH,
            EntityOp::Box { .. } => block::BOX,
        };
        self.var_i32(block_type)?;
        self.var_i32(frame)?;
        self.var_i32(id)?;

        match op {
            EntityOp::Define { parent, def } => {
                if let Some(parent) = parent {
                    self.var_i32(*parent)?;
                }
                self.entity_def(def)
            }
            EntityOp::Undefine => Ok(()),
            EntityOp::SetPosition(p) => self.point(*p),
            EntityOp::SetTransform(t) => self.transform(*t),
            EntityOp::Log { category, message, color } => {
                self.string(category)?;
                self.string(message)?;
                self.color(*color)
            }
            EntityOp::Parameter { label, value } => {
                self.string(label)?;
                self.string(value)
            }
            EntityOp::Value { label, value } => {
                self.string(label)?;
                self.f32(*value)
            }
            EntityOp::Line { category, start, end, color } => {
                self.string(category)?;
                self.point(*start)?;
                self.point(*end)?;
                self.color(*color)
            }
            EntityOp::Circle { category, position, up, radius, color } => {
                self.string(category)?;
                self.point(*position)?;
                self.point(*up)?;
                self.f32(*radius)?;
                self.color(*color)
            }
            EntityOp::Sphere { category, center, radius, color } => {
                self.string(category)?;
                self.point(*center)?;
                self.f32(*radius)?;
                self.color(*color)
            }
            EntityOp::Capsule { category, p1, p2, radius, color } => {
                self.string(category)?;
                self.point(*p1)?;
                self.point(*p2)?;
                self.f32(*radius)?;
                self.color(*color)
            }
            EntityOp::Mesh { category, verts, color } => {
                self.string(category)?;
                self.w.write_all(&(verts.len() as i32).to_le_bytes())?;
                for v in verts {
                    self.point(*v)?;
                }
                self.color(*color)
            }
            EntityOp::Box { category, transform, dimensions, color } => {
                self.string(category)?;
                self.transform(*transform)?;
                self.point(*dimensions)?;
                self.color(*color)
            }
        }
    }

    fn entity_def(&mut self, def: &EntityDef) -> io::Result<()> {
        self.var_i32(def.id)?;
        self.string(&def.name)?;
        self.string(&def.path)?;
        self.string(&def.type_name)?;
        self.string(&def.category)?;
        self.transform(def.initial_transform)?;
        self.var_i32(def.static_params.len() as i32)?;
        for (k, v) in &def.static_params {
            self.string(k)?;
            self.string(v)?;
        }
        self.var_i32(def.creation_frame)
    }

    fn var_i32(&mut self, value: i32) -> io::Result<()> {
        let mut v = value as u32;
        while v >= 0x80 {
            self.w.write_all(&[(v as u8) | 0x80])?;
            v >>= 7;
        }
        self.w.write_all(&[v as u8])
    }

    fn f32(&mut self, v: f32) -> io::Result<()> {
        self.w.write_all(&v.to_le_bytes())
    }

    fn string(&mut self, s: &str) -> io::Result<()> {
        self.var_i32(s.len() as i32)?;
        self.w.write_all(s.as_bytes())
    }

    fn point(&mut self, p: Point) -> io::Result<()> {
        self.f32(p.x)?;
        self.f32(p.y)?;
        self.f32(p.z)
    }

    fn transform(&mut self, t: Transform) -> io::Result<()> {
        self.point(t.translation)?;
        self.f32(t.rotation.x)?;
        self.f32(t.rotation.y)?;
        self.f32(t.rotation.z)?;
        self.f32(t.rotation.w)
    }

    fn color(&mut self, c: Color) -> io::Result<()> {
        self.var_i32(c.0 as i32)
    }
}
