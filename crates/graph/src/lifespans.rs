//! The entity timelines: when each entity existed and when it logged something, and how that
//! maps to the pixels of a time window.

use vrd_model::Replay;

/// One entity's life: from the time it appeared to the time it was gone, and its log entries.
#[derive(Debug, Clone, PartialEq)]
pub struct Lifespan {
    pub entity: i32,
    pub start: f64,
    pub end: f64,
    /// When the entity logged something, ascending.
    pub events: Vec<f64>,
}

/// Every entity that exists at some point in the capture, in the order they appeared.
pub fn lifespans(replay: &Replay) -> Vec<Lifespan> {
    let mut spans: Vec<(i32, Lifespan)> = replay
        .entities()
        .filter_map(|e| {
            let (first, last) = replay.lifetime(e.id)?;
            let start = replay.time_for_frame(first);
            let end = if last >= replay.last_frame() { replay.total_time() } else { replay.time_for_frame(last + 1) };
            let events = e.log_frames.iter().map(|&f| replay.time_for_frame(f)).collect();
            Some((first, Lifespan { entity: e.id, start, end: end.max(start), events }))
        })
        .collect();
    spans.sort_by_key(|(first, span)| (*first, span.entity));
    spans.into_iter().map(|(_, span)| span).collect()
}

/// The part of `start..end` inside the window `t0..t1`, as pixel offsets within a plot `width` wide.
/// `None` when it lies wholly outside.
pub fn span_columns(start: f64, end: f64, t0: f64, t1: f64, width: f32) -> Option<(f32, f32)> {
    let range = t1 - t0;
    if range <= 0.0 || end < t0 || start > t1 {
        return None;
    }
    let to_px = |t: f64| (((t - t0) / range) as f32 * width).clamp(0.0, width);
    Some((to_px(start), to_px(end)))
}

/// Pixel offsets of the events inside the window, one per pixel column however many fall there.
pub fn event_columns(events: &[f64], t0: f64, t1: f64, width: f32) -> Vec<f32> {
    let range = t1 - t0;
    if range <= 0.0 || width <= 0.0 {
        return Vec::new();
    }
    let first = events.partition_point(|&t| t < t0);
    let mut columns: Vec<f32> = Vec::new();
    for &t in &events[first..] {
        if t > t1 {
            break;
        }
        let x = (((t - t0) / range) as f32 * width).floor();
        if columns.last() != Some(&x) {
            columns.push(x);
        }
    }
    columns
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_are_clipped_to_the_window() {
        let (a, b) = span_columns(2.0, 6.0, 0.0, 10.0, 100.0).unwrap();
        assert!((a - 20.0).abs() < 1e-3 && (b - 60.0).abs() < 1e-3);
        assert_eq!(span_columns(-5.0, 20.0, 0.0, 10.0, 100.0), Some((0.0, 100.0)));
        assert_eq!(span_columns(11.0, 12.0, 0.0, 10.0, 100.0), None);
        assert_eq!(span_columns(-3.0, -1.0, 0.0, 10.0, 100.0), None);
        assert_eq!(span_columns(0.0, 1.0, 5.0, 5.0, 100.0), None);
    }

    #[test]
    fn many_events_in_a_pixel_make_one_column() {
        let events: Vec<f64> = (0..1000).map(|i| f64::from(i) * 0.001).collect();
        let columns = event_columns(&events, 0.0, 10.0, 100.0);
        // A thousand events span the first second: ten pixels.
        assert_eq!(columns.len(), 10);
        assert!(columns.windows(2).all(|w| w[1] > w[0]));
    }

    #[test]
    fn only_events_inside_the_window_count() {
        let events = [1.0, 2.0, 3.0, 4.0, 9.0];
        assert_eq!(event_columns(&events, 2.0, 5.0, 30.0), vec![0.0, 10.0, 20.0]);
        assert!(event_columns(&events, 5.0, 8.0, 30.0).is_empty());
        assert!(event_columns(&events, 0.0, 0.0, 30.0).is_empty());
    }
}
