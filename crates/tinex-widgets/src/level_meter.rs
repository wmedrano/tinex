//! Weighted-peak dBFS display: unity is 0 dBFS, not the end of the scale.
use xilem::Color;
use xilem::masonry::kurbo::{Affine, Line, Rect, Stroke};
use xilem::masonry::peniko::Fill;
use xilem::masonry::vello::Scene;

pub const METER_MIN_DB: f64 = -60.0;
pub const METER_MAX_DB: f64 = 12.0;

/// Keep positive overloads (including infinity), but never propagate NaN,
/// negative amplitudes, or negative zero into presentation state.
pub fn sanitize_level(amplitude: f32) -> f32 {
    if amplitude > 0.0 { amplitude } else { 0.0 }
}

pub fn amplitude_to_dbfs(amplitude: f32) -> f64 {
    let amplitude = sanitize_level(amplitude);
    if amplitude == 0.0 {
        f64::NEG_INFINITY
    } else {
        20.0 * f64::from(amplitude).log10()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MeterStyle {
    pub trough: Color,
    pub normal: Color,
    pub warning: Color,
    pub near_ceiling: Color,
    pub over: Color,
    pub reference: Color,
}

/// A noninteractive bar-only horizontal level meter with a 0 dBFS tick.
/// Only geometry is clamped; positive dBFS levels retain their value.
#[derive(Clone, Copy)]
pub struct LevelMeter {
    pub amplitude: f32,
}

impl LevelMeter {
    pub const HEIGHT: f64 = 24.0;

    pub fn new(amplitude: f32) -> Self {
        Self {
            amplitude: sanitize_level(amplitude),
        }
    }

    pub fn dbfs(self) -> f64 {
        amplitude_to_dbfs(self.amplitude)
    }

    pub fn render(self, scene: &mut Scene, rect: Rect, scale: f64, style: &MeterStyle) {
        self.render_with_range(scene, rect, scale, style, METER_MIN_DB, METER_MAX_DB);
    }

    pub(crate) fn render_with_range(
        self,
        scene: &mut Scene,
        rect: Rect,
        scale: f64,
        style: &MeterStyle,
        min_db: f64,
        max_db: f64,
    ) {
        if !scale.is_finite() || scale <= 0.0 {
            return;
        }
        let Some(geometry) = self.geometry_with_range(rect, min_db, max_db) else {
            return;
        };
        let transform = Affine::scale(scale);
        scene.push_clip_layer(transform, &rect);
        scene.fill(Fill::NonZero, transform, style.trough, None, &geometry.bar);
        let colors = [style.normal, style.warning, style.near_ceiling, style.over];
        for (segment, color) in geometry.segments.into_iter().zip(colors) {
            if let Some(segment) = segment {
                scene.fill(Fill::NonZero, transform, color, None, &segment);
            }
        }
        if let Some(overflow) = geometry.overflow {
            scene.fill(Fill::NonZero, transform, style.over, None, &overflow);
        }
        // Reference remains visible even at silence and above the displayed range.
        scene.stroke(
            &Stroke::new(1.0),
            transform,
            style.reference,
            None,
            &Line::new(
                (geometry.reference_x, geometry.bar.y0 - 2.0),
                (geometry.reference_x, geometry.bar.y1 + 2.0),
            ),
        );
        scene.pop_layer();
    }

    #[cfg(test)]
    fn geometry(self, rect: Rect) -> Option<Geometry> {
        self.geometry_with_range(rect, METER_MIN_DB, METER_MAX_DB)
    }

    fn geometry_with_range(self, rect: Rect, min_db: f64, max_db: f64) -> Option<Geometry> {
        if !min_db.is_finite() || !max_db.is_finite() || min_db >= max_db {
            return None;
        }
        let position = |db: f64| ((db - min_db) / (max_db - min_db)).clamp(0.0, 1.0);
        if ![
            rect.x0,
            rect.y0,
            rect.x1,
            rect.y1,
            rect.width(),
            rect.height(),
        ]
        .iter()
        .all(|value| value.is_finite())
            || rect.width() <= 0.0
            || rect.height() <= 0.0
        {
            return None;
        }
        let bar = Rect::new(
            rect.x0,
            rect.y0 + rect.height() * 0.25,
            rect.x1,
            rect.y0 + rect.height() * 0.75,
        );
        let db = self.dbfs();
        let fill = position(db);
        let boundaries = [min_db, -12.0, -6.0, 0.0, max_db];
        let segments = std::array::from_fn(|index| {
            let start = position(boundaries[index]);
            let end = position(boundaries[index + 1]).min(fill);
            (end > start).then(|| {
                Rect::new(
                    bar.x0 + start * bar.width(),
                    bar.y0,
                    bar.x0 + end * bar.width(),
                    bar.y1,
                )
            })
        });
        let overflow = (db > max_db).then(|| {
            Rect::new(
                (bar.x1 - 4.0).max(bar.x0),
                bar.y0 - 2.0,
                bar.x1,
                bar.y1 + 2.0,
            )
        });
        Some(Geometry {
            bar,
            segments,
            reference_x: bar.x0 + position(0.0) * bar.width(),
            overflow,
        })
    }
}

struct Geometry {
    bar: Rect,
    segments: [Option<Rect>; 4],
    reference_x: f64,
    overflow: Option<Rect>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decibels_use_amplitude_not_power_and_preserve_headroom() {
        for db in [-60.0_f64, -12.0, -6.0, 0.0, 6.0, 12.0, 24.0] {
            let amplitude = 10.0_f64.powf(db / 20.0) as f32;
            assert!((amplitude_to_dbfs(amplitude) - db).abs() < 0.00001);
        }
        assert_eq!(amplitude_to_dbfs(1.0), 0.0);
        assert!((amplitude_to_dbfs(2.0) - 6.0206).abs() < 0.0001);
    }

    #[test]
    fn silence_invalid_inputs_and_infinity_have_explicit_semantics() {
        for amplitude in [0.0, -0.0, -2.0, f32::NEG_INFINITY, f32::NAN] {
            assert_eq!(sanitize_level(amplitude).to_bits(), 0.0_f32.to_bits());
            let meter = LevelMeter::new(amplitude);
            assert_eq!(meter.dbfs(), f64::NEG_INFINITY);
            let geometry = meter.geometry(Rect::new(0.0, 0.0, 144.0, 40.0)).unwrap();
            assert!(geometry.segments.iter().all(Option::is_none));
            assert!((geometry.reference_x - 120.0).abs() < 0.00001);
        }
        let meter = LevelMeter::new(f32::INFINITY);
        assert_eq!(meter.dbfs(), f64::INFINITY);
        assert!(
            meter
                .geometry(Rect::new(0.0, 0.0, 144.0, 40.0))
                .unwrap()
                .overflow
                .is_some()
        );
    }

    #[test]
    fn bar_uses_full_width_and_is_centered_at_standard_and_compact_heights() {
        for (width, height) in [(144.0, LevelMeter::HEIGHT), (176.0, 28.0)] {
            let rect = Rect::new(8.0, 12.0, 8.0 + width, 12.0 + height);
            let geometry = LevelMeter::new(1.0).geometry(rect).unwrap();
            assert_eq!(geometry.bar.x0, rect.x0);
            assert_eq!(geometry.bar.x1, rect.x1);
            assert_eq!(geometry.bar.center(), rect.center());
            assert_eq!(geometry.bar.height(), height * 0.5);
            assert!(geometry.reference_x > rect.x0 && geometry.reference_x < rect.x1);
        }
    }

    #[test]
    fn levels_below_display_floor_leave_the_bar_empty() {
        for amplitude in [0.0, 0.000001, 0.0001, 0.000999] {
            let geometry = LevelMeter::new(amplitude)
                .geometry(Rect::new(0.0, 0.0, 144.0, LevelMeter::HEIGHT))
                .unwrap();
            assert!(geometry.segments.iter().all(Option::is_none));
            assert!(geometry.overflow.is_none());
        }
    }

    #[test]
    fn fixed_zones_fill_progressively_and_positive_values_are_distinct() {
        let rect = Rect::new(0.0, 0.0, 144.0, 40.0);
        let unity = LevelMeter::new(1.0).geometry(rect).unwrap();
        assert_eq!(
            unity
                .segments
                .iter()
                .filter(|segment| segment.is_some())
                .count(),
            3
        );
        assert_eq!(unity.segments[0].unwrap().x1, 96.0); // -12 dBFS.
        assert_eq!(unity.segments[1].unwrap().x1, 108.0); // -6 dBFS.
        assert_eq!(unity.segments[2].unwrap().x1, 120.0); // 0 dBFS.
        let hot = LevelMeter::new(2.0).geometry(rect).unwrap();
        assert!(hot.segments[3].unwrap().x1 > unity.reference_x);
        assert!(hot.segments[3].unwrap().x1 < 144.0);
        assert!(hot.overflow.is_none());
        let hotter = LevelMeter::new(3.0).geometry(rect).unwrap();
        assert!(hotter.segments[3].unwrap().x1 > hot.segments[3].unwrap().x1);
        let overflow = LevelMeter::new(16.0).geometry(rect).unwrap();
        assert_eq!(overflow.segments[3].unwrap().x1, 144.0);
        assert!(overflow.overflow.is_some());
    }

    #[test]
    fn geometry_is_finite_for_narrow_rectangles_and_rejects_degenerate_bounds() {
        for amplitude in [0.0, 1.0, 2.0, f32::INFINITY, f32::NAN] {
            for width in [0.001, 1.0, 144.0] {
                let geometry = LevelMeter::new(amplitude)
                    .geometry(Rect::new(4.0, 8.0, 4.0 + width, 8.001))
                    .unwrap();
                for rect in std::iter::once(geometry.bar)
                    .chain(geometry.segments.into_iter().flatten())
                    .chain(geometry.overflow)
                {
                    assert!(rect.width() >= 0.0 && rect.height() >= 0.0);
                    assert!(
                        [rect.x0, rect.y0, rect.x1, rect.y1]
                            .iter()
                            .all(|value| value.is_finite())
                    );
                }
            }
            for rect in [
                Rect::ZERO,
                Rect::new(0.0, 0.0, -1.0, 40.0),
                Rect::new(0.0, 0.0, f64::NAN, 40.0),
                Rect::new(-f64::MAX, 0.0, f64::MAX, 40.0),
            ] {
                assert!(LevelMeter::new(amplitude).geometry(rect).is_none());
            }
        }
    }
    #[test]
    fn combined_meter_scale_aligns_unity_and_preserves_overflow_and_silence() {
        let rect = Rect::new(8.0, 8.0, 208.0, 28.0);
        for db in [-60.0_f32, -6.0, 0.0, 6.0, 12.0] {
            let geometry = LevelMeter::new(10.0_f32.powf(db / 20.0))
                .geometry_with_range(rect, -61.0, 12.0)
                .unwrap();
            let right = geometry.segments.iter().flatten().last().unwrap().x1;
            let thumb = rect.x0 + f64::from((db + 61.0) / 73.0) * rect.width();
            assert!((right - thumb).abs() < 0.0001);
            assert!((geometry.reference_x - (rect.x0 + 61.0 / 73.0 * rect.width())).abs() < 1e-6);
        }
        for amplitude in [0.0, f32::NAN, -1.0] {
            let geometry = LevelMeter::new(amplitude)
                .geometry_with_range(rect, -61.0, 12.0)
                .unwrap();
            assert!(geometry.segments.iter().all(Option::is_none));
            assert!(geometry.overflow.is_none());
        }
        for amplitude in [4.0, 16.0, f32::INFINITY] {
            let geometry = LevelMeter::new(amplitude)
                .geometry_with_range(rect, -61.0, 12.0)
                .unwrap();
            assert!(geometry.overflow.is_some());
            assert_eq!(geometry.segments[3].unwrap().x1, rect.x1);
        }
        for (min, max) in [(f64::NAN, 12.0), (-61.0, f64::INFINITY), (12.0, -61.0)] {
            assert!(
                LevelMeter::new(1.0)
                    .geometry_with_range(rect, min, max)
                    .is_none()
            );
        }
    }
}
