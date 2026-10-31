use crate::Series;

/// A series reduced to what can be drawn across a time window. `x` is in pixels from the window's
/// left edge; values are still in data units, so the caller picks the vertical scale.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Trace {
    /// The step line, as `(x, value)` points with `x` never decreasing.
    pub line: Vec<(f32, f32)>,
    /// The area between zero and the line, as `(x0, x1, low, high)` spans.
    pub fill: Vec<(f32, f32, f32, f32)>,
}

impl Trace {
    fn point(&mut self, x: f32, v: f32) {
        let n = self.line.len();
        // Extend a horizontal run rather than adding points along it.
        if n >= 2 && self.line[n - 1].1 == v && self.line[n - 2].1 == v {
            self.line[n - 1].0 = x;
        } else if self.line.last() != Some(&(x, v)) {
            self.line.push((x, v));
        }
    }

    fn span(&mut self, x0: f32, x1: f32, min: f32, max: f32) {
        if x1 <= x0 {
            return;
        }
        let (low, high) = (min.min(0.0), max.max(0.0));
        if let Some(last) = self.fill.last_mut()
            && last.1 >= x0
            && last.2 == low
            && last.3 == high
        {
            last.1 = x1;
            return;
        }
        self.fill.push((x0, x1, low, high));
    }
}

/// Reduces `series` to the window `t0..t1` drawn `width` pixels wide.
///
/// Up to one step per two pixels is drawn exactly. Busier series are drawn per pixel column with
/// the column's entry, extremes and exit values, so spikes are never lost however far out the view
/// is zoomed, and the output never exceeds a few points per column.
pub fn trace(series: &Series, t0: f64, t1: f64, width: f32) -> Trace {
    let mut out = Trace::default();
    let Some(&first_time) = series.times.first() else {
        return out;
    };
    let usable = width.is_finite() && width >= 1.0 && t0.is_finite() && t1.is_finite() && t1 > t0;
    if !usable {
        return out;
    }
    // Only where the series has started, the entity exists and the window is.
    let a = t0.max(series.valid.0).max(first_time);
    let b = t1.min(series.valid.1);
    if a.is_nan() || b.is_nan() || a >= b {
        return out;
    }

    let to_px = |t: f64| (((t - t0) / (t1 - t0)) * f64::from(width)) as f32;
    let first = series.times.partition_point(|&t| t <= a) - 1;
    let end = series.times.partition_point(|&t| t < b);
    let runs = (first..end).map(|i| {
        let start = series.times[i].max(a);
        let stop = series.times.get(i + 1).map_or(b, |&t| t.min(b));
        (to_px(start), to_px(stop), series.values[i])
    });

    if (end - first) as f32 <= width * 0.5 {
        for (x0, x1, v) in runs {
            out.point(x0, v);
            out.point(x1, v);
            out.span(x0, x1, v, v);
        }
        return out;
    }

    let (xa, xb) = (to_px(a), to_px(b));
    let clamp = |x: f32| x.clamp(xa, xb);
    let mut column: Option<Column> = None;
    for (x0, x1, v) in runs {
        let c0 = x0.floor() as i64;
        let c1 = ((x1.ceil() as i64) - 1).max(c0);
        match &mut column {
            Some(col) if col.index == c0 => col.add(v),
            _ => {
                if let Some(col) = column.take() {
                    col.emit(&mut out, clamp);
                }
                column = Some(Column::new(c0, v));
            }
        }
        if c1 > c0 {
            // The value runs on past this column, fills every column up to `c1` and starts that one.
            if let Some(col) = column.take() {
                col.emit(&mut out, clamp);
            }
            if c1 - 1 > c0 {
                let (fx0, fx1) = (clamp((c0 + 1) as f32), clamp(c1 as f32));
                out.point(fx0, v);
                out.point(fx1, v);
                out.span(fx0, fx1, v, v);
            }
            column = Some(Column::new(c1, v));
        }
    }
    if let Some(col) = column {
        col.emit(&mut out, clamp);
    }
    out
}

/// Everything a series did within one pixel column.
struct Column {
    index: i64,
    entry: f32,
    min: f32,
    max: f32,
    exit: f32,
}

impl Column {
    fn new(index: i64, v: f32) -> Column {
        Column { index, entry: v, min: v, max: v, exit: v }
    }

    fn add(&mut self, v: f32) {
        self.min = self.min.min(v);
        self.max = self.max.max(v);
        self.exit = v;
    }

    fn emit(&self, out: &mut Trace, clamp: impl Fn(f32) -> f32) {
        let (x0, mid, x1) = (clamp(self.index as f32), clamp(self.index as f32 + 0.5), clamp(self.index as f32 + 1.0));
        out.point(x0, self.entry);
        if self.min < self.entry || self.max > self.entry {
            // Visit both extremes, ending at the one nearer the exit value.
            if (self.exit - self.min).abs() <= (self.exit - self.max).abs() {
                out.point(mid, self.max);
                out.point(mid, self.min);
            } else {
                out.point(mid, self.min);
                out.point(mid, self.max);
            }
        }
        out.point(x1, self.exit);
        out.span(x0, x1, self.min, self.max);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn series(samples: &[(f64, f32)], valid: (f64, f64)) -> Series {
        Series::new(1, "s", samples.iter().copied(), valid)
    }

    #[test]
    fn a_few_steps_are_drawn_exactly() {
        let s = series(&[(0.0, 1.0), (5.0, 3.0)], (0.0, 10.0));
        let t = trace(&s, 0.0, 10.0, 100.0);
        assert_eq!(t.line, [(0.0, 1.0), (50.0, 1.0), (50.0, 3.0), (100.0, 3.0)]);
        assert_eq!(t.fill, [(0.0, 50.0, 0.0, 1.0), (50.0, 100.0, 0.0, 3.0)]);
    }

    #[test]
    fn negative_values_fill_down_from_zero() {
        let s = series(&[(0.0, -2.0)], (0.0, 10.0));
        let t = trace(&s, 0.0, 10.0, 10.0);
        assert_eq!(t.fill, [(0.0, 10.0, -2.0, 0.0)]);
    }

    #[test]
    fn nothing_is_drawn_before_the_first_value_or_outside_the_lifetime() {
        let s = series(&[(4.0, 1.0)], (2.0, 6.0));
        let t = trace(&s, 0.0, 10.0, 100.0);
        assert_eq!(t.line.first().unwrap().0, 40.0);
        assert_eq!(t.line.last().unwrap().0, 60.0);
        assert!(trace(&s, 7.0, 9.0, 100.0).line.is_empty());
        assert!(trace(&s, 0.0, 3.0, 100.0).line.is_empty());
    }

    #[test]
    fn the_value_in_effect_at_the_window_start_is_carried_in() {
        let s = series(&[(0.0, 7.0), (8.0, 9.0)], (0.0, 10.0));
        let t = trace(&s, 5.0, 10.0, 50.0);
        assert_eq!(t.line, [(0.0, 7.0), (30.0, 7.0), (30.0, 9.0), (50.0, 9.0)]);
    }

    #[test]
    fn degenerate_inputs_draw_nothing() {
        let s = series(&[(0.0, 1.0)], (0.0, 10.0));
        assert!(trace(&s, 0.0, 10.0, 0.0).line.is_empty());
        assert!(trace(&s, 5.0, 5.0, 100.0).line.is_empty());
        assert!(trace(&s, f64::NAN, 5.0, 100.0).line.is_empty());
        assert!(trace(&series(&[], (0.0, 10.0)), 0.0, 10.0, 100.0).line.is_empty());
    }

    #[test]
    fn busy_series_are_bounded_per_column_and_keep_every_extreme() {
        // 10 000 samples of a noisy signal with one huge spike, drawn 200 pixels wide.
        let samples: Vec<(f64, f32)> = (0..10_000)
            .map(|i| {
                let v = if i == 6_543 { 1000.0 } else { ((i * 37) % 11) as f32 };
                (f64::from(i) * 0.01, v)
            })
            .collect();
        let s = series(&samples, (0.0, 100.0));
        let t = trace(&s, 0.0, 100.0, 200.0);
        assert!(t.line.len() <= 4 * 201, "{} points", t.line.len());
        assert!(t.line.windows(2).all(|w| w[0].0 <= w[1].0), "x never goes back");
        assert!(t.line.iter().all(|p| (0.0..=200.0).contains(&p.0)));
        assert!(t.line.iter().any(|p| p.1 == 1000.0), "the spike survives decimation");
        let max_fill = t.fill.iter().map(|f| f.3).fold(0.0, f32::max);
        assert_eq!(max_fill, 1000.0);
        // Every drawn value is a real value.
        assert!(t.line.iter().all(|p| p.1 == 1000.0 || (0.0..=10.0).contains(&p.1)));
    }

    #[test]
    fn decimated_flat_stretches_collapse_to_two_points() {
        // Busy in the first tenth, then constant.
        let mut samples: Vec<(f64, f32)> = (0..1000).map(|i| (f64::from(i) * 0.001, (i % 2) as f32)).collect();
        samples.push((1.0, 5.0));
        let s = series(&samples, (0.0, 10.0));
        let t = trace(&s, 0.0, 10.0, 100.0);
        let flat: Vec<_> = t.line.iter().filter(|p| p.1 == 5.0).collect();
        assert!(flat.len() <= 3, "{flat:?}");
        assert_eq!(t.line.last().unwrap(), &(100.0, 5.0));
    }

    #[test]
    fn exact_and_decimated_agree_on_the_envelope() {
        let samples: Vec<(f64, f32)> = (0..300).map(|i| (f64::from(i), ((i * 7) % 13) as f32)).collect();
        let s = series(&samples, (0.0, 300.0));
        let exact = trace(&s, 0.0, 300.0, 1000.0);
        let coarse = trace(&s, 0.0, 300.0, 50.0);
        let range = |t: &Trace| t.line.iter().fold((f32::MAX, f32::MIN), |(a, b), p| (a.min(p.1), b.max(p.1)));
        assert_eq!(range(&exact), range(&coarse));
    }
}
