/// Tick positions for a time axis over `t0..t1` drawn `width_px` wide: the step (1, 2 or 5 times a
/// power of ten seconds) is the smallest that keeps ticks `min_spacing_px` apart.
pub fn time_ticks(t0: f64, t1: f64, width_px: f32, min_spacing_px: f32) -> (f64, Vec<f64>) {
    let usable = t0.is_finite() && t1.is_finite() && t1 > t0 && width_px.is_finite() && width_px > 0.0;
    if !usable {
        return (1.0, Vec::new());
    }
    let min_step = (t1 - t0) * f64::from(min_spacing_px.max(1.0)) / f64::from(width_px);
    let magnitude = 10f64.powf(min_step.log10().floor());
    let step =
        [1.0, 2.0, 5.0, 10.0].into_iter().map(|m| m * magnitude).find(|&s| s >= min_step).unwrap_or(10.0 * magnitude);
    let first = (t0 / step).ceil() as i64;
    let last = (t1 / step).floor() as i64;
    let ticks = (first..=last).map(|i| i as f64 * step).collect();
    (step, ticks)
}

/// A tick label: seconds with just enough decimals for `step`.
pub fn format_time(t: f64, step: f64) -> String {
    let decimals = if step >= 1.0 { 0 } else { (-step.log10().floor()) as usize };
    format!("{:.*}s", decimals.min(6), t)
}

/// A compact value label: up to three decimals, trailing zeros dropped.
pub fn format_value(v: f32) -> String {
    if !v.is_finite() {
        return v.to_string();
    }
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".to_string() } else { s.to_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticks_use_round_steps_and_stay_inside() {
        let (step, ticks) = time_ticks(0.0, 53.43, 1000.0, 80.0);
        assert_eq!(step, 5.0);
        assert_eq!(ticks.first(), Some(&0.0));
        assert_eq!(ticks.last(), Some(&50.0));

        let (step, ticks) = time_ticks(12.34, 12.9, 600.0, 80.0);
        assert!((step - 0.1).abs() < 1e-12, "{step}");
        assert!(ticks.iter().all(|&t| (12.34..=12.9).contains(&t)));
        assert!(ticks.windows(2).all(|w| w[1] > w[0]));
    }

    #[test]
    fn ticks_are_never_closer_than_asked() {
        for (t0, t1, w) in [(0.0, 1.0, 1000.0), (0.0, 3600.0, 500.0), (5.0, 5.001, 800.0)] {
            let (step, _) = time_ticks(t0, t1, w, 70.0);
            assert!(step / (t1 - t0) * f64::from(w) >= 70.0 - 1e-6, "{t0}..{t1}: step {step}");
        }
        assert!(time_ticks(1.0, 1.0, 100.0, 50.0).1.is_empty());
        assert!(time_ticks(0.0, 1.0, 0.0, 50.0).1.is_empty());
    }

    #[test]
    fn labels() {
        assert_eq!(format_time(15.0, 5.0), "15s");
        assert_eq!(format_time(12.3, 0.1), "12.3s");
        assert_eq!(format_time(0.25, 0.05), "0.25s");
        assert_eq!(format_value(59512.0), "59512");
        assert_eq!(format_value(15.283264), "15.283");
        assert_eq!(format_value(0.5), "0.5");
        assert_eq!(format_value(-0.0001), "0");
        assert_eq!(format_value(-2.5), "-2.5");
    }
}
