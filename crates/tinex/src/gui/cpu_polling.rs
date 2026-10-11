use std::time::{Duration, Instant};

pub(super) const CPU_INTERVAL: Duration = Duration::from_secs(5);

/// Independent of redraws and audio meter requests; never catches up in bursts.
#[derive(Default)]
pub(super) struct CpuPolling {
    next_sample: Option<Instant>,
}

impl CpuPolling {
    pub fn due(&mut self, now: Instant, eligible: bool) -> bool {
        if !eligible || self.next_sample.is_some_and(|deadline| now < deadline) {
            return false;
        }
        self.next_sample = Some(now + CPU_INTERVAL);
        true
    }

    pub fn deadline(&self, now: Instant, eligible: bool) -> Option<Instant> {
        eligible.then(|| self.next_sample.unwrap_or(now).max(now))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn samples_immediately_then_every_five_seconds_without_catch_up() {
        let now = Instant::now();
        let mut polling = CpuPolling::default();
        assert_eq!(polling.deadline(now, true), Some(now));
        assert!(polling.due(now, true));
        assert_eq!(polling.deadline(now, true), Some(now + CPU_INTERVAL));
        assert!(!polling.due(now + CPU_INTERVAL - Duration::from_nanos(1), true));
        assert!(polling.due(now + CPU_INTERVAL, true));
        let later = now + Duration::from_secs(60);
        assert!(polling.due(later, true));
        assert!(!polling.due(later, true));
        assert_eq!(polling.deadline(later, true), Some(later + CPU_INTERVAL));
    }

    #[test]
    fn ineligible_views_do_not_sample_or_schedule_and_resume_without_bursts() {
        let now = Instant::now();
        let mut polling = CpuPolling::default();
        assert!(!polling.due(now, false));
        assert_eq!(polling.deadline(now, false), None);
        assert!(polling.due(now, true));
        assert!(!polling.due(now + Duration::from_secs(1), false));
        assert!(!polling.due(now + Duration::from_secs(2), true));
        let later = now + Duration::from_secs(20);
        assert!(!polling.due(later, false));
        assert_eq!(polling.deadline(later, false), None);
        assert!(polling.due(later, true));
        assert!(!polling.due(later, true));
    }

    #[test]
    fn cpu_updates_round_percentages_and_clear_invalid_or_disconnected_values() {
        use crate::gui::{action::Action, state::AppState, update::update};
        let mut state = AppState::default();
        assert!(update(&mut state, Action::CpuLoad(12.34)).changed);
        assert_eq!(state.cpu_load, Some(12.3));
        assert!(!update(&mut state, Action::CpuLoad(12.31)).changed);
        for invalid in [f32::NAN, f32::INFINITY, -1.0] {
            update(&mut state, Action::CpuLoad(12.3));
            assert!(update(&mut state, Action::CpuLoad(invalid)).changed);
            assert_eq!(state.cpu_load, None);
        }
        update(&mut state, Action::CpuLoad(100.0));
        assert_eq!(state.cpu_load, Some(100.0));
        update(&mut state, Action::Disconnected);
        assert_eq!(state.cpu_load, None);
        assert!(!update(&mut state, Action::CpuLoad(12.3)).changed);
    }
}
