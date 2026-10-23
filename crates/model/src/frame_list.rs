/// Values stamped with the frame they were recorded at, kept sorted by frame.
///
/// Several values may share a frame. Lookups return the most recent value at or before a frame,
/// which is how state-like data (transforms, parameters) is meant to be read.
#[derive(Debug, Clone, PartialEq)]
pub struct FrameList<T> {
    frames: Vec<i32>,
    values: Vec<T>,
}

impl<T> Default for FrameList<T> {
    fn default() -> Self {
        Self { frames: Vec::new(), values: Vec::new() }
    }
}

impl<T> FrameList<T> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends a value. Out-of-order frames are inserted in place; ties keep insertion order.
    pub fn push(&mut self, frame: i32, value: T) {
        if self.frames.last().is_none_or(|&last| last <= frame) {
            self.frames.push(frame);
            self.values.push(value);
        } else {
            let at = self.frames.partition_point(|&f| f <= frame);
            self.frames.insert(at, frame);
            self.values.insert(at, value);
        }
    }

    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn frames(&self) -> &[i32] {
        &self.frames
    }

    pub fn values(&self) -> &[T] {
        &self.values
    }

    pub fn last(&self) -> Option<(i32, &T)> {
        Some((*self.frames.last()?, self.values.last()?))
    }

    pub fn iter(&self) -> impl DoubleEndedIterator<Item = (i32, &T)> + ExactSizeIterator {
        self.frames.iter().copied().zip(self.values.iter())
    }

    /// The most recent value recorded at or before `frame`.
    pub fn at_or_before(&self, frame: i32) -> Option<&T> {
        let n = self.frames.partition_point(|&f| f <= frame);
        n.checked_sub(1).map(|i| &self.values[i])
    }

    /// Entries recorded in `start..=end`.
    pub fn range(&self, start: i32, end: i32) -> impl DoubleEndedIterator<Item = (i32, &T)> + ExactSizeIterator {
        let lo = self.frames.partition_point(|&f| f < start);
        let hi = self.frames.partition_point(|&f| f <= end).max(lo);
        self.frames[lo..hi].iter().copied().zip(self.values[lo..hi].iter())
    }

    /// Index of the first entry at or after `frame`.
    pub fn lower_bound(&self, frame: i32) -> usize {
        self.frames.partition_point(|&f| f < frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn list() -> FrameList<&'static str> {
        let mut l = FrameList::new();
        l.push(2, "a");
        l.push(5, "b");
        l.push(5, "c");
        l.push(9, "d");
        l
    }

    #[test]
    fn at_or_before_returns_latest_state() {
        let l = list();
        assert_eq!(l.at_or_before(1), None);
        assert_eq!(l.at_or_before(2), Some(&"a"));
        assert_eq!(l.at_or_before(4), Some(&"a"));
        assert_eq!(l.at_or_before(5), Some(&"c"));
        assert_eq!(l.at_or_before(100), Some(&"d"));
    }

    #[test]
    fn range_is_inclusive() {
        let l = list();
        let got: Vec<_> = l.range(5, 9).map(|(_, v)| *v).collect();
        assert_eq!(got, ["b", "c", "d"]);
        assert_eq!(l.range(6, 8).count(), 0);
        assert_eq!(l.range(9, 2).count(), 0);
    }

    #[test]
    fn out_of_order_pushes_are_sorted() {
        let mut l = FrameList::new();
        l.push(5, 'x');
        l.push(1, 'y');
        l.push(5, 'z');
        assert_eq!(l.frames(), &[1, 5, 5]);
        assert_eq!(l.values(), &['y', 'x', 'z']);
    }
}
