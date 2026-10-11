use super::*;
use std::collections::HashMap;
use tinex_core::track::TrackId;

fn create(state: &mut AppState) -> Track {
    let effect = update(state, Action::Ui(UiAction::CreateTrack))
        .effects
        .remove(0);
    let TinexRequest::NewTrack(track) = effect.request else {
        panic!("expected create");
    };
    track
}

fn confirm(state: &mut AppState, track: &Track) {
    update(
        state,
        Action::Notification(TinexNotification::TrackCreated(track.id())),
    );
}

fn sample(state: &mut AppState) -> HashMap<TrackId, f32> {
    let result = update(state, Action::SampleLevels);
    assert!(!result.changed); // Transport bookkeeping does not change the view.
    assert_eq!(result.effects.len(), 1);
    let effect = result.effects.into_iter().next().unwrap();
    assert!(matches!(effect.operation, Operation::SampleLevels));
    let TinexRequest::OutputLevel(tracks) = effect.request else {
        panic!("expected levels");
    };
    tracks
}

fn reply(state: &mut AppState, tracks: HashMap<TrackId, f32>) -> bool {
    let result = update(
        state,
        Action::Notification(TinexNotification::OutputLevel {
            output_level: 0.0,
            tracks,
        }),
    );
    assert!(result.effects.is_empty()); // Responses never trigger another sample.
    result.changed
}

#[test]
fn samples_only_confirmed_tracks_and_keeps_one_request_in_flight() {
    let mut state = AppState::default();
    let empty = sample(&mut state);
    assert!(empty.is_empty()); // Master sampling continues without any track rows.
    assert!(!reply(&mut state, empty));
    let confirmed = create(&mut state);
    let _pending = create(&mut state);
    confirm(&mut state, &confirmed);
    assert_eq!(state.tracks[0].output_level, 0.0);
    update(
        &mut state,
        Action::Ui(UiAction::DeleteTrack(confirmed.id())),
    );
    let tracks = sample(&mut state);
    assert_eq!(tracks.len(), 1);
    assert_eq!(tracks[&confirmed.id()], 0.0);
    for _ in 0..10 {
        assert!(update(&mut state, Action::SampleLevels).effects.is_empty());
    }
    assert!(state.level_request_in_flight);
    assert!(!reply(&mut state, tracks));
    assert!(!state.level_request_in_flight);
    assert_eq!(sample(&mut state).len(), 1);
}

#[test]
fn master_updates_without_tracks_and_sanitizes_values_without_clamping_headroom() {
    let mut state = AppState::default();
    assert_eq!(state.master_output_level, 0.0);
    for value in [
        0.5,
        2.0,
        f32::INFINITY,
        0.0,
        -1.0,
        f32::NAN,
        f32::NEG_INFINITY,
    ] {
        let tracks = sample(&mut state);
        assert!(tracks.is_empty());
        let expected = sanitize_level(value);
        let changed = state.master_output_level != expected;
        let result = update(
            &mut state,
            Action::Notification(TinexNotification::OutputLevel {
                output_level: value,
                tracks,
            }),
        );
        assert_eq!(result.changed, changed);
        assert_eq!(state.master_output_level, expected);
        assert!(!state.level_request_in_flight);
        assert!(result.effects.is_empty());
        assert!(
            !update(
                &mut state,
                Action::Notification(TinexNotification::OutputLevel {
                    output_level: value,
                    tracks: HashMap::new(),
                })
            )
            .changed
        );
    }
    update(&mut state, Action::Disconnected);
    assert!(
        !update(
            &mut state,
            Action::Notification(TinexNotification::OutputLevel {
                output_level: 2.0,
                tracks: HashMap::new(),
            })
        )
        .changed
    );
    assert_eq!(state.master_output_level, 0.0);
}

#[test]
fn returned_map_allocation_is_reused_without_clamping_hot_levels() {
    let mut state = AppState::default();
    let track = create(&mut state);
    confirm(&mut state, &track);
    let mut tracks = sample(&mut state);
    tracks.insert(track.id(), 2.0);
    let slot = tracks.get(&track.id()).unwrap() as *const f32;
    let capacity = tracks.capacity();
    assert!(reply(&mut state, tracks));
    assert_eq!(state.tracks[0].output_level, 2.0);
    assert_eq!(
        state.level_request_buffer.get(&track.id()).unwrap() as *const f32,
        slot
    );
    let tracks = sample(&mut state);
    assert_eq!(tracks.capacity(), capacity);
    assert_eq!(tracks.get(&track.id()).unwrap() as *const f32, slot);
}

#[test]
fn unchanged_or_invalid_values_do_not_rebuild_forever() {
    let mut state = AppState::default();
    let track = create(&mut state);
    confirm(&mut state, &track);
    for value in [0.0, -1.0, -0.0, f32::NAN, f32::NEG_INFINITY] {
        assert!(!reply(&mut state, HashMap::from([(track.id(), value)])));
        assert_eq!(state.tracks[0].output_level.to_bits(), 0.0_f32.to_bits());
    }
    for value in [0.5, 2.0, f32::INFINITY] {
        assert!(reply(&mut state, HashMap::from([(track.id(), value)])));
        assert!(!reply(&mut state, HashMap::from([(track.id(), value)])));
        assert_eq!(state.tracks[0].output_level, value);
    }
}

#[test]
fn responses_follow_stable_ids_during_creation_and_deletion() {
    let mut state = AppState::default();
    let first = create(&mut state);
    let second = create(&mut state);
    confirm(&mut state, &first);
    let mut tracks = sample(&mut state);
    confirm(&mut state, &second); // New row was not included in this sample.
    state.tracks[1].output_level = 0.5;
    tracks.insert(first.id(), 2.0);
    let unknown = TrackId::new();
    tracks.insert(unknown, 10.0);
    assert!(reply(&mut state, tracks));
    assert_eq!(state.tracks[0].output_level, 2.0);
    assert_eq!(state.tracks[1].output_level, 0.0);
    assert!(!state.level_request_buffer.contains_key(&unknown));

    let mut tracks = sample(&mut state);
    assert_eq!(tracks.len(), 2);
    update(&mut state, Action::Ui(UiAction::DeleteTrack(first.id())));
    tracks.insert(first.id(), 3.0);
    assert!(reply(&mut state, tracks)); // Pending deletion still has a live meter.
    assert_eq!(state.tracks[0].output_level, 3.0);
    let mut tracks = sample(&mut state);
    update(
        &mut state,
        Action::Notification(TinexNotification::TrackDeleted(first)),
    );
    tracks.insert(second.id(), 0.25);
    assert!(reply(&mut state, tracks)); // Stale first ID cannot recreate its row.
    assert_eq!(state.tracks.len(), 1);
    assert_eq!(state.tracks[0].id, second.id());
    assert_eq!(state.tracks[0].output_level, 0.25);
    assert_eq!(state.level_request_buffer.len(), 1);
    assert!(reply(&mut state, HashMap::new())); // Missing values clear stale levels.
    assert_eq!(state.tracks[0].output_level, 0.0);
}

#[test]
fn disconnection_clears_levels_and_pending_transport() {
    for failure in [
        Action::Disconnected,
        Action::RequestFailed(Operation::SampleLevels),
    ] {
        let mut state = AppState::default();
        let track = create(&mut state);
        confirm(&mut state, &track);
        reply(&mut state, HashMap::from([(track.id(), 2.0)]));
        update(
            &mut state,
            Action::Notification(TinexNotification::OutputLevel {
                output_level: 3.0,
                tracks: HashMap::from([(track.id(), 2.0)]),
            }),
        );
        let tracks = sample(&mut state);
        assert!(state.level_request_in_flight);
        assert!(update(&mut state, failure).changed);
        assert!(!state.level_request_in_flight);
        assert!(state.level_request_buffer.is_empty());
        assert_eq!(state.tracks[0].output_level, 0.0);
        assert_eq!(state.master_output_level, 0.0);
        assert!(update(&mut state, Action::SampleLevels).effects.is_empty());
        assert!(!reply(&mut state, tracks));
        assert!(state.level_request_buffer.is_empty());
    }
}
