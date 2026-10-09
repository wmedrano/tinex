use vello::Scene;
use vello::kurbo::{Affine, Rect};
use vello::peniko::Fill;

use super::theme::Theme;

const MIN_DB: f64 = -60.0;
const MAX_DB: f64 = 6.0;
const ZERO_DB_FRACTION: f64 = -MIN_DB / (MAX_DB - MIN_DB);

/// A stateless level display. Callers own measurements, layout, and update timing.
pub struct LevelMeter(pub f32);

impl LevelMeter {
    pub fn draw(&self, scene: &mut Scene, theme: &Theme, bounds: Rect, scale: f64) {
        if bounds.width() <= 0.0 || bounds.height() <= 0.0 {
            return;
        }
        scene.fill(
            Fill::NonZero,
            Affine::scale(scale),
            theme.button_background,
            None,
            &bounds,
        );
        let [normal, over] = level_meter_segments(bounds, self.0);
        for (fill, color) in [(normal, theme.meter_normal), (over, theme.meter_over)] {
            if fill.width() > 0.0 {
                scene.fill(Fill::NonZero, Affine::scale(scale), color, None, &fill);
            }
        }
        let zero_db_x = bounds.x0 + bounds.width() * ZERO_DB_FRACTION;
        let tick = Rect::new(
            (zero_db_x - 0.5).max(bounds.x0),
            bounds.y0,
            (zero_db_x + 0.5).min(bounds.x1),
            bounds.y1,
        );
        scene.fill(
            Fill::NonZero,
            Affine::scale(scale),
            theme.meter_reference,
            None,
            &tick,
        );
    }
}

fn level_meter_fill(meter: Rect, level: f32) -> Rect {
    // Callers supply linear amplitude; only the display uses decibels.
    let fraction = if level.is_nan() || level <= 0.0 {
        0.0
    } else {
        let db = 20.0 * f64::from(level).log10();
        ((db - MIN_DB) / (MAX_DB - MIN_DB)).clamp(0.0, 1.0)
    };
    Rect::new(
        meter.x0,
        meter.y0,
        meter.x0 + meter.width() * fraction,
        meter.y1,
    )
}

fn level_meter_segments(meter: Rect, level: f32) -> [Rect; 2] {
    let fill = level_meter_fill(meter, level);
    let split = fill.x1.min(meter.x0 + meter.width() * ZERO_DB_FRACTION);
    [
        Rect::new(meter.x0, meter.y0, split, meter.y1),
        Rect::new(split, meter.y0, fill.x1, meter.y1),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_clamps_levels_and_handles_nan() {
        let bounds = Rect::new(10.0, 20.0, 110.0, 28.0);
        for (level, fraction) in [
            (-1.0, 0.0),
            (0.0, 0.0),
            (0.0001, 0.0),
            (0.001, 0.0),
            (10.0_f32.powf(-30.0 / 20.0), 30.0 / 66.0),
            (1.0, ZERO_DB_FRACTION),
            (10.0_f32.powf(6.0 / 20.0), 1.0),
            (2.0, 1.0),
            (f32::NAN, 0.0),
            (f32::INFINITY, 1.0),
            (f32::NEG_INFINITY, 0.0),
        ] {
            let fill = level_meter_fill(bounds, level);
            assert!((fill.width() - bounds.width() * fraction).abs() < 1e-5);
            assert_eq!(fill.x0, bounds.x0);
            assert_eq!(fill.y0, bounds.y0);
            assert_eq!(fill.y1, bounds.y1);
        }
    }

    #[test]
    fn over_range_starts_at_zero_db_only_above_full_scale() {
        let bounds = Rect::new(10.0, 20.0, 110.0, 28.0);
        let reference = bounds.x0 + bounds.width() * ZERO_DB_FRACTION;
        for level in [0.0, 0.5, 1.0] {
            let [normal, over] = level_meter_segments(bounds, level);
            assert!(normal.x1 <= reference);
            assert_eq!(over.width(), 0.0);
        }
        let [normal, over] = level_meter_segments(bounds, 1.5);
        assert_eq!(normal.x1, reference);
        assert_eq!(over.x0, reference);
        assert!(over.width() > 0.0);
        assert!(over.x1 < bounds.x1);
        let [normal, over] = level_meter_segments(bounds, 1.0);
        assert_eq!(normal.x1, reference);
        assert_eq!(over.x0, reference);
    }
}
