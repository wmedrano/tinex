//! Finite circular pole approximation of flux at a tine tip.
//! Geometry is normalized by the gap; electrical gain is set separately.
const TABLE_SIZE: usize = 2049;
const MAX_RADIUS_SQUARED: f64 = 64.0;
const POLE_RADIUS: f64 = 0.35;
const ALIGNMENT: f64 = 0.25;

#[derive(Clone)]
pub(super) struct Pickup {
    coefficients: Box<[[f64; 4]]>,
    end_flux: f64,
    tail_exponent: f64,
}

impl Pickup {
    pub fn new() -> Self {
        let flux: Vec<_> = (0..TABLE_SIZE)
            .map(|i| {
                reference_flux((i as f64 * MAX_RADIUS_SQUARED / (TABLE_SIZE - 1) as f64).sqrt())
            })
            .collect();
        let step = MAX_RADIUS_SQUARED / (TABLE_SIZE - 1) as f64;
        let end = flux[TABLE_SIZE - 1];
        let slope = (end - flux[TABLE_SIZE - 2]) / step;
        // Store the Hermite polynomials once; the audio callback uses Horner's rule.
        let coefficients = (0..TABLE_SIZE - 1)
            .map(|i| {
                let (a, b) = (flux[i], flux[i + 1]);
                let da = if i == 0 {
                    b - a
                } else {
                    0.5 * (b - flux[i - 1])
                };
                let db = if i + 2 == TABLE_SIZE {
                    b - a
                } else {
                    0.5 * (flux[i + 2] - a)
                };
                [
                    a,
                    da,
                    3.0 * (b - a) - 2.0 * da - db,
                    2.0 * (a - b) + da + db,
                ]
            })
            .collect();
        Self {
            coefficients,
            end_flux: end,
            tail_exponent: -(1.0 + MAX_RADIUS_SQUARED) * slope / end,
        }
    }

    pub fn flux(&self, x: f64, y: f64, inverse_gap: f64) -> f64 {
        let x = x * inverse_gap + ALIGNMENT;
        let y = y * inverse_gap;
        self.radial_flux(x * x + y * y)
    }

    fn radial_flux(&self, radius_squared: f64) -> f64 {
        if radius_squared >= MAX_RADIUS_SQUARED {
            return self.end_flux
                * ((1.0 + MAX_RADIUS_SQUARED) / (1.0 + radius_squared)).powf(self.tail_exponent);
        }
        let position = radius_squared.max(0.0) * (TABLE_SIZE - 1) as f64 / MAX_RADIUS_SQUARED;
        let index = position as usize;
        let t = position - index as f64;
        let [a, b, c, d] = self.coefficients[index];
        a + t * (b + t * (c + t * d))
    }
}

// Area average of a gap-normalized magnetic point-pole field over a finite pole.
// This is an approximation to the spatial flux weighting, not an exact Maxwell solve.
fn reference_flux(radius: f64) -> f64 {
    let mut sum = 0.0;
    for ring in 0..12 {
        let r = POLE_RADIUS * ((ring as f64 + 0.5) / 12.0).sqrt();
        for angle in 0..32 {
            let theta = std::f64::consts::TAU * (angle as f64 + 0.5) / 32.0;
            let x = radius - r * theta.cos();
            let y = r * theta.sin();
            let d = 1.0 + x * x + y * y;
            sum += 1.0 / (d * d.sqrt());
        }
    }
    sum / (12.0 * 32.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_matches_reference_and_is_continuous() {
        let pickup = Pickup::new();
        for i in 0..1000 {
            let r2 = 64.0 * (i as f64 + 0.37) / 1000.0;
            let expected = reference_flux(r2.sqrt());
            assert!((pickup.radial_flux(r2) - expected).abs() < 1e-4);
        }
        for boundary in [0.03125, 1.0, 10.0, 64.0] {
            let eps = 1e-6;
            let left = (pickup.radial_flux(boundary) - pickup.radial_flux(boundary - eps)) / eps;
            let right = (pickup.radial_flux(boundary + eps) - pickup.radial_flux(boundary)) / eps;
            assert!((left - right).abs() < 2e-5);
        }
        assert!(pickup.flux(0.2, 0.1, 125.0).is_finite());
    }
}
