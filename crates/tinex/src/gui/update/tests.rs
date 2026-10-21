use super::*;
use tinex_core::track::TrackId;

#[test]
fn arming_replaces_the_set_and_waits_for_confirmation() {
    let mut state = AppState::default();
    let first = create(&mut state);
    let second = create(&mut state);
    confirm(&mut state, first.id());
    confirm(&mut state, second.id());
    let button = state.tracks[0].id;
    state.page = Page::Track(first.id());
    for (id, expected, confirmed) in [
        (first.id(), [first.id()].into(), [true, false]),
        (second.id(), [second.id()].into(), [false, true]),
        (second.id(), Default::default(), [false, false]),
    ] {
        let before: Vec<_> = state.tracks.iter().map(|track| track.armed).collect();
        let result = update(&mut state, Action::Ui(UiAction::ToggleTrackArm(id)));
        assert!(result.changed);
        assert!(matches!(result.effects.as_slice(), [Effect {
            request: TinexRequest::ArmTracks(ids), operation: Operation::ArmTracks,
        }] if *ids == expected));
        assert!(state.arm_request_in_flight);
        assert_eq!(
            state
                .tracks
                .iter()
                .map(|track| track.armed)
                .collect::<Vec<_>>(),
            before
        );
        for other in [first.id(), second.id()] {
            let ignored = update(&mut state, Action::Ui(UiAction::ToggleTrackArm(other)));
            assert!(!ignored.changed && ignored.effects.is_empty());
        }
        let notification = || Action::Notification(TinexNotification::ArmTracks(expected.clone()));
        assert!(update(&mut state, notification()).changed);
        assert!(!state.arm_request_in_flight);
        assert_eq!(
            state
                .tracks
                .iter()
                .map(|track| track.armed)
                .collect::<Vec<_>>(),
            confirmed
        );
        assert!(!update(&mut state, notification()).changed);
        assert_eq!(state.page, Page::Track(first.id()));
        assert_eq!(state.tracks[0].id, button);
    }
}

#[test]
fn arming_ignores_unavailable_tracks_and_late_deleted_ids() {
    let mut state = AppState::default();
    let first = create(&mut state);
    let second = create(&mut state);
    for id in [first.id(), TrackId::new()] {
        assert!(
            update(&mut state, Action::Ui(UiAction::ToggleTrackArm(id)))
                .effects
                .is_empty()
        );
    }
    confirm(&mut state, first.id());
    confirm(&mut state, second.id());
    state.tracks[0].deleting = true;
    assert!(
        update(&mut state, Action::Ui(UiAction::ToggleTrackArm(first.id())))
            .effects
            .is_empty()
    );
    state.tracks[0].deleting = false;
    update(&mut state, Action::Ui(UiAction::ToggleTrackArm(first.id())));
    let first_id = first.id();
    update(
        &mut state,
        Action::Notification(TinexNotification::TrackDeleted(first)),
    );
    assert!(state.arm_request_in_flight);
    assert!(
        update(
            &mut state,
            Action::Notification(TinexNotification::ArmTracks(
                [first_id, TrackId::new()].into(),
            ))
        )
        .changed
    );
    assert!(!state.arm_request_in_flight);
    assert!(!state.tracks[0].armed);
    update(
        &mut state,
        Action::Notification(TinexNotification::ArmTracks([second.id()].into())),
    );
    assert!(state.tracks[0].armed);
    update(
        &mut state,
        Action::Notification(TinexNotification::TrackDeleted(second)),
    );
    assert!(state.tracks.is_empty());
}

#[test]
fn arming_failure_and_disconnection_preserve_confirmed_state() {
    for failed_send in [false, true] {
        let mut state = AppState::default();
        let track = create(&mut state);
        confirm(&mut state, track.id());
        update(
            &mut state,
            Action::Notification(TinexNotification::ArmTracks([track.id()].into())),
        );
        let effect = update(&mut state, Action::Ui(UiAction::ToggleTrackArm(track.id())))
            .effects
            .remove(0);
        update(
            &mut state,
            if failed_send {
                Action::RequestFailed(effect.operation)
            } else {
                Action::Disconnected
            },
        );
        assert!(!state.connected && !state.arm_request_in_flight);
        assert!(state.tracks[0].armed);
        assert!(
            update(&mut state, Action::Ui(UiAction::ToggleTrackArm(track.id())))
                .effects
                .is_empty()
        );
        assert!(
            !update(
                &mut state,
                Action::Notification(TinexNotification::ArmTracks(Default::default()))
            )
            .changed
        );
        assert!(state.tracks[0].armed);
    }
}

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
    let survivor_id = state.tracks[1].id;
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
    assert_eq!(state.tracks[0].id, survivor_id);
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
fn level_responses_preserve_navigation_and_track_identity() {
    let mut state = AppState::default();
    let track = create(&mut state);
    confirm(&mut state, track.id());
    let row = state.tracks[0].id;
    update(&mut state, Action::Ui(UiAction::OpenTrack(track.id())));
    assert_eq!(update(&mut state, Action::SampleLevels).effects.len(), 1);
    update(
        &mut state,
        Action::Notification(TinexNotification::Status {
            output_level: 2.0,
            tracks: std::collections::HashMap::from([(track.id(), 2.0)]),
            transport: tinex_core::Transport::default(),
        }),
    );
    assert_eq!(state.page, Page::Track(track.id()));
    assert_eq!(state.tracks[0].output_level, 2.0);
    assert_eq!(state.tracks[0].id, row);
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
    let track_id = state.tracks[0].id;
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
        assert_eq!(state.tracks[0].id, track_id);
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

fn add_confirmed_plugin(
    state: &mut AppState,
    track_id: TrackId,
) -> tinex_core::plugin::PluginInstance {
    update(
        state,
        Action::Ui(UiAction::OpenPlugins {
            track_id,
            index: u16::MAX,
        }),
    );
    let effect = update(
        state,
        Action::Ui(UiAction::AddPlugin { track_id, index: 0 }),
    )
    .effects
    .remove(0);
    let TinexRequest::AddPlugin { plugin, .. } = effect.request else {
        panic!("expected plugin addition")
    };
    let index = state
        .tracks
        .iter()
        .find(|track| track.id == track_id)
        .unwrap()
        .plugins
        .len() as u16;
    update(
        state,
        Action::Notification(TinexNotification::PluginAdded {
            track_id,
            plugin_id: plugin.id(),
            index,
        }),
    );
    plugin
}

#[test]
fn plugin_additions_use_selected_positions_and_match_confirmations_by_id() {
    let mut state = AppState::default();
    let first = create(&mut state);
    let second = create(&mut state);
    confirm(&mut state, first.id());
    confirm(&mut state, second.id());
    add_confirmed_plugin(&mut state, first.id());
    add_confirmed_plugin(&mut state, first.id());
    let before = state.tracks[0].plugins.clone();
    let mut pending = Vec::new();
    for (position, choice) in [(1, 4), (0, 0)] {
        update(
            &mut state,
            Action::Ui(UiAction::OpenPlugins {
                track_id: first.id(),
                index: position,
            }),
        );
        assert_eq!(state.page, Page::Plugins(first.id(), position));
        let result = update(
            &mut state,
            Action::Ui(UiAction::AddPlugin {
                track_id: first.id(),
                index: choice,
            }),
        );
        let effect = result.effects.into_iter().next().unwrap();
        let TinexRequest::AddPlugin {
            track_id,
            plugin,
            index,
        } = effect.request
        else {
            panic!("expected plugin addition")
        };
        assert_eq!(track_id, first.id());
        assert_eq!(index, position);
        pending.push(state.tracks[0].pending_plugins[&plugin.id()].clone());
        assert_eq!(state.page, Page::Track(first.id()));
        assert_eq!(state.tracks[0].plugins, before);
    }
    for (track_id, plugin_id) in [
        (TrackId::new(), pending[0].id),
        (second.id(), pending[0].id),
        (first.id(), tinex_core::plugin::PluginInstanceId::new()),
    ] {
        assert!(
            !update(
                &mut state,
                Action::Notification(TinexNotification::PluginAdded {
                    track_id,
                    plugin_id,
                    index: 0
                })
            )
            .changed
        );
    }
    assert_eq!(state.tracks[0].pending_plugins.len(), 2);
    // Confirm by instance id and actual engine position, even if it differs
    // from the requested position or pending request order.
    for (plugin_id, index) in [(pending[1].id, 2), (pending[0].id, 1)] {
        assert!(
            update(
                &mut state,
                Action::Notification(TinexNotification::PluginAdded {
                    track_id: first.id(),
                    plugin_id,
                    index
                })
            )
            .changed
        );
        assert!(
            !update(
                &mut state,
                Action::Notification(TinexNotification::PluginAdded {
                    track_id: first.id(),
                    plugin_id,
                    index
                })
            )
            .changed
        );
    }
    assert_eq!(
        state.tracks[0].plugins,
        [
            before[0].clone(),
            pending[0].clone(),
            before[1].clone(),
            pending[1].clone()
        ]
    );
    assert!(state.tracks[0].pending_plugins.is_empty());
    assert!(state.tracks[1].plugins.is_empty());
}

#[test]
fn failed_insertions_clear_pending_plugins_and_preserve_the_confirmed_chain() {
    for failed_send in [false, true] {
        let mut state = AppState::default();
        let track = create(&mut state);
        confirm(&mut state, track.id());
        add_confirmed_plugin(&mut state, track.id());
        let before = state.tracks[0].plugins.clone();
        update(
            &mut state,
            Action::Ui(UiAction::OpenPlugins {
                track_id: track.id(),
                index: 0,
            }),
        );
        let effect = update(
            &mut state,
            Action::Ui(UiAction::AddPlugin {
                track_id: track.id(),
                index: 4,
            }),
        )
        .effects
        .remove(0);
        assert_eq!(state.tracks[0].pending_plugins.len(), 1);
        update(
            &mut state,
            if failed_send {
                Action::RequestFailed(effect.operation)
            } else {
                Action::Disconnected
            },
        );
        assert!(!state.connected);
        assert!(state.tracks[0].pending_plugins.is_empty());
        assert_eq!(state.tracks[0].plugins, before);
        assert!(
            !update(
                &mut state,
                Action::Ui(UiAction::OpenPlugins {
                    track_id: track.id(),
                    index: 0,
                })
            )
            .changed
        );
    }
}

#[test]
fn plugin_deletion_waits_for_confirmation_and_preserves_other_instances() {
    let mut state = AppState::default();
    let first = create(&mut state);
    let second = create(&mut state);
    confirm(&mut state, first.id());
    confirm(&mut state, second.id());
    let removed = add_confirmed_plugin(&mut state, first.id());
    let survivor = add_confirmed_plugin(&mut state, first.id());
    let other = add_confirmed_plugin(&mut state, second.id());
    state.page = Page::Track(first.id());
    let button = state.tracks[0].plugins[1].id;
    let action = UiAction::DeletePlugin {
        track_id: first.id(),
        plugin_id: removed.id(),
    };
    let result = update(&mut state, Action::Ui(action));
    assert!(result.changed);
    assert!(matches!(result.effects.as_slice(), [Effect {
        request: TinexRequest::DeletePlugin(track_id, plugin_id), ..
    }] if *track_id == first.id() && *plugin_id == removed.id()));
    assert_eq!(state.tracks[0].plugins.len(), 2);
    assert!(state.tracks[0].plugins[0].deleting);
    assert!(update(&mut state, Action::Ui(action)).effects.is_empty());
    assert!(
        update(
            &mut state,
            Action::Ui(UiAction::DeletePlugin {
                track_id: second.id(),
                plugin_id: removed.id(),
            })
        )
        .effects
        .is_empty()
    );
    assert!(
        update(
            &mut state,
            Action::Ui(UiAction::DeletePlugin {
                track_id: TrackId::new(),
                plugin_id: removed.id(),
            })
        )
        .effects
        .is_empty()
    );
    assert!(
        update(
            &mut state,
            Action::Notification(TinexNotification::PluginDeleted(removed))
        )
        .changed
    );
    assert_eq!(state.tracks[0].plugins[0].id, survivor.id());
    assert_eq!(state.tracks[0].plugins[0].id, button);
    assert_eq!(state.tracks[1].plugins[0].id, other.id());
    assert_eq!(state.page, Page::Track(first.id()));
    assert!(update(&mut state, Action::Ui(action)).effects.is_empty());
    update(&mut state, Action::Ui(UiAction::DeleteTrack(first.id())));
    assert!(
        update(
            &mut state,
            Action::Ui(UiAction::DeletePlugin {
                track_id: first.id(),
                plugin_id: survivor.id(),
            })
        )
        .effects
        .is_empty()
    );
    update(
        &mut state,
        Action::Notification(TinexNotification::TrackDeleted(first)),
    );
    assert!(
        !update(
            &mut state,
            Action::Notification(TinexNotification::PluginDeleted(survivor))
        )
        .changed
    );
}

#[test]
fn failed_plugin_deletion_and_disconnection_preserve_confirmed_plugins() {
    for failed_send in [false, true] {
        let mut state = AppState::default();
        let track = create(&mut state);
        confirm(&mut state, track.id());
        let plugin = add_confirmed_plugin(&mut state, track.id());
        let before = state.tracks[0].plugins.clone();
        let action = UiAction::DeletePlugin {
            track_id: track.id(),
            plugin_id: plugin.id(),
        };
        let effect = update(&mut state, Action::Ui(action)).effects.remove(0);
        if failed_send {
            let (sender, receiver) = std::sync::mpsc::channel();
            drop(receiver);
            let failure = crate::gui::dispatch::execute_effect(&sender, effect).unwrap();
            update(&mut state, failure);
        } else {
            update(&mut state, Action::Disconnected);
        }
        assert!(!state.connected);
        assert_eq!(state.tracks[0].plugins, before);
        assert_eq!(state.page, Page::Track(track.id()));
        assert!(update(&mut state, Action::Ui(action)).effects.is_empty());
    }
}
