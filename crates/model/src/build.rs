use std::collections::BTreeSet;

use vrd_format::{EntityOp, Record};

use crate::replay::{DrawCommand, Entity, LoadInfo, LogEntry, Replay, Shape};

impl Replay {
    /// Indexes a decoded record stream.
    pub fn from_records(records: Vec<Record>, info: LoadInfo) -> Replay {
        let mut r = Replay { frame_times: vec![0.0], info, ..Replay::default() };

        for record in records {
            match record {
                Record::Header => {}
                Record::FrameStep { total_time } => r.frame_times.push(total_time),
                Record::Entity { frame, id, op } => r.apply(frame, id, op),
            }
        }

        r.finish();
        r
    }

    fn entity_mut(&mut self, id: i32) -> &mut Entity {
        self.entities.entry(id).or_insert_with(|| Entity::placeholder(id))
    }

    fn apply(&mut self, frame: i32, id: i32, op: EntityOp) {
        match op {
            EntityOp::Define { parent, def } => {
                self.entity_categories.insert(def.category.clone());
                let e = self.entity_mut(id);
                // A repeated definition overrides the descriptive fields but keeps the history.
                if e.defined {
                    e.registration_frame = def.creation_frame;
                } else {
                    e.creation_frame = def.creation_frame;
                    e.registration_frame = def.creation_frame;
                }
                e.defined = true;
                e.parent = parent;
                e.name = def.name;
                e.path = def.path;
                e.type_name = def.type_name;
                e.category = def.category;
                e.initial_transform = def.initial_transform;
                e.static_params = def.static_params;
            }
            EntityOp::Undefine => self.entity_mut(id).destroyed_frame = Some(frame),
            EntityOp::SetPosition(p) => {
                let e = self.entity_mut(id);
                let mut t = e.transforms.last().map_or(e.initial_transform, |(_, t)| *t);
                t.translation = p;
                e.transforms.push(frame, t);
            }
            EntityOp::SetTransform(t) => self.entity_mut(id).transforms.push(frame, t),
            EntityOp::Log { category, message, color } => {
                self.log_categories.insert(category.clone());
                self.log_colors.insert(color);
                let e = self.entity_mut(id);
                if e.log_frames.last() != Some(&frame) {
                    e.log_frames.push(frame);
                }
                self.logs.push(LogEntry {
                    frame,
                    entity: id,
                    category,
                    message: message.replace(['\r', '\n'], " "),
                    color,
                });
            }
            EntityOp::Parameter { label, value } => {
                self.param_names.insert(label.clone());
                let list = self.entity_mut(id).params.entry(label).or_default();
                // Repeated sets of the same value carry no information.
                if list.last().is_none_or(|(_, last)| *last != value) {
                    list.push(frame, value);
                }
            }
            EntityOp::Value { label, value } => {
                self.param_names.insert(label.clone());
                self.entity_mut(id).values.entry(label).or_default().push(frame, value);
            }
            EntityOp::Line { category, start, end, color } => {
                self.draw(frame, id, category, color, Shape::Line { start, end })
            }
            EntityOp::Circle { category, position, up, radius, color } => {
                self.draw(frame, id, category, color, Shape::Circle { position, up, radius })
            }
            EntityOp::Sphere { category, center, radius, color } => {
                self.draw(frame, id, category, color, Shape::Sphere { center, radius })
            }
            EntityOp::Capsule { category, p1, p2, radius, color } => {
                self.draw(frame, id, category, color, Shape::Capsule { p1, p2, radius })
            }
            EntityOp::Mesh { category, verts, color } => self.draw(frame, id, category, color, Shape::Mesh { verts }),
            EntityOp::Box { category, transform, dimensions, color } => {
                self.draw(frame, id, category, color, Shape::Box { transform, dimensions })
            }
        }
    }

    fn draw(&mut self, frame: i32, entity: i32, category: String, color: vrd_format::Color, shape: Shape) {
        self.entity_mut(entity).has_draws = true;
        self.draws.push(DrawCommand { frame, entity, category, color, shape, is_creation: false });
    }

    /// Sorts the frame-ordered lists and derives everything that needs the full picture.
    fn finish(&mut self) {
        // Records arrive in frame order from every known writer; the sort is a cheap no-op then.
        self.logs.sort_by_key(|l| l.frame);
        self.draws.sort_by_key(|d| d.frame);

        for (index, d) in self.draws.iter_mut().enumerate() {
            self.draw_colors.insert(d.color);
            let Some(e) = self.entities.get_mut(&d.entity) else {
                continue;
            };
            d.is_creation = d.category.is_empty() && (d.frame == e.creation_frame || d.frame == e.registration_frame);
            if d.is_creation {
                e.creation_draws.push(index);
            } else if !d.category.is_empty() {
                self.draw_categories.insert(d.category.clone());
            }
        }

        self.build_hierarchy();
    }

    fn build_hierarchy(&mut self) {
        self.roots.clear();
        self.children.clear();

        let ids: Vec<i32> = self.entities.keys().copied().collect();
        for id in ids {
            match self.effective_parent(id) {
                Some(p) => self.children.entry(p).or_default().push(id),
                None => self.roots.push(id),
            }
        }
    }

    /// The entity's parent if it exists and following parents never loops back to the entity.
    fn effective_parent(&self, id: i32) -> Option<i32> {
        let parent = self.entities.get(&id)?.parent.filter(|&p| p >= 0 && p != id)?;
        if !self.entities.contains_key(&parent) {
            return None;
        }
        let mut seen = BTreeSet::from([id]);
        let mut cursor = parent;
        while seen.insert(cursor) {
            match self.entities.get(&cursor).and_then(|e| e.parent) {
                Some(next) if self.entities.contains_key(&next) && next >= 0 => cursor = next,
                _ => return Some(parent),
            }
        }
        // The chain loops back on itself, so there is no sensible place in the tree for this entity.
        None
    }
}
