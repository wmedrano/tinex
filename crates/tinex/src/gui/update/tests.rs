use super::*;
use tinex_core::track::TrackId;

fn create(state: &mut AppState) -> Track {
    let result = update(state, Action::Ui(UiAction::CreateTrack));
    assert!(result.changed);
    assert_eq!(result.effects.len(), 1);
    match result.effects.into_iter().next().unwrap().request {
        TinexRequest::NewTrack(track) => track,
        _ => panic!("wrong effect"),
    }
}

fn confirm(state: &mut AppState, id: TrackId) {
    assert!(
        update(
            state,
            Action::Notification(TinexNotification::TrackCreated(id))
        )
        .changed
    );
}

#[test]
fn creation_waits_for_matching_notification_and_is_idempotent() {
    let mut state = AppState::default();
    let track = create(&mut state);
    assert!(state.tracks.is_empty());
    assert_eq!(state.pending[&track.id()].name, "Track 1");
    let unknown = TrackId::new();
    assert!(
        !update(
            &mut state,
            Action::Notification(TinexNotification::TrackCreated(unknown))
        )
        .changed
    );
    assert_eq!(state.pending.len(), 1);
    confirm(&mut state, track.id());
    assert!(state.pending.is_empty());
    assert_eq!(state.tracks[0].id, track.id());
    assert_eq!(state.tracks[0].name, "Track 1");
    assert!(
        !update(
            &mut state,
            Action::Notification(TinexNotification::TrackCreated(track.id()))
        )
        .changed
    );
    assert_eq!(state.tracks.len(), 1);
}

#[test]
fn concurrent_creations_keep_names_and_request_order() {
    let mut state = AppState::default();
    let first = create(&mut state);
    let second = create(&mut state);
    confirm(&mut state, second.id());
    confirm(&mut state, first.id());
    assert_eq!(
        state
            .tracks
            .iter()
            .map(|row| row.name.as_str())
            .collect::<Vec<_>>(),
        ["Track 1", "Track 2"]
    );
    assert_eq!(state.tracks[0].id, first.id());
    assert_eq!(state.tracks[1].id, second.id());
}

#[test]
fn rejection_clears_pending_and_keeps_numbering_monotonic() {
    let mut state = AppState::default();
    let rejected = create(&mut state);
    update(
        &mut state,
        Action::Notification(TinexNotification::TrackCreationFailed(rejected)),
    );
    assert!(state.pending.is_empty());
    assert!(state.tracks.is_empty());
    assert!(state.message.as_deref().unwrap().contains("Track 1"));
    let second = create(&mut state);
    confirm(&mut state, second.id());
    assert_eq!(state.tracks[0].name, "Track 2");
    assert!(state.message.is_none());
}

#[test]
fn deletion_waits_for_confirmation_and_does_not_rename_survivors() {
    let mut state = AppState::default();
    let first = create(&mut state);
    let second = create(&mut state);
    confirm(&mut state, first.id());
    confirm(&mut state, second.id());
    let survivor_widget = state.tracks[1].delete_button;
    let result = update(&mut state, Action::Ui(UiAction::DeleteTrack(first.id())));
    assert!(matches!(result.effects[0].request, TinexRequest::DeleteTrack(id) if id == first.id()));
    assert_eq!(state.tracks.len(), 2);
    assert!(state.tracks[0].deleting);
    assert!(
        update(&mut state, Action::Ui(UiAction::DeleteTrack(first.id())))
            .effects
            .is_empty()
    );
    update(
        &mut state,
        Action::Notification(TinexNotification::TrackDeleted(first)),
    );
    assert_eq!(state.tracks.len(), 1);
    assert_eq!(state.tracks[0].name, "Track 2");
    assert_eq!(state.tracks[0].delete_button, survivor_widget);
    let third = create(&mut state);
    confirm(&mut state, third.id());
    assert_eq!(state.tracks[1].name, "Track 3");
}

#[test]
fn unknown_deletions_and_failures_are_harmless() {
    let mut state = AppState::default();
    let track = create(&mut state);
    confirm(&mut state, track.id());
    assert!(
        update(
            &mut state,
            Action::Ui(UiAction::DeleteTrack(TrackId::new()))
        )
        .effects
        .is_empty()
    );
    assert!(
        !update(
            &mut state,
            Action::Notification(TinexNotification::TrackDeleted(Track::new()))
        )
        .changed
    );
    assert!(
        !update(
            &mut state,
            Action::Notification(TinexNotification::TrackCreationFailed(Track::new()))
        )
        .changed
    );
    assert_eq!(state.tracks.len(), 1);
}

#[test]
fn navigation_is_local_idempotent_and_only_opens_confirmed_tracks() {
    let mut state = AppState::default();
    let track = create(&mut state);
    for id in [track.id(), TrackId::new()] {
        let result = update(&mut state, Action::Ui(UiAction::OpenTrack(id)));
        assert!(!result.changed);
        assert!(result.effects.is_empty());
        assert_eq!(state.page, Page::Main);
    }
    confirm(&mut state, track.id());
    let result = update(&mut state, Action::Ui(UiAction::OpenTrack(track.id())));
    assert!(result.changed);
    assert!(result.effects.is_empty());
    assert_eq!(state.page, Page::Track(track.id()));
    assert!(!update(&mut state, Action::Ui(UiAction::OpenTrack(track.id()))).changed);
    assert!(!update(&mut state, Action::Ui(UiAction::OpenTrack(TrackId::new()))).changed);
    assert_eq!(state.page, Page::Track(track.id()));

    update(&mut state, Action::Disconnected);
    assert_eq!(state.page, Page::Track(track.id()));
    let result = update(&mut state, Action::Ui(UiAction::BackToMain));
    assert!(result.changed);
    assert!(result.effects.is_empty());
    assert_eq!(state.page, Page::Main);
    assert!(!update(&mut state, Action::Ui(UiAction::BackToMain)).changed);
    assert!(update(&mut state, Action::Ui(UiAction::OpenTrack(track.id()))).changed);
    assert_eq!(state.page, Page::Track(track.id())); // Read-only navigation after disconnect.
}

#[test]
fn only_confirmed_deletion_of_the_selected_track_returns_to_main() {
    let mut state = AppState::default();
    let first = create(&mut state);
    let second = create(&mut state);
    confirm(&mut state, first.id());
    confirm(&mut state, second.id());
    update(&mut state, Action::Ui(UiAction::OpenTrack(second.id())));
    update(&mut state, Action::Ui(UiAction::DeleteTrack(second.id())));
    assert_eq!(state.page, Page::Track(second.id()));
    update(
        &mut state,
        Action::Notification(TinexNotification::TrackDeleted(first)),
    );
    assert_eq!(state.page, Page::Track(second.id()));
    update(
        &mut state,
        Action::Notification(TinexNotification::TrackDeleted(second)),
    );
    assert_eq!(state.page, Page::Main);
    assert!(state.tracks.is_empty());
}

#[test]
fn level_responses_preserve_navigation_and_widget_identity() {
    let mut state = AppState::default();
    let track = create(&mut state);
    confirm(&mut state, track.id());
    let open_button = state.tracks[0].open_button;
    update(&mut state, Action::Ui(UiAction::OpenTrack(track.id())));
    assert_eq!(update(&mut state, Action::SampleLevels).effects.len(), 1);
    update(
        &mut state,
        Action::Notification(TinexNotification::OutputLevel {
            output_level: 2.0,
            tracks: std::collections::HashMap::from([(track.id(), 2.0)]),
        }),
    );
    assert_eq!(state.page, Page::Track(track.id()));
    assert_eq!(state.tracks[0].output_level, 2.0);
    assert_eq!(state.tracks[0].open_button, open_button);
}

#[test]
fn disconnection_settles_pending_operations_and_disables_actions() {
    let mut state = AppState::default();
    let track = create(&mut state);
    confirm(&mut state, track.id());
    create(&mut state);
    update(&mut state, Action::Ui(UiAction::DeleteTrack(track.id())));
    update(&mut state, Action::Disconnected);
    assert!(!state.connected);
    assert!(state.pending.is_empty());
    assert!(!state.tracks[0].deleting);
    assert!(state.message.as_deref().unwrap().contains("disconnected"));
    assert!(
        update(&mut state, Action::Ui(UiAction::CreateTrack))
            .effects
            .is_empty()
    );
    assert!(
        update(&mut state, Action::Ui(UiAction::DeleteTrack(track.id())))
            .effects
            .is_empty()
    );
    assert!(!update(&mut state, Action::Disconnected).changed);
}

#[test]
fn failed_send_clears_all_pending_operations_without_removing_confirmed_rows() {
    let mut state = AppState::default();
    let confirmed = create(&mut state);
    confirm(&mut state, confirmed.id());
    let pending = create(&mut state);
    update(
        &mut state,
        Action::Ui(UiAction::DeleteTrack(confirmed.id())),
    );
    update(
        &mut state,
        Action::RequestFailed(Operation::Create(pending.id())),
    );
    assert!(state.pending.is_empty());
    assert!(!state.tracks[0].deleting);
    assert_eq!(state.tracks[0].name, "Track 1");
    assert!(!state.connected);
}

#[test]
fn volume_changes_use_linear_gain_and_affect_only_the_requested_track() {
    let mut state = AppState::default();
    let first = create(&mut state);
    let second = create(&mut state);
    confirm(&mut state, first.id());
    confirm(&mut state, second.id());
    assert_eq!(state.tracks[0].volume_db, 0.0);
    let slider_id = state.tracks[0].volume_slider;
    for (db, gain) in [
        (-61.0, 0.0),
        (-60.0, 0.001),
        (-6.0, 10.0_f32.powf(-0.3)),
        (0.0, 1.0),
        (6.0, 10.0_f32.powf(0.3)),
        (12.0, 10.0_f32.powf(0.6)),
    ] {
        let result = update(
            &mut state,
            Action::Ui(UiAction::SetTrackVolume {
                track_id: first.id(),
                db,
            }),
        );
        assert!(result.changed);
        assert!(
            matches!(result.effects.as_slice(), [Effect { request: TinexRequest::SetTrackVolume(id, value), .. }]
            if *id == first.id() && (*value - gain).abs() < 1e-6)
        );
        assert_eq!(state.tracks[0].volume_db, db);
        assert_eq!(state.tracks[0].volume_slider, slider_id);
        assert_eq!(state.tracks[1].volume_db, 0.0);
        let repeated = update(
            &mut state,
            Action::Ui(UiAction::SetTrackVolume {
                track_id: first.id(),
                db,
            }),
        );
        assert!(!repeated.changed && repeated.effects.is_empty());
    }
}

#[test]
fn volume_rejects_invalid_unavailable_tracks_and_restores_failed_changes() {
    let mut state = AppState::default();
    let track = create(&mut state);
    let action = |track_id, db| Action::Ui(UiAction::SetTrackVolume { track_id, db });
    assert!(
        update(&mut state, action(track.id(), -6.0))
            .effects
            .is_empty()
    );
    confirm(&mut state, track.id());
    for (id, db) in [
        (TrackId::new(), -6.0),
        (track.id(), f32::NAN),
        (track.id(), f32::INFINITY),
        (track.id(), f32::NEG_INFINITY),
    ] {
        let result = update(&mut state, action(id, db));
        assert!(!result.changed && result.effects.is_empty());
    }
    state.tracks[0].deleting = true;
    assert!(
        update(&mut state, action(track.id(), -6.0))
            .effects
            .is_empty()
    );
    state.tracks[0].deleting = false;
    let effect = update(&mut state, action(track.id(), -6.0))
        .effects
        .remove(0);
    assert_eq!(state.tracks[0].volume_db, -6.0);
    update(&mut state, Action::RequestFailed(effect.operation));
    assert_eq!(state.tracks[0].volume_db, 0.0);
    assert!(!state.connected);
    assert!(
        update(&mut state, action(track.id(), -6.0))
            .effects
            .is_empty()
    );
}

#[test]
fn volume_above_twelve_db_is_clamped_to_the_new_limit() {
    let mut state = AppState::default();
    let track = create(&mut state);
    confirm(&mut state, track.id());
    let result = update(
        &mut state,
        Action::Ui(UiAction::SetTrackVolume {
            track_id: track.id(),
            db: 24.0,
        }),
    );
    assert_eq!(state.tracks[0].volume_db, 12.0);
    assert!(
        matches!(result.effects.as_slice(), [Effect { request: TinexRequest::SetTrackVolume(id, gain), .. }]
        if *id == track.id() && (*gain - 10.0_f32.powf(0.6)).abs() < 1e-6)
    );
}
