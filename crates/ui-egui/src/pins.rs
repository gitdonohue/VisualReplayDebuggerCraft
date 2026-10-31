/// Entities kept in the timeline panels (graphs, parameter timelines) whatever is selected.
#[derive(Debug, Clone, Default)]
pub struct Pins(Vec<i32>);

impl Pins {
    /// The most entities a panel shows at once.
    pub const MAX: usize = 10;

    /// Pins an entity, or unpins it. Nothing is pinned beyond [`Self::MAX`].
    pub fn toggle(&mut self, entity: i32) {
        if let Some(i) = self.0.iter().position(|&p| p == entity) {
            self.0.remove(i);
        } else if self.0.len() < Self::MAX {
            self.0.push(entity);
        }
    }

    pub fn contains(&self, entity: i32) -> bool {
        self.0.contains(&entity)
    }

    pub fn as_slice(&self) -> &[i32] {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// What a panel shows: the pinned entities, then the selection, keeping those that have
    /// something to show there.
    pub fn shown(&self, selected: Option<i32>, has_data: impl Fn(i32) -> bool) -> Vec<i32> {
        let mut out: Vec<i32> = self.0.iter().copied().filter(|&id| has_data(id)).collect();
        if let Some(id) = selected.filter(|&id| has_data(id) && !out.contains(&id)) {
            out.push(id);
        }
        out.truncate(Self::MAX);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggling_pins_and_unpins_within_the_limit() {
        let mut p = Pins::default();
        p.toggle(1);
        p.toggle(2);
        assert_eq!(p.as_slice(), [1, 2]);
        p.toggle(1);
        assert_eq!(p.as_slice(), [2]);
        for id in 10..40 {
            p.toggle(id);
        }
        assert_eq!(p.as_slice().len(), Pins::MAX);
        assert!(p.contains(2));
        p.clear();
        assert!(p.is_empty());
    }

    #[test]
    fn the_selection_is_shown_after_the_pins_and_only_if_it_has_data() {
        let mut p = Pins::default();
        p.toggle(5);
        p.toggle(6);
        let has = |id: i32| id != 6 && id != 9;
        assert_eq!(p.shown(Some(7), has), [5, 7], "pin 6 has no data here");
        assert_eq!(p.shown(Some(5), has), [5], "no duplicate");
        assert_eq!(p.shown(Some(9), has), [5]);
        assert_eq!(p.shown(None, has), [5]);
        assert!(Pins::default().shown(None, has).is_empty());
    }
}
