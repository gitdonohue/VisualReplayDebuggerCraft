/// The span of time shown by timeline views such as the graphs. It can be zoomed and panned but
/// never leaves the capture.
#[derive(Debug, Clone, PartialEq)]
pub struct TimeWindow {
    start: f64,
    end: f64,
    total: f64,
    last_playhead: Option<f64>,
}

impl Default for TimeWindow {
    fn default() -> Self {
        Self::new(0.0)
    }
}

impl TimeWindow {
    /// The narrowest window, in seconds.
    pub const MIN_RANGE: f64 = 1.0e-3;
    /// How far past an edge the playhead may move before the window jumps instead of sliding.
    const SLIDE_LIMIT: f64 = 0.25;

    /// A window showing the whole of a capture `total` seconds long.
    pub fn new(total: f64) -> Self {
        let total = if total.is_finite() { total.max(0.0) } else { 0.0 };
        Self { start: 0.0, end: total, total, last_playhead: None }
    }

    pub fn start(&self) -> f64 {
        self.start
    }

    pub fn end(&self) -> f64 {
        self.end
    }

    pub fn range(&self) -> f64 {
        self.end - self.start
    }

    pub fn total(&self) -> f64 {
        self.total
    }

    /// Whether the whole capture is visible.
    pub fn is_full(&self) -> bool {
        self.start <= 0.0 && self.end >= self.total
    }

    /// Shows the whole capture.
    pub fn fill(&mut self) {
        self.start = 0.0;
        self.end = self.total;
    }

    /// Shows `start..end`, clamped to the capture and widened to [`Self::MIN_RANGE`] if needed.
    pub fn set(&mut self, start: f64, end: f64) {
        if !(start.is_finite() && end.is_finite()) {
            return;
        }
        let (start, end) = if start <= end { (start, end) } else { (end, start) };
        let range = (end - start).clamp(self.min_range(), self.total);
        let center = (start + end) * 0.5;
        self.place(center - range * 0.5, range);
    }

    /// Zooms by `factor` (below 1 zooms in) keeping the time `about` at the same place on screen.
    pub fn zoom(&mut self, factor: f64, about: f64) {
        if !(factor.is_finite() && factor > 0.0 && about.is_finite()) || self.total <= 0.0 {
            return;
        }
        let range = self.range();
        let new_range = (range * factor).clamp(self.min_range(), self.total);
        let fraction = if range > 0.0 { ((about - self.start) / range).clamp(0.0, 1.0) } else { 0.5 };
        self.place(about - fraction * new_range, new_range);
    }

    /// Slides the window by `dt` seconds, stopping at the ends of the capture.
    pub fn pan(&mut self, dt: f64) {
        if dt.is_finite() {
            let range = self.range();
            self.place(self.start + dt, range);
        }
    }

    /// Where `t` falls across the window: 0 at the start, 1 at the end.
    pub fn fraction_of(&self, t: f64) -> f64 {
        let range = self.range();
        if range > 0.0 { (t - self.start) / range } else { 0.0 }
    }

    /// The time at `fraction` across the window.
    pub fn time_at(&self, fraction: f64) -> f64 {
        self.start + fraction * self.range()
    }

    /// Keeps the playhead in view when it moves. Moving just past an edge (as during playback)
    /// slides the window along with it; a jump elsewhere centres the window on it. A playhead that
    /// has not moved leaves the window alone, so the user can pan away from it while paused.
    pub fn observe_playhead(&mut self, playhead: f64, follow: bool) {
        let moved = self.last_playhead != Some(playhead);
        self.last_playhead = Some(playhead);
        if !follow || !moved || !playhead.is_finite() || (playhead >= self.start && playhead <= self.end) {
            return;
        }
        let range = self.range();
        let limit = range * Self::SLIDE_LIMIT;
        if playhead > self.end && playhead - self.end <= limit {
            self.pan(playhead - self.end);
        } else if playhead < self.start && self.start - playhead <= limit {
            self.pan(playhead - self.start);
        } else {
            self.place(playhead - range * 0.5, range);
        }
    }

    fn min_range(&self) -> f64 {
        Self::MIN_RANGE.min(self.total)
    }

    /// Puts the window at `start` with width `range`, shifted back inside the capture if needed.
    fn place(&mut self, start: f64, range: f64) {
        let range = range.clamp(0.0, self.total);
        let start = start.clamp(0.0, self.total - range);
        self.start = start;
        self.end = start + range;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn starts_showing_everything() {
        let w = TimeWindow::new(50.0);
        assert_eq!((w.start(), w.end()), (0.0, 50.0));
        assert!(w.is_full());
        assert_eq!(TimeWindow::new(f64::NAN).total(), 0.0);
        assert_eq!(TimeWindow::new(-3.0).total(), 0.0);
    }

    #[test]
    fn zoom_keeps_the_anchor_in_place() {
        let mut w = TimeWindow::new(100.0);
        w.zoom(0.5, 25.0);
        assert!(close(w.range(), 50.0));
        assert!(close(w.fraction_of(25.0), 0.25), "25 s stays a quarter of the way across");
        w.zoom(0.1, 30.0);
        assert!(close(w.range(), 5.0));
        assert!(close(w.time_at(w.fraction_of(30.0)), 30.0));
        assert!(w.start() <= 30.0 && w.end() >= 30.0);
    }

    #[test]
    fn zoom_is_limited_both_ways() {
        let mut w = TimeWindow::new(100.0);
        w.zoom(1.0e-9, 40.0);
        assert!(close(w.range(), TimeWindow::MIN_RANGE));
        w.zoom(1.0e9, 40.0);
        assert!(w.is_full());
        for bad in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            let before = w.clone();
            w.zoom(bad, 10.0);
            assert_eq!(w, before);
        }
    }

    #[test]
    fn zooming_out_near_an_edge_stays_inside() {
        let mut w = TimeWindow::new(100.0);
        w.set(90.0, 100.0);
        w.zoom(3.0, 99.0);
        assert!(close(w.end(), 100.0) && close(w.range(), 30.0));
    }

    #[test]
    fn pan_stops_at_the_ends() {
        let mut w = TimeWindow::new(100.0);
        w.set(10.0, 20.0);
        w.pan(5.0);
        assert_eq!((w.start(), w.end()), (15.0, 25.0));
        w.pan(-1000.0);
        assert_eq!((w.start(), w.end()), (0.0, 10.0));
        w.pan(1000.0);
        assert_eq!((w.start(), w.end()), (90.0, 100.0));
        w.pan(f64::NAN);
        assert_eq!((w.start(), w.end()), (90.0, 100.0));
    }

    #[test]
    fn set_orders_clamps_and_widens() {
        let mut w = TimeWindow::new(100.0);
        w.set(30.0, 20.0);
        assert_eq!((w.start(), w.end()), (20.0, 30.0));
        w.set(-50.0, 500.0);
        assert!(w.is_full());
        w.set(42.0, 42.0);
        assert!(close(w.range(), TimeWindow::MIN_RANGE));
    }

    #[test]
    fn an_empty_capture_is_safe() {
        let mut w = TimeWindow::new(0.0);
        w.zoom(0.5, 0.0);
        w.pan(3.0);
        w.set(1.0, 2.0);
        w.observe_playhead(1.0, true);
        assert_eq!((w.start(), w.end()), (0.0, 0.0));
        assert_eq!(w.fraction_of(1.0), 0.0);
    }

    #[test]
    fn the_playhead_is_followed_while_it_moves() {
        let mut w = TimeWindow::new(100.0);
        w.set(10.0, 20.0);
        w.observe_playhead(15.0, true);
        assert_eq!((w.start(), w.end()), (10.0, 20.0), "inside: nothing to do");

        // Playback creeps past the right edge: the window slides with it.
        w.observe_playhead(20.5, true);
        assert!(close(w.end(), 20.5) && close(w.range(), 10.0));
        // Stepping back past the left edge slides the other way.
        w.set(10.0, 20.0);
        w.observe_playhead(9.0, true);
        assert!(close(w.start(), 9.0));

        // A jump far away recentres.
        w.observe_playhead(70.0, true);
        assert!(close(w.start(), 65.0) && close(w.end(), 75.0));
    }

    #[test]
    fn a_still_playhead_lets_the_user_pan_away() {
        let mut w = TimeWindow::new(100.0);
        w.set(10.0, 20.0);
        w.observe_playhead(15.0, true);
        w.pan(50.0);
        w.observe_playhead(15.0, true);
        assert_eq!((w.start(), w.end()), (60.0, 70.0));
    }

    #[test]
    fn following_can_be_switched_off_without_a_jump_when_it_is_switched_back_on() {
        let mut w = TimeWindow::new(100.0);
        w.set(10.0, 20.0);
        w.observe_playhead(50.0, false);
        assert_eq!((w.start(), w.end()), (10.0, 20.0));
        w.observe_playhead(50.0, true);
        assert_eq!((w.start(), w.end()), (10.0, 20.0), "the playhead did not move since");
    }
}
