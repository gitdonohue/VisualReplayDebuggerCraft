//! Playback control for a [`Replay`]: current time and frame, play/pause, speed, looping, seeking
//! and frame stepping. It has no UI code, so the desktop app, the web app and the MCP server can
//! share it.

use vrd_model::Replay;

pub const MIN_SPEED: f64 = 0.01;
pub const MAX_SPEED: f64 = 100.0;

/// Where playback is in a replay and how it advances.
///
/// The current frame is stored rather than derived from the time on demand: a capture can contain
/// consecutive frames with equal timestamps, and stepping must still be able to land on each.
#[derive(Debug, Clone, PartialEq)]
pub struct Playback {
    time: f64,
    frame: i32,
    playing: bool,
    speed: f64,
    looping: bool,
}

impl Default for Playback {
    fn default() -> Self {
        Self { time: 0.0, frame: 0, playing: false, speed: 1.0, looping: true }
    }
}

impl Playback {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn time(&self) -> f64 {
        self.time
    }

    pub fn frame(&self) -> i32 {
        self.frame
    }

    pub fn is_playing(&self) -> bool {
        self.playing
    }

    pub fn speed(&self) -> f64 {
        self.speed
    }

    pub fn is_looping(&self) -> bool {
        self.looping
    }

    pub fn set_looping(&mut self, looping: bool) {
        self.looping = looping;
    }

    /// Sets the playback rate, clamped to [`MIN_SPEED`]..=[`MAX_SPEED`]. Non-finite values are ignored.
    pub fn set_speed(&mut self, speed: f64) {
        if speed.is_finite() {
            self.speed = speed.clamp(MIN_SPEED, MAX_SPEED);
        }
    }

    pub fn play(&mut self, replay: &Replay) {
        // Pressing play at the end starts over rather than stopping again immediately.
        if self.time >= replay.total_time() && replay.total_time() > 0.0 {
            self.seek_time(replay, 0.0);
        }
        self.playing = true;
    }

    pub fn pause(&mut self) {
        self.playing = false;
    }

    pub fn toggle(&mut self, replay: &Replay) {
        if self.playing {
            self.pause();
        } else {
            self.play(replay);
        }
    }

    /// Goes back to the start and stops.
    pub fn rewind(&mut self, replay: &Replay) {
        self.playing = false;
        self.seek_time(replay, 0.0);
    }

    /// Moves to time `t` (clamped to the capture). Returns the frame that is now current.
    pub fn seek_time(&mut self, replay: &Replay, t: f64) -> i32 {
        let t = if t.is_nan() { 0.0 } else { t.clamp(0.0, replay.total_time()) };
        self.time = t;
        self.frame = replay.frame_for_time(t);
        self.frame
    }

    /// Moves to the start of `frame` (clamped to the capture). Returns the frame now current.
    pub fn seek_frame(&mut self, replay: &Replay, frame: i32) -> i32 {
        self.frame = frame.clamp(0, replay.last_frame());
        self.time = replay.time_for_frame(self.frame);
        self.frame
    }

    /// Steps by `delta` frames and pauses, as a debugger step does.
    pub fn step(&mut self, replay: &Replay, delta: i32) -> i32 {
        self.playing = false;
        self.seek_frame(replay, self.frame.saturating_add(delta))
    }

    /// Moves to the start of the next frame that carries a log entry, if any.
    pub fn step_to_next_log(&mut self, replay: &Replay) -> Option<i32> {
        let logs = replay.logs();
        let next = logs.partition_point(|l| l.frame <= self.frame);
        let frame = logs.get(next)?.frame;
        self.playing = false;
        Some(self.seek_frame(replay, frame))
    }

    /// Moves to the start of the closest earlier frame that carries a log entry, if any.
    pub fn step_to_previous_log(&mut self, replay: &Replay) -> Option<i32> {
        let logs = replay.logs();
        let before = logs.partition_point(|l| l.frame < self.frame);
        let frame = logs.get(before.checked_sub(1)?)?.frame;
        self.playing = false;
        Some(self.seek_frame(replay, frame))
    }

    /// Advances by `dt` seconds of wall-clock time if playing. Returns whether the position moved.
    pub fn advance(&mut self, replay: &Replay, dt: f64) -> bool {
        if !self.playing || !dt.is_finite() || dt <= 0.0 {
            return false;
        }
        let total = replay.total_time();
        let before = (self.time, self.frame);

        let mut t = self.time + dt * self.speed;
        if t >= total {
            if self.looping && total > 0.0 {
                t %= total;
            } else {
                t = total;
                self.playing = false;
            }
        }
        self.seek_time(replay, t);
        before != (self.time, self.frame)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vrd_format::{Color, EntityOp, Record};
    use vrd_model::LoadInfo;

    fn replay(times: &[f32], log_frames: &[i32]) -> Replay {
        let mut records: Vec<Record> = times.iter().map(|&t| Record::FrameStep { total_time: t }).collect();
        records.extend(log_frames.iter().map(|&frame| Record::Entity {
            frame,
            id: 1,
            op: EntityOp::Log { category: "c".into(), message: "m".into(), color: Color::WHITE },
        }));
        Replay::from_records(records, LoadInfo::default())
    }

    #[test]
    fn seeking_clamps_and_tracks_frames() {
        let r = replay(&[1.0, 2.0, 3.0], &[]);
        let mut p = Playback::new();
        assert_eq!(p.seek_time(&r, 1.5), 1);
        assert_eq!(p.time(), 1.5);
        assert_eq!(p.seek_time(&r, 99.0), 3);
        assert_eq!(p.time(), 3.0);
        assert_eq!(p.seek_time(&r, -5.0), 0);
        assert_eq!(p.seek_time(&r, f64::NAN), 0);
        assert_eq!(p.seek_frame(&r, 2), 2);
        assert_eq!(p.time(), 2.0);
        assert_eq!(p.seek_frame(&r, 100), 3);
        assert_eq!(p.seek_frame(&r, -100), 0);
    }

    #[test]
    fn stepping_pauses_and_clamps() {
        let r = replay(&[1.0, 2.0], &[]);
        let mut p = Playback::new();
        p.play(&r);
        assert_eq!(p.step(&r, 1), 1);
        assert!(!p.is_playing());
        assert_eq!(p.step(&r, 5), 2);
        assert_eq!(p.step(&r, -1), 1);
        assert_eq!(p.step(&r, i32::MIN), 0);
        assert_eq!(p.step(&r, i32::MAX), 2);
    }

    #[test]
    fn stepping_visits_every_frame_even_with_equal_timestamps() {
        let r = replay(&[1.0, 1.0, 1.0, 2.0], &[]);
        let mut p = Playback::new();
        let mut visited = vec![p.frame()];
        for _ in 0..4 {
            visited.push(p.step(&r, 1));
        }
        assert_eq!(visited, [0, 1, 2, 3, 4]);
        let mut back = vec![];
        for _ in 0..4 {
            back.push(p.step(&r, -1));
        }
        assert_eq!(back, [3, 2, 1, 0]);
    }

    #[test]
    fn playback_advances_with_speed() {
        let r = replay(&[1.0, 2.0, 3.0, 4.0], &[]);
        let mut p = Playback::new();
        assert!(!p.advance(&r, 0.5), "paused playback does not move");
        p.play(&r);
        p.set_speed(2.0);
        assert!(p.advance(&r, 0.75));
        assert_eq!(p.time(), 1.5);
        assert_eq!(p.frame(), 1);
        assert!(!p.advance(&r, 0.0));
        assert!(!p.advance(&r, f64::NAN));
        assert!(!p.advance(&r, -1.0));
    }

    #[test]
    fn reaching_the_end_loops_or_stops() {
        let r = replay(&[1.0, 2.0], &[]);
        let mut p = Playback::new();
        p.play(&r);
        p.advance(&r, 2.5);
        assert!(p.is_playing());
        assert_eq!(p.time(), 0.5);

        p.set_looping(false);
        p.advance(&r, 10.0);
        assert!(!p.is_playing());
        assert_eq!(p.time(), 2.0);
        assert_eq!(p.frame(), 2);

        p.play(&r);
        assert!(p.is_playing());
        assert_eq!(p.time(), 0.0, "play at the end restarts");
    }

    #[test]
    fn speed_is_clamped_and_rejects_garbage() {
        let mut p = Playback::new();
        p.set_speed(1e9);
        assert_eq!(p.speed(), MAX_SPEED);
        p.set_speed(0.0);
        assert_eq!(p.speed(), MIN_SPEED);
        p.set_speed(f64::NAN);
        assert_eq!(p.speed(), MIN_SPEED);
    }

    #[test]
    fn empty_replay_is_safe() {
        let r = replay(&[], &[]);
        let mut p = Playback::new();
        p.play(&r);
        p.advance(&r, 1.0);
        assert_eq!(p.frame(), 0);
        assert_eq!(p.step(&r, 1), 0);
        assert_eq!(p.seek_time(&r, 5.0), 0);
    }

    #[test]
    fn log_navigation_jumps_between_frames_with_logs() {
        let r = replay(&[1.0, 2.0, 3.0, 4.0, 5.0], &[1, 1, 3]);
        let mut p = Playback::new();
        assert_eq!(p.step_to_next_log(&r), Some(1));
        assert_eq!(p.step_to_next_log(&r), Some(3));
        assert_eq!(p.step_to_next_log(&r), None);
        assert_eq!(p.frame(), 3);
        assert_eq!(p.step_to_previous_log(&r), Some(1));
        assert_eq!(p.step_to_previous_log(&r), None);
        assert_eq!(p.frame(), 1);
    }
}
