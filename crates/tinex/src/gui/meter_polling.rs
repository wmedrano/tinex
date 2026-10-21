use std::time::{Duration, Instant};

use super::action::Action;
use super::state::AppState;

pub(super) const METER_INTERVAL: Duration = Duration::from_nanos(1_000_000_000 / 30);

/// Sampling is clock-driven, never response-driven or tied to redraw frequency.
#[derive(Default)]
pub(super) struct MeterPolling {
    next_sample: Option<Instant>,
}

impl MeterPolling {
    pub fn action(&mut self, now: Instant, state: &AppState, visible: bool) -> Option<Action> {
        if !Self::eligible(state, visible)
            || self.next_sample.is_some_and(|deadline| now < deadline)
        {
            return None;
        }
        // Missed deadlines do not generate catch-up bursts.
        self.next_sample = Some(now + METER_INTERVAL);
        Some(Action::SampleLevels)
    }

    pub fn deadline(&self, now: Instant, state: &AppState, visible: bool) -> Option<Instant> {
        Self::eligible(state, visible).then(|| self.next_sample.unwrap_or(now).max(now))
    }

    fn eligible(state: &AppState, visible: bool) -> bool {
        // The master meter still needs responses with an empty track list (for
        // example, its smoothed level decays after deleting the last track).
        visible && state.connected && !state.level_request_in_flight
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::action::UiAction;
    use crate::gui::update::update;
    use tinex_core::{TinexNotification, TinexRequest};

    fn with_track() -> AppState {
        let mut state = AppState::default();
        let effect = update(&mut state, Action::Ui(UiAction::CreateTrack))
            .effects
            .remove(0);
        let TinexRequest::NewTrack(track) = effect.request else {
            panic!("expected create");
        };
        update(
            &mut state,
            Action::Notification(TinexNotification::TrackCreated(track.id())),
        );
        state
    }

    #[test]
    fn samples_at_most_thirty_times_per_second_without_catch_up_bursts() {
        let state = with_track();
        let mut polling = MeterPolling::default();
        let now = Instant::now();
        assert!(polling.action(now, &state, true).is_some());
        assert!(polling.action(now, &state, true).is_none());
        assert!(
            polling
                .action(now + METER_INTERVAL - Duration::from_nanos(1), &state, true)
                .is_none()
        );
        assert_eq!(
            polling.deadline(now, &state, true),
            Some(now + METER_INTERVAL)
        );
        assert!(polling.action(now + METER_INTERVAL, &state, true).is_some());
        let later = now + Duration::from_secs(2);
        assert!(polling.action(later, &state, true).is_some());
        assert!(polling.action(later, &state, true).is_none());
        assert_eq!(
            polling.deadline(later, &state, true),
            Some(later + METER_INTERVAL)
        );
    }

    #[test]
    fn waits_for_outstanding_request_and_resumes_on_response() {
        let mut state = with_track();
        let mut polling = MeterPolling::default();
        let now = Instant::now();
        let action = polling.action(now, &state, true).unwrap();
        let effect = update(&mut state, action).effects.remove(0);
        assert!(state.level_request_in_flight);
        assert!(
            polling
                .action(now + Duration::from_secs(1), &state, true)
                .is_none()
        );
        assert!(polling.deadline(now, &state, true).is_none());
        let TinexRequest::PollStatus {
            requested_tracks: tracks,
        } = effect.request
        else {
            panic!("expected levels");
        };
        update(
            &mut state,
            Action::Notification(TinexNotification::Status {
                output_level: 0.0,
                tracks,
                transport: tinex_core::Transport::default(),
            }),
        );
        assert!(
            polling
                .action(now + Duration::from_secs(1), &state, true)
                .is_some()
        );
    }

    #[test]
    fn inactive_occluded_and_disconnected_views_do_not_sample() {
        let now = Instant::now();
        let mut polling = MeterPolling::default();
        let mut state = with_track();
        // The coordinator passes false for suspension, occlusion, or zero size.
        assert!(polling.action(now, &state, false).is_none());
        assert!(polling.deadline(now, &state, false).is_none());
        assert!(polling.action(now, &state, true).is_some());
        assert!(polling.action(now, &state, false).is_none());
        assert!(polling.action(now, &state, true).is_none());
        assert!(polling.action(now + METER_INTERVAL, &state, true).is_some());
        state.connected = false;
        assert!(
            polling
                .action(now + Duration::from_secs(1), &state, true)
                .is_none()
        );
        assert!(polling.deadline(now, &state, true).is_none());
        state.connected = true;
        state.tracks.clear();
        let later = now + Duration::from_secs(1);
        assert!(polling.action(later, &state, true).is_some());
        assert_eq!(
            polling.deadline(later, &state, true),
            Some(later + METER_INTERVAL)
        );
    }
}
