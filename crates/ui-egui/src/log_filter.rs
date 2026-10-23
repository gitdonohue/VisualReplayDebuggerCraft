use std::collections::BTreeSet;

use vrd_model::{LogEntry, Replay};

/// Which log entries the logs panel shows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LogFilter {
    /// Case-insensitive substring matched against the message, category and entity name.
    pub text: String,
    pub hidden_categories: BTreeSet<String>,
    /// Only entries logged by this entity.
    pub only_entity: Option<i32>,
}

impl LogFilter {
    /// Indices into [`Replay::logs`] of the entries that pass, in order.
    pub fn apply(&self, replay: &Replay) -> Vec<u32> {
        let needle = self.text.trim().to_ascii_lowercase();
        replay
            .logs()
            .iter()
            .enumerate()
            .filter(|(_, entry)| self.matches(replay, entry, &needle))
            .map(|(i, _)| i as u32)
            .collect()
    }

    fn matches(&self, replay: &Replay, entry: &LogEntry, needle: &str) -> bool {
        if self.only_entity.is_some_and(|id| id != entry.entity) {
            return false;
        }
        if self.hidden_categories.contains(&entry.category) {
            return false;
        }
        if needle.is_empty() {
            return true;
        }
        contains_ignore_case(&entry.message, needle)
            || contains_ignore_case(&entry.category, needle)
            || replay
                .entity(entry.entity)
                .is_some_and(|e| contains_ignore_case(&e.name, needle) || contains_ignore_case(&e.path, needle))
    }
}

/// `needle` must already be ASCII-lowercase.
pub(crate) fn contains_ignore_case(haystack: &str, needle: &str) -> bool {
    let (h, n) = (haystack.as_bytes(), needle.as_bytes());
    n.is_empty() || (h.len() >= n.len() && h.windows(n.len()).any(|w| w.eq_ignore_ascii_case(n)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use vrd_format::{Color, EntityDef, EntityOp, Record};
    use vrd_model::LoadInfo;

    fn replay() -> Replay {
        let define = |id, name: &str| Record::Entity {
            frame: 0,
            id,
            op: EntityOp::Define {
                parent: None,
                def: EntityDef { id, name: name.into(), path: format!("World/{name}"), ..EntityDef::default() },
            },
        };
        let log = |frame, id, category: &str, message: &str| Record::Entity {
            frame,
            id,
            op: EntityOp::Log { category: category.into(), message: message.into(), color: Color::WHITE },
        };
        Replay::from_records(
            vec![
                define(1, "Player"),
                define(2, "Goblin"),
                log(0, 1, "AI", "Looking for Targets"),
                log(0, 2, "Physics", "Collision with wall"),
                log(1, 2, "AI", "Attack!"),
                log(2, 1, "Net", "RPC sent"),
            ],
            LoadInfo::default(),
        )
    }

    #[test]
    fn empty_filter_passes_everything() {
        assert_eq!(LogFilter::default().apply(&replay()), [0, 1, 2, 3]);
    }

    #[test]
    fn text_matches_message_category_and_entity_ignoring_case() {
        let r = replay();
        let f = |text: &str| LogFilter { text: text.into(), ..LogFilter::default() };
        assert_eq!(f("TARGETS").apply(&r), [0]);
        assert_eq!(f("physics").apply(&r), [1]);
        assert_eq!(f("goblin").apply(&r), [1, 2]);
        assert_eq!(f("world/player").apply(&r), [0, 3]);
        assert_eq!(f("  attack  ").apply(&r), [2]);
        assert!(f("nothing like this").apply(&r).is_empty());
    }

    #[test]
    fn categories_and_entity_restrict_results() {
        let r = replay();
        let hidden = LogFilter { hidden_categories: ["AI".to_string()].into(), ..LogFilter::default() };
        assert_eq!(hidden.apply(&r), [1, 3]);

        let only = LogFilter { only_entity: Some(2), ..LogFilter::default() };
        assert_eq!(only.apply(&r), [1, 2]);

        let both =
            LogFilter { only_entity: Some(2), hidden_categories: ["AI".to_string()].into(), text: "wall".into() };
        assert_eq!(both.apply(&r), [1]);
    }

    #[test]
    fn substring_helper() {
        assert!(contains_ignore_case("Hello World", "o w"));
        assert!(contains_ignore_case("abc", ""));
        assert!(!contains_ignore_case("ab", "abc"));
    }
}
