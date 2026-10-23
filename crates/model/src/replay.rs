use std::collections::{BTreeMap, BTreeSet};

use vrd_format::{Color, Point, Transform};

use crate::{FrameList, LoadError};

/// How a capture was read from disk.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoadInfo {
    pub compressed: bool,
    /// The capture ended mid-stream (for instance, it was still being recorded).
    pub truncated: bool,
    pub warning: Option<String>,
    pub record_count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entity {
    pub id: i32,
    pub parent: Option<i32>,
    pub name: String,
    pub path: String,
    pub type_name: String,
    pub category: String,
    pub initial_transform: Transform,
    pub static_params: Vec<(String, String)>,
    pub creation_frame: i32,
    /// The frame of the most recent (re-)registration.
    pub registration_frame: i32,
    /// The frame at which the entity was unregistered, if it was.
    pub destroyed_frame: Option<i32>,
    /// Transform history. Position-only updates are merged into full transforms.
    pub transforms: FrameList<Transform>,
    /// String parameter history, per parameter name.
    pub params: BTreeMap<String, FrameList<String>>,
    /// Numeric parameter history, per parameter name.
    pub values: BTreeMap<String, FrameList<f32>>,
    /// Frames that have at least one log entry from this entity, ascending and unique.
    pub log_frames: Vec<i32>,
    /// Indices into [`Replay::draws`] of the persistent "creation" draws (e.g. the entity's mesh).
    pub creation_draws: Vec<usize>,
    pub has_draws: bool,
    pub defined: bool,
}

impl Entity {
    pub(crate) fn placeholder(id: i32) -> Self {
        Self {
            id,
            parent: None,
            name: String::new(),
            path: String::new(),
            type_name: String::new(),
            category: String::new(),
            initial_transform: Transform::IDENTITY,
            static_params: Vec::new(),
            creation_frame: 0,
            registration_frame: 0,
            destroyed_frame: None,
            transforms: FrameList::new(),
            params: BTreeMap::new(),
            values: BTreeMap::new(),
            log_frames: Vec::new(),
            creation_draws: Vec::new(),
            has_draws: false,
            defined: false,
        }
    }

    /// The entity's name, or a generated one if the capture never named it.
    pub fn display_name(&self) -> String {
        if self.name.is_empty() { format!("Entity {}", self.id) } else { self.name.clone() }
    }

    pub fn has_logs(&self) -> bool {
        !self.log_frames.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LogEntry {
    pub frame: i32,
    pub entity: i32,
    pub category: String,
    pub message: String,
    pub color: Color,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Shape {
    Line { start: Point, end: Point },
    Circle { position: Point, up: Point, radius: f32 },
    Sphere { center: Point, radius: f32 },
    Capsule { p1: Point, p2: Point, radius: f32 },
    Mesh { verts: Vec<Point> },
    Box { transform: Transform, dimensions: Point },
}

#[derive(Debug, Clone, PartialEq)]
pub struct DrawCommand {
    pub frame: i32,
    pub entity: i32,
    pub category: String,
    pub color: Color,
    pub shape: Shape,
    /// A persistent draw made when the entity was registered (empty category), such as its mesh.
    /// Other draws only exist for the single frame they were issued on.
    pub is_creation: bool,
}

/// A fully indexed capture.
#[derive(Debug, Clone, Default)]
pub struct Replay {
    pub(crate) frame_times: Vec<f32>,
    pub(crate) entities: BTreeMap<i32, Entity>,
    pub(crate) roots: Vec<i32>,
    pub(crate) children: BTreeMap<i32, Vec<i32>>,
    pub(crate) logs: Vec<LogEntry>,
    pub(crate) draws: Vec<DrawCommand>,
    pub(crate) entity_categories: BTreeSet<String>,
    pub(crate) log_categories: BTreeSet<String>,
    pub(crate) log_colors: BTreeSet<Color>,
    pub(crate) draw_categories: BTreeSet<String>,
    pub(crate) draw_colors: BTreeSet<Color>,
    pub(crate) param_names: BTreeSet<String>,
    pub(crate) info: LoadInfo,
}

impl Replay {
    /// Decodes and indexes a capture held in memory.
    pub fn from_bytes(bytes: &[u8]) -> Result<Replay, LoadError> {
        let decoded = vrd_format::decode(bytes)?;
        let info = LoadInfo {
            compressed: decoded.compressed,
            truncated: decoded.truncated,
            warning: decoded.warning,
            record_count: decoded.records.len(),
        };
        Ok(Replay::from_records(decoded.records, info))
    }

    pub fn info(&self) -> &LoadInfo {
        &self.info
    }

    // ---- time -----------------------------------------------------------------------------

    /// Number of frames, counting the open frame after the last step.
    pub fn frame_count(&self) -> usize {
        self.frame_times.len()
    }

    /// The highest valid frame number.
    pub fn last_frame(&self) -> i32 {
        self.frame_times.len().saturating_sub(1) as i32
    }

    /// The time at which each frame starts. Always begins with `0.0`.
    pub fn frame_times(&self) -> &[f32] {
        &self.frame_times
    }

    pub fn total_time(&self) -> f64 {
        self.frame_times.last().copied().map_or(0.0, f64::from)
    }

    /// The frame that is current at time `t`: the last frame that has started by then.
    pub fn frame_for_time(&self, t: f64) -> i32 {
        if t.is_nan() || t <= 0.0 {
            return 0;
        }
        let started = self.frame_times.partition_point(|&ft| f64::from(ft) <= t);
        (started.saturating_sub(1) as i32).min(self.last_frame())
    }

    /// The time at which `frame` starts. Frames outside the capture clamp to its ends.
    pub fn time_for_frame(&self, frame: i32) -> f64 {
        if frame <= 0 {
            return 0.0;
        }
        self.frame_times.get(frame as usize).or(self.frame_times.last()).copied().map_or(0.0, f64::from)
    }

    // ---- entities -------------------------------------------------------------------------

    pub fn entity(&self, id: i32) -> Option<&Entity> {
        self.entities.get(&id)
    }

    /// All entities, ordered by id.
    pub fn entities(&self) -> impl Iterator<Item = &Entity> {
        self.entities.values()
    }

    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }

    /// Entities with no (valid) parent, ordered by id.
    pub fn root_entities(&self) -> &[i32] {
        &self.roots
    }

    /// Children of `id`, ordered by id.
    pub fn children_of(&self, id: i32) -> &[i32] {
        self.children.get(&id).map_or(&[], Vec::as_slice)
    }

    /// First and last frame (inclusive) during which the entity exists.
    pub fn lifetime(&self, id: i32) -> Option<(i32, i32)> {
        let e = self.entities.get(&id)?;
        Some((e.creation_frame, e.destroyed_frame.unwrap_or(self.last_frame())))
    }

    pub fn is_alive_at(&self, id: i32, frame: i32) -> bool {
        self.lifetime(id).is_some_and(|(start, end)| frame >= start && frame <= end)
    }

    /// The entity's transform at `frame`: its latest recorded one, else its initial transform.
    pub fn transform_at(&self, id: i32, frame: i32) -> Option<Transform> {
        let e = self.entities.get(&id)?;
        Some(e.transforms.at_or_before(frame).copied().unwrap_or(e.initial_transform))
    }

    pub fn position_at(&self, id: i32, frame: i32) -> Option<Point> {
        self.transform_at(id, frame).map(|t| t.translation)
    }

    /// String parameters as of `frame`, sorted by name.
    pub fn params_at(&self, id: i32, frame: i32) -> Vec<(&str, &str)> {
        let Some(e) = self.entities.get(&id) else {
            return Vec::new();
        };
        e.params.iter().filter_map(|(k, list)| list.at_or_before(frame).map(|v| (k.as_str(), v.as_str()))).collect()
    }

    /// Numeric parameters as of `frame`, sorted by name.
    pub fn values_at(&self, id: i32, frame: i32) -> Vec<(&str, f32)> {
        let Some(e) = self.entities.get(&id) else {
            return Vec::new();
        };
        e.values.iter().filter_map(|(k, list)| list.at_or_before(frame).map(|v| (k.as_str(), *v))).collect()
    }

    /// Everything known about an entity at `frame` as display rows, in the order the original
    /// debugger showed them: identity, position, static parameters, then dynamic ones.
    pub fn properties_at(&self, id: i32, frame: i32) -> Vec<(String, String)> {
        let Some(e) = self.entities.get(&id) else {
            return Vec::new();
        };
        let mut rows = vec![
            ("Name".to_string(), e.display_name()),
            ("Path".to_string(), e.path.clone()),
            (
                "Id".to_string(),
                match e.parent {
                    Some(p) => format!("{}({})", e.id, p),
                    None => format!("{}(-1)", e.id),
                },
            ),
        ];
        if !self.is_alive_at(id, frame) {
            return rows;
        }
        if let Some(p) = self.position_at(id, frame) {
            rows.push(("Position".to_string(), format!("({}, {}, {})", p.x, p.y, p.z)));
        }
        rows.extend(e.static_params.iter().cloned());
        rows.extend(self.params_at(id, frame).into_iter().map(|(k, v)| (k.to_string(), v.to_string())));
        rows.extend(self.values_at(id, frame).into_iter().map(|(k, v)| (k.to_string(), v.to_string())));
        rows
    }

    // ---- logs and draws ---------------------------------------------------------------------

    /// All log entries, ordered by frame.
    pub fn logs(&self) -> &[LogEntry] {
        &self.logs
    }

    /// Log entries recorded in `start..=end`.
    pub fn logs_in(&self, start: i32, end: i32) -> &[LogEntry] {
        let lo = self.logs.partition_point(|l| l.frame < start);
        let hi = self.logs.partition_point(|l| l.frame <= end).max(lo);
        &self.logs[lo..hi]
    }

    /// All draw commands, ordered by frame.
    pub fn draws(&self) -> &[DrawCommand] {
        &self.draws
    }

    /// Draw commands issued on exactly `frame`, including creation draws.
    pub fn draws_at(&self, frame: i32) -> &[DrawCommand] {
        self.draws_in(frame, frame)
    }

    /// Draw commands issued in `start..=end`, including creation draws.
    pub fn draws_in(&self, start: i32, end: i32) -> &[DrawCommand] {
        let lo = self.draws.partition_point(|d| d.frame < start);
        let hi = self.draws.partition_point(|d| d.frame <= end).max(lo);
        &self.draws[lo..hi]
    }

    /// The persistent creation draws of an entity (its mesh and other registration-time draws).
    pub fn creation_draws(&self, id: i32) -> impl Iterator<Item = &DrawCommand> {
        self.entities.get(&id).into_iter().flat_map(|e| e.creation_draws.iter()).map(|&i| &self.draws[i])
    }

    // ---- categories -------------------------------------------------------------------------

    pub fn entity_categories(&self) -> &BTreeSet<String> {
        &self.entity_categories
    }

    pub fn log_categories(&self) -> &BTreeSet<String> {
        &self.log_categories
    }

    pub fn log_colors(&self) -> &BTreeSet<Color> {
        &self.log_colors
    }

    /// Categories of per-frame (non-creation) draws.
    pub fn draw_categories(&self) -> &BTreeSet<String> {
        &self.draw_categories
    }

    /// Colours used by any draw command.
    pub fn draw_colors(&self) -> &BTreeSet<Color> {
        &self.draw_colors
    }

    /// Names of all dynamic parameters, string and numeric.
    pub fn param_names(&self) -> &BTreeSet<String> {
        &self.param_names
    }
}
