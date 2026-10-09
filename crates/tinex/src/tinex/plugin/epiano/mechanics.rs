//! SI-unit, mass-normalized modal reduction of a tine and its compliant mount.
use nalgebra::{DMatrix, SymmetricEigen};
use std::f64::consts::TAU;

pub(super) const MODES: usize = 14;
pub(super) const DAMPER_STAGES: usize = 32;
const ELEMENTS: usize = 6;
const DOFS: usize = 2 * ELEMENTS + 1;
const HAMMER_MASS: f64 = 0.0006;
const CONTACT_STIFFNESS: f64 = 2.0e9;
const CONTACT_LOSS: f64 = 3500.0;

#[derive(Clone, Copy, Default)]
pub(super) struct Step {
    pub a: f64,
    pub b: f64,
    pub compliance: f64,
}

#[derive(Clone, Copy, Default)]
pub(super) struct LinearStep {
    pub modes: [Step; MODES],
    // Endpoint compliance at the hammer and at the two pickup coordinates.
    pub cc: f64,
    pub cx: f64,
    pub cy: f64,
    pub xx: f64,
    pub yy: f64,
}

#[derive(Clone)]
pub(super) struct NoteModel {
    pub count: usize,
    pub omega: [f64; MODES],
    pub hammer: [f64; MODES],
    pub tip_x: [f64; MODES],
    pub tip_y: [f64; MODES],
    pub steps: Box<[LinearStep; DAMPER_STAGES + 1]>,
    pub geometric_stiffness: f64,
    pub pickup_gap: f64,
    pub inverse_pickup_gap: f64,
    pub output_gain: f64,
    pub rate: f64,
}

/// Hermite beam shape functions, with rotation measured in radians.
fn shape(t: f64, length: f64) -> [f64; 4] {
    [
        1.0 - 3.0 * t * t + 2.0 * t.powi(3),
        length * (t - 2.0 * t * t + t.powi(3)),
        3.0 * t * t - 2.0 * t.powi(3),
        length * (-t * t + t.powi(3)),
    ]
}

fn participation(vector: &DMatrix<f64>, mode: usize, position: f64, length: f64) -> f64 {
    let scaled = (position / length * ELEMENTS as f64).clamp(0.0, ELEMENTS as f64);
    let element = (scaled.floor() as usize).min(ELEMENTS - 1);
    let n = shape(scaled - element as f64, length / ELEMENTS as f64);
    // The extra coordinate is the tone bar; hammer and pickup touch only the tine.
    let mut value = 0.0;
    for (local, weight) in n.into_iter().enumerate() {
        let global = 2 * element + local;
        if global >= 2 {
            value += weight * vector[(global - 2, mode)];
        }
    }
    value
}

impl NoteModel {
    pub fn new(note: u8, rate: f64) -> Self {
        let frequency = 440.0 * 2.0_f64.powf((f64::from(note) - 69.0) / 12.0);
        let young: f64 = 2.0e11;
        let density = 7850.0;
        let radius: f64 = 0.00065;
        let area = std::f64::consts::PI * radius.powi(2);
        let inertia = std::f64::consts::PI * radius.powi(4) / 4.0;
        let rigidity = young * inertia;
        let length = (1.875104_f64.powi(2) * (rigidity / (density * area)).sqrt()
            / (TAU * frequency))
            .sqrt();
        let element_length = length / ELEMENTS as f64;
        let full_dofs = 2 * (ELEMENTS + 1);
        let mut full_mass = DMatrix::zeros(full_dofs, full_dofs);
        let mut full_stiffness = DMatrix::zeros(full_dofs, full_dofs);
        let l = element_length;
        let k = [
            [12.0, 6.0 * l, -12.0, 6.0 * l],
            [6.0 * l, 4.0 * l * l, -6.0 * l, 2.0 * l * l],
            [-12.0, -6.0 * l, 12.0, -6.0 * l],
            [6.0 * l, 2.0 * l * l, -6.0 * l, 4.0 * l * l],
        ];
        let m = [
            [156.0, 22.0 * l, 54.0, -13.0 * l],
            [22.0 * l, 4.0 * l * l, 13.0 * l, -3.0 * l * l],
            [54.0, 13.0 * l, 156.0, -22.0 * l],
            [-13.0 * l, -3.0 * l * l, -22.0 * l, 4.0 * l * l],
        ];
        for element in 0..ELEMENTS {
            for i in 0..4 {
                for j in 0..4 {
                    full_mass[(2 * element + i, 2 * element + j)] +=
                        density * area * l / 420.0 * m[i][j];
                    full_stiffness[(2 * element + i, 2 * element + j)] +=
                        rigidity / l.powi(3) * k[i][j];
                }
            }
        }
        // Point-mass approximation of the movable tuning spring, 92% along the tine.
        let spring_position = 0.92 * length;
        let scaled = spring_position / l;
        let spring_element = scaled.floor() as usize;
        let weights = shape(scaled - spring_element as f64, l);
        let spring_mass = density * area * length * 0.12;
        for i in 0..4 {
            for j in 0..4 {
                full_mass[(2 * spring_element + i, 2 * spring_element + j)] +=
                    spring_mass * weights[i] * weights[j];
            }
        }
        let dofs = DOFS;
        let mount = dofs - 1;
        let mut mass = DMatrix::zeros(dofs, dofs);
        let mut stiffness = DMatrix::zeros(dofs, dofs);
        // Clamped tine plus an effective tone-bar coordinate. A weak elastic
        // connection near the root represents compliance of their common block.
        for i in 0..mount {
            for j in 0..mount {
                mass[(i, j)] = full_mass[(i + 2, j + 2)];
                stiffness[(i, j)] = full_stiffness[(i + 2, j + 2)];
            }
        }
        let beam_mass = mass.view((0, 0), (mount, mount)).into_owned();
        let beam_stiffness = stiffness.view((0, 0), (mount, mount)).into_owned();
        let beam_inverse = beam_mass
            .cholesky()
            .expect("positive beam mass")
            .l()
            .try_inverse()
            .expect("invertible beam mass");
        let beam_normalized: DMatrix<f64> =
            &beam_inverse * beam_stiffness * beam_inverse.transpose();
        let beam_eigen =
            SymmetricEigen::new((&beam_normalized + beam_normalized.transpose()) * 0.5);
        let beam_omega = beam_eigen.eigenvalues.min().sqrt();
        let bar_mass = 0.018 * (length / 0.1).clamp(0.3, 2.0);
        mass[(mount, mount)] = bar_mass;
        let bar_stiffness = bar_mass * (beam_omega * 1.003).powi(2);
        stiffness[(mount, mount)] = bar_stiffness;
        let coupling = bar_stiffness * 0.002;
        let root_shape = shape(0.15 * ELEMENTS as f64, l);
        let mut connection = [0.0; DOFS];
        // 15% of the length lies in the first of the six elements.
        connection[0] = -root_shape[2];
        connection[1] = -root_shape[3];
        connection[mount] = 1.0;
        for i in 0..dofs {
            for j in 0..dofs {
                stiffness[(i, j)] += coupling * connection[i] * connection[j];
            }
        }
        let inverse_l = mass
            .cholesky()
            .expect("positive mechanical mass")
            .l()
            .try_inverse()
            .expect("invertible mass factor");
        let normalized = &inverse_l * stiffness * inverse_l.transpose();
        let eigen = SymmetricEigen::new((&normalized + normalized.transpose()) * 0.5);
        let vectors = inverse_l.transpose() * &eigen.eigenvectors;
        let mut indices: [_; DOFS] = std::array::from_fn(|i| i);
        indices.sort_by(|&a, &b| eigen.eigenvalues[a].total_cmp(&eigen.eigenvalues[b]));
        // Match the fundamental exactly without changing mass or participation.
        let tuning = TAU * frequency / eigen.eigenvalues[indices[0]].sqrt();
        let mut model = Self {
            count: 0,
            omega: [0.0; MODES],
            hammer: [0.0; MODES],
            tip_x: [0.0; MODES],
            tip_y: [0.0; MODES],
            steps: Box::new([LinearStep::default(); DAMPER_STAGES + 1]),
            geometric_stiffness: 0.25 * 3.0 * rigidity / length.powi(5),
            pickup_gap: 0.008,
            inverse_pickup_gap: 1.0 / 0.008,
            output_gain: 0.0,
            rate,
        };
        let mut damper_weights = [0.0; MODES];
        // Six beam modes plus the compliant tone-bar mode, in each polarization.
        for plane in 0..2 {
            for (rank, &index) in indices.iter().take(MODES / 2).enumerate() {
                let omega = eigen.eigenvalues[index].sqrt()
                    * tuning
                    * if plane == 0 { 1.0 } else { 1.00035 };
                let hz = omega / TAU;
                if hz >= rate * 0.2 {
                    continue;
                }
                let taper = if hz <= rate * 0.15 {
                    1.0
                } else {
                    let t = (hz / rate - 0.15) / 0.05;
                    0.5 + 0.5 * (std::f64::consts::PI * t).cos()
                };
                let slot = plane * (MODES / 2) + rank;
                // Midpoint prewarping removes frequency dispersion in the linear solver.
                model.omega[slot] = 2.0 * rate * (omega / (2.0 * rate)).tan();
                let tip = participation(&vectors, index, length, length) * taper;
                let impact = participation(&vectors, index, 0.72 * length, length) * taper;
                damper_weights[slot] =
                    participation(&vectors, index, 0.45 * length, length).powi(2);
                model.hammer[slot] = impact * if plane == 0 { 1.0 } else { 0.018 };
                if plane == 0 {
                    model.tip_x[slot] = tip;
                } else {
                    model.tip_y[slot] = tip;
                }
                model.count = slot + 1;
            }
        }
        let dt = 1.0 / rate;
        let decay_seconds = (3.4 * (130.81 / frequency).powf(0.18)).clamp(1.2, 5.0);
        let reference_damper = damper_weights[0].max(1e-12);
        for stage in 0..=DAMPER_STAGES {
            let damper = stage as f64 / DAMPER_STAGES as f64;
            let mut linear = LinearStep::default();
            for (i, &damper_weight) in damper_weights.iter().enumerate().take(model.count) {
                let omega = model.omega[i];
                let sigma = 1.0 / decay_seconds
                    + 0.4 * (omega / (TAU * frequency)).sqrt()
                    + 65.0 * damper * (damper_weight / reference_damper).clamp(0.3, 4.0);
                let denominator = 1.0 + dt * sigma + dt * dt * omega * omega / 4.0;
                let step = Step {
                    a: (1.0 + dt * sigma - dt * dt * omega * omega / 4.0) / denominator,
                    b: 1.0 / denominator,
                    compliance: dt * dt / (2.0 * denominator),
                };
                linear.modes[i] = step;
                let (c, x, y) = (model.hammer[i], model.tip_x[i], model.tip_y[i]);
                linear.cc += step.compliance * c * c;
                linear.cx += step.compliance * c * x;
                linear.cy += step.compliance * c * y;
                linear.xx += step.compliance * x * x;
                linear.yy += step.compliance * y * y;
            }
            model.steps[stage] = linear;
        }
        // Conservative headroom for 32 voices; frequency compensation is output voicing.
        let static_compliance: f64 = (0..model.count)
            .map(|i| model.tip_x[i] * model.hammer[i] / model.omega[i].powi(2))
            .sum();
        let expected_motion = (HAMMER_MASS * static_compliance.abs()).sqrt() * 3.0;
        let voiced_gain =
            0.025 * model.pickup_gap / (TAU * frequency * expected_motion.max(0.00002));
        // The finite-pole flux gradient is <= 0.86 / gap. Cauchy-Schwarz
        // bounds tip speed from modal kinetic energy for a nominal hard strike.
        let participation_norm = (0..MODES)
            .map(|i| model.tip_x[i].powi(2) + model.tip_y[i].powi(2))
            .sum::<f64>()
            .sqrt();
        let peak_gain = 0.015 * model.pickup_gap
            / (0.86 * participation_norm.max(1e-12) * HAMMER_MASS.sqrt() * 3.0);
        model.output_gain = rate * voiced_gain.min(peak_gain);
        model
    }
}

#[derive(Clone, Copy)]
pub(super) struct Hammer {
    pub position: f64,
    pub velocity: f64,
}

#[derive(Clone)]
pub(super) struct Assembly {
    pub q: [f64; MODES],
    // dt * velocity, so every modal step avoids a sample-rate multiply.
    pub momentum_step: [f64; MODES],
    pub hammer: Option<Hammer>,
    pub damper: f64,
    pub held: bool,
    pub initial_energy: f64,
    pub tip: [f64; 2],
    contact_position: f64,
    last_force: [f64; 3],
}

impl Assembly {
    pub fn new() -> Self {
        Self {
            q: [0.0; MODES],
            momentum_step: [0.0; MODES],
            hammer: None,
            damper: 0.0,
            held: true,
            initial_energy: 0.0,
            tip: [0.0; 2],
            contact_position: 0.0,
            last_force: [0.0; 3],
        }
    }

    pub fn strike(&mut self, model: &NoteModel, velocity: u8) {
        let speed = 0.12 + 2.88 * (f64::from(velocity) / 127.0).powf(1.5);
        self.tip = [
            dot(&self.q, &model.tip_x, model.count),
            dot(&self.q, &model.tip_y, model.count),
        ];
        self.contact_position = dot(&self.q, &model.hammer, model.count);
        let position = self.contact_position;
        self.hammer = Some(Hammer {
            position,
            velocity: speed,
        });
        self.last_force[0] = 0.0;
        self.held = true;
        self.initial_energy = self.energy(model);
    }

    pub fn energy(&self, model: &NoteModel) -> f64 {
        let x = dot(&self.q, &model.tip_x, model.count);
        let y = dot(&self.q, &model.tip_y, model.count);
        let mut energy = 0.25 * model.geometric_stiffness * (x * x + y * y).powi(2);
        for i in 0..model.count {
            energy += 0.5
                * ((self.momentum_step[i] * model.rate).powi(2)
                    + (model.omega[i] * self.q[i]).powi(2));
        }
        if let Some(hammer) = self.hammer {
            energy += 0.5 * HAMMER_MASS * hammer.velocity.powi(2);
            let compression = hammer.position - dot(&self.q, &model.hammer, model.count);
            energy += contact_potential(compression);
        }
        energy
    }

    /// Returns false if the bounded contact solve failed. State then stays unchanged.
    pub fn advance(&mut self, model: &NoteModel, dt: f64, pedal: bool) -> bool {
        let target = if self.held || pedal { 0.0 } else { 1.0 };
        self.damper += (target - self.damper).clamp(-dt / 0.005, dt / 0.005);
        let stage = (self.damper * DAMPER_STAGES as f64 + 0.5) as usize;
        let linear = &model.steps[stage.min(DAMPER_STAGES)];
        let mut free = [0.0; MODES];
        let (mut free_x, mut free_y, mut free_contact) = (0.0, 0.0, 0.0);
        for i in 0..MODES / 2 {
            let j = i + MODES / 2;
            free[i] = linear.modes[i].a * self.q[i] + linear.modes[i].b * self.momentum_step[i];
            free[j] = linear.modes[j].a * self.q[j] + linear.modes[j].b * self.momentum_step[j];
            free_x += free[i] * model.tip_x[i];
            free_y += free[j] * model.tip_y[j];
        }
        if self.hammer.is_some() {
            free_contact = dot(&free, &model.hammer, MODES);
        }
        let [old_x, old_y] = self.tip;
        let old_contact = self.contact_position;
        let (old_delta, free_delta) = self.hammer.map_or((0.0, 0.0), |h| {
            (
                h.position - old_contact,
                h.position + dt * h.velocity - free_contact,
            )
        });
        let hammer_compliance = if self.hammer.is_some() {
            dt * dt / (2.0 * HAMMER_MASS)
        } else {
            0.0
        };
        // Previous-step forces provide a cheap prediction for the contact
        // endpoint. Newton still solves and checks the full current equations.
        let mut endpoint = [
            free_delta - (hammer_compliance + linear.cc) * self.last_force[0]
                + linear.cx * self.last_force[1]
                + linear.cy * self.last_force[2],
            free_x + linear.cx * self.last_force[0] - linear.xx * self.last_force[1],
            free_y + linear.cy * self.last_force[0] - linear.yy * self.last_force[2],
        ];
        let mut force = [0.0; 3];
        let mut converged = false;
        if self.hammer.is_none() {
            // The free-ring geometric compliance is small. Solve its two
            // endpoint equations by bounded fixed-point iteration, retaining
            // the same energy-consistent discrete gradient as the contact path.
            let k = model.geometric_stiffness / 4.0;
            let old_radius = old_x * old_x + old_y * old_y;
            let (mut x, mut y) = (free_x, free_y);
            for _ in 0..16 {
                let radius_sum = old_radius + x * x + y * y;
                force[1] = k * radius_sum * (x + old_x);
                force[2] = k * radius_sum * (y + old_y);
                let next_x = free_x - linear.xx * force[1];
                let next_y = free_y - linear.yy * force[2];
                if (next_x - x).abs().max((next_y - y).abs()) < 1e-13 {
                    converged = true;
                    break;
                }
                x = next_x;
                y = next_y;
            }
        } else {
            for _ in 0..16 {
                let (forces, jacobian, error) = residual(
                    endpoint,
                    [old_delta, old_x, old_y],
                    [free_delta, free_x, free_y],
                    linear,
                    model.geometric_stiffness,
                    dt,
                    true,
                    hammer_compliance,
                );
                force = forces;
                let error_norm = norm(error);
                if error_norm < 1e-12 {
                    converged = true;
                    break;
                }
                let Some(correction) = solve3(jacobian, error) else {
                    break;
                };
                let mut scale = 1.0;
                let mut accepted = false;
                for _ in 0..8 {
                    let candidate = std::array::from_fn(|i| endpoint[i] - scale * correction[i]);
                    let (_, _, candidate_residual) = residual(
                        candidate,
                        [old_delta, old_x, old_y],
                        [free_delta, free_x, free_y],
                        linear,
                        model.geometric_stiffness,
                        dt,
                        true,
                        hammer_compliance,
                    );
                    if norm(candidate_residual) < error_norm {
                        endpoint = candidate;
                        accepted = true;
                        break;
                    }
                    scale *= 0.5;
                }
                if !accepted {
                    break;
                }
            }
        }
        if !converged || force.iter().any(|x| !x.is_finite()) {
            return false;
        }
        self.last_force = force;
        let velocity_scale = 2.0 / dt;
        for i in 0..MODES / 2 {
            let j = i + MODES / 2;
            let mut qx = free[i] - linear.modes[i].compliance * model.tip_x[i] * force[1];
            let mut qy = free[j] - linear.modes[j].compliance * model.tip_y[j] * force[2];
            if self.hammer.is_some() {
                qx += linear.modes[i].compliance * model.hammer[i] * force[0];
                qy += linear.modes[j].compliance * model.hammer[j] * force[0];
            }
            self.momentum_step[i] = 2.0 * (qx - self.q[i]) - self.momentum_step[i];
            self.momentum_step[j] = 2.0 * (qy - self.q[j]) - self.momentum_step[j];
            self.q[i] = qx;
            self.q[j] = qy;
        }
        self.tip = [
            free_x + linear.cx * force[0] - linear.xx * force[1],
            free_y + linear.cy * force[0] - linear.yy * force[2],
        ];
        self.contact_position =
            free_contact + linear.cc * force[0] - linear.cx * force[1] - linear.cy * force[2];
        if let Some(hammer) = &mut self.hammer {
            let position = hammer.position + dt * hammer.velocity - hammer_compliance * force[0];
            hammer.velocity = (position - hammer.position) * velocity_scale - hammer.velocity;
            hammer.position = position;
            if endpoint[0] <= 0.0 && hammer.velocity < 0.0 {
                self.hammer = None;
            }
        }
        true
    }
}

pub(super) fn dot(a: &[f64; MODES], b: &[f64; MODES], count: usize) -> f64 {
    a[..count].iter().zip(&b[..count]).map(|(x, y)| x * y).sum()
}

fn contact_potential(delta: f64) -> f64 {
    CONTACT_STIFFNESS / 3.0 * delta.max(0.0).powi(3)
}

// Discrete gradient of one-sided compression energy, plus positive compression loss.
#[inline]
fn contact_gradient(old: f64, new: f64, dt: f64) -> (f64, f64) {
    let change = new - old;
    let (mut force, mut derivative) = if old >= 0.0 && new >= 0.0 {
        (
            CONTACT_STIFFNESS / 3.0 * (new * new + new * old + old * old),
            CONTACT_STIFFNESS / 3.0 * (2.0 * new + old),
        )
    } else if old <= 0.0 && new <= 0.0 {
        (0.0, 0.0)
    } else {
        let potential_change = contact_potential(new) - contact_potential(old);
        (
            potential_change / change,
            (CONTACT_STIFFNESS * new.max(0.0).powi(2) * change - potential_change) / change.powi(2),
        )
    };
    let midpoint = 0.5 * (new + old);
    if change > 0.0 && midpoint > 0.0 {
        force += CONTACT_LOSS * midpoint * change / dt;
        derivative += CONTACT_LOSS * (midpoint + 0.5 * change) / dt;
    }
    (force, derivative)
}

#[allow(clippy::too_many_arguments)]
#[inline]
fn residual(
    end: [f64; 3],
    old: [f64; 3],
    free: [f64; 3],
    l: &LinearStep,
    stiffness: f64,
    dt: f64,
    has_hammer: bool,
    hc: f64,
) -> ([f64; 3], [[f64; 3]; 3], [f64; 3]) {
    let (c, dc) = if has_hammer {
        contact_gradient(old[0], end[0], dt)
    } else {
        (0.0, 0.0)
    };
    let radius_sum = end[1].powi(2) + end[2].powi(2) + old[1].powi(2) + old[2].powi(2);
    let k = stiffness / 4.0;
    let gx = k * radius_sum * (end[1] + old[1]);
    let gy = k * radius_sum * (end[2] + old[2]);
    let gxx = k * (radius_sum + 2.0 * end[1] * (end[1] + old[1]));
    let gxy = k * 2.0 * end[2] * (end[1] + old[1]);
    let gyx = k * 2.0 * end[1] * (end[2] + old[2]);
    let gyy = k * (radius_sum + 2.0 * end[2] * (end[2] + old[2]));
    let r = [
        end[0] - free[0] + (hc + l.cc) * c - l.cx * gx - l.cy * gy,
        end[1] - free[1] - l.cx * c + l.xx * gx,
        end[2] - free[2] - l.cy * c + l.yy * gy,
    ];
    let j = [
        [
            1.0 + (hc + l.cc) * dc,
            -l.cx * gxx - l.cy * gyx,
            -l.cx * gxy - l.cy * gyy,
        ],
        [-l.cx * dc, 1.0 + l.xx * gxx, l.xx * gxy],
        [-l.cy * dc, l.yy * gyx, 1.0 + l.yy * gyy],
    ];
    ([c, gx, gy], j, r)
}

fn norm(v: [f64; 3]) -> f64 {
    v.into_iter().map(f64::abs).fold(0.0, f64::max)
}

#[inline]
fn solve3(a: [[f64; 3]; 3], b: [f64; 3]) -> Option<[f64; 3]> {
    // The endpoint Jacobian is scaled as identity plus force compliance. An
    // explicit adjugate avoids pivot and reduction loops in this tiny system.
    // Reject a singular/nonfinite result; the Newton line search still checks
    // the actual residual before accepting the correction.
    let c0 = a[1][1] * a[2][2] - a[1][2] * a[2][1];
    let c1 = a[1][2] * a[2][0] - a[1][0] * a[2][2];
    let c2 = a[1][0] * a[2][1] - a[1][1] * a[2][0];
    let determinant = a[0][0] * c0 + a[0][1] * c1 + a[0][2] * c2;
    if !determinant.is_finite() || determinant.abs() < 1e-20 {
        return None;
    }
    let inverse = 1.0 / determinant;
    let answer = [
        (c0 * b[0]
            + (a[0][2] * a[2][1] - a[0][1] * a[2][2]) * b[1]
            + (a[0][1] * a[1][2] - a[0][2] * a[1][1]) * b[2])
            * inverse,
        (c1 * b[0]
            + (a[0][0] * a[2][2] - a[0][2] * a[2][0]) * b[1]
            + (a[0][2] * a[1][0] - a[0][0] * a[1][2]) * b[2])
            * inverse,
        (c2 * b[0]
            + (a[0][1] * a[2][0] - a[0][0] * a[2][1]) * b[1]
            + (a[0][0] * a[1][1] - a[0][1] * a[1][0]) * b[2])
            * inverse,
    ];
    answer.iter().all(|x| x.is_finite()).then_some(answer)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fundamental_pitch_across_keyboard_and_sample_rates() {
        let mut worst = 0.0_f64;
        for rate in [44100.0, 48000.0, 96000.0] {
            for note in 16..=127 {
                let model = NoteModel::new(note, rate * 4.0);
                let step = model.steps[0].modes[0];
                let determinant = 2.0 * step.b - step.a;
                let half_trace = (step.a + 2.0 * step.b - 1.0) / 2.0;
                let measured =
                    (half_trace / determinant.sqrt()).clamp(-1.0, 1.0).acos() * rate * 4.0 / TAU;
                let expected = 440.0 * 2.0_f64.powf((f64::from(note) - 69.0) / 12.0);
                let cents = 1200.0 * (measured / expected).log2().abs();
                worst = worst.max(cents);
                assert!(cents < 5.0, "note {note}, rate {rate}, cents {cents}");
            }
        }
        eprintln!("Worst linear fundamental pitch error: {worst:.6} cents");
    }

    #[test]
    fn geometric_discrete_gradient_matches_energy_change() {
        let l = LinearStep::default();
        let stiffness = 2e5;
        for (old, end) in [
            ([0.0, 0.001, 0.002], [0.0, -0.003, 0.001]),
            ([0.0, 0.0, 0.0], [0.0, 0.001, -0.002]),
        ] {
            let (f, _, _) = residual(end, old, [0.0; 3], &l, stiffness, 1e-5, false, 0.0);
            let energy = |p: [f64; 3]| stiffness / 4.0 * (p[1].powi(2) + p[2].powi(2)).powi(2);
            let work = f[1] * (end[1] - old[1]) + f[2] * (end[2] - old[2]);
            assert!((work - energy(end) + energy(old)).abs() < 1e-15);
        }
    }

    #[test]
    fn hammer_response_converges_with_internal_rate() {
        fn trajectory(rate: f64) -> Vec<f64> {
            let model = NoteModel::new(60, rate);
            let mut assembly = Assembly::new();
            assembly.strike(&model, 127);
            let steps = (rate / 48000.0) as usize;
            let mut output = Vec::new();
            for _ in 0..960 {
                for _ in 0..steps {
                    assert!(assembly.advance(&model, 1.0 / rate, false));
                }
                output.push(assembly.q[0] * model.tip_x[0]);
            }
            assert!(assembly.hammer.is_none());
            output
        }
        let a = trajectory(192000.0);
        let b = trajectory(384000.0);
        let c = trajectory(768000.0);
        let difference = |x: &[f64], y: &[f64]| {
            x.iter()
                .zip(y)
                .map(|(a, b)| (a - b).powi(2))
                .sum::<f64>()
                .sqrt()
        };
        let reference = c.iter().map(|x| x * x).sum::<f64>().sqrt();
        let coarse = difference(&a, &c) / reference;
        let fine = difference(&b, &c) / reference;
        eprintln!("Hammer fundamental response vs 16x: 4x error {coarse:.6}, 8x error {fine:.6}");
        assert!(coarse < 0.05);
        assert!(fine < coarse * 0.75);
    }

    #[test]
    fn contact_has_no_tension_and_dissipates() {
        for old in [-0.001, 0.0, 0.0001, 0.001] {
            for new in [-0.001, 0.0, 0.0001, 0.001] {
                let (force, derivative) = contact_gradient(old, new, 1.0 / 192000.0);
                assert!(force >= 0.0 && derivative >= 0.0);
                let loss = force * (new - old) - (contact_potential(new) - contact_potential(old));
                assert!(loss >= -1e-12);
            }
        }
    }

    #[test]
    fn energy_and_hammer_separation() {
        let model = NoteModel::new(60, 192000.0);
        let mut assembly = Assembly::new();
        assembly.strike(&model, 127);
        let mut previous = assembly.energy(&model);
        let initial = previous;
        for _ in 0..4000 {
            assert!(assembly.advance(&model, 1.0 / 192000.0, false));
            let energy = assembly.energy(&model);
            assert!(energy <= previous + initial * 1e-7, "{energy} > {previous}");
            previous = energy;
        }
        assert!(assembly.hammer.is_none());
        assert!(previous > initial * 0.01);
        assert!(previous < initial);
    }

    #[test]
    fn undamped_free_motion_conserves_energy() {
        let mut model = NoteModel::new(48, 192000.0);
        let dt = 1.0 / 192000.0;
        for i in 0..model.count {
            let w = model.omega[i];
            let d = 1.0 + dt * dt * w * w / 4.0;
            model.steps[0].modes[i] = Step {
                a: (1.0 - dt * dt * w * w / 4.0) / d,
                b: 1.0 / d,
                compliance: dt * dt / (2.0 * d),
            };
        }
        // Rebuild coordinate compliance for the undamped solver.
        let mut linear = LinearStep {
            modes: model.steps[0].modes,
            ..Default::default()
        };
        for i in 0..model.count {
            let s = linear.modes[i].compliance;
            linear.cc += s * model.hammer[i].powi(2);
            linear.cx += s * model.hammer[i] * model.tip_x[i];
            linear.cy += s * model.hammer[i] * model.tip_y[i];
            linear.xx += s * model.tip_x[i].powi(2);
            linear.yy += s * model.tip_y[i].powi(2);
        }
        model.steps[0] = linear;
        let mut assembly = Assembly::new();
        assembly.q[0] = 0.00002;
        assembly.q[model.count / 2] = 0.00001;
        assembly.tip = [
            dot(&assembly.q, &model.tip_x, model.count),
            dot(&assembly.q, &model.tip_y, model.count),
        ];
        assembly.contact_position = dot(&assembly.q, &model.hammer, model.count);
        let initial = assembly.energy(&model);
        for _ in 0..20000 {
            assert!(assembly.advance(&model, dt, false));
        }
        assert!((assembly.energy(&model) / initial - 1.0).abs() < 1e-7);
    }
}
