use super::dispatch::execute_effect;
use super::state::{Page, UiPluginInstance};
use super::*;
use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Receiver, Sender, TryRecvError};
use std::sync::{Arc, Mutex, mpsc};
use tinex_core::plugin::{PluginInstance, PluginInstanceId};
use tinex_core::track::Track;
use tinex_core::{ProcessArgs, Tinex, TinexNotification, TinexRequest};

#[test]
fn lifecycle_visibility_and_unchanged_ticks_do_not_request_redraws() {
    let mut visibility = Visibility::default();
    assert!(!visibility.visible());
    visibility.active = true;
    assert!(visibility.visible());
    visibility.occluded = true;
    assert!(!visibility.visible());
    visibility.occluded = false;
    visibility.zero_size = true;
    assert!(!visibility.visible());
    visibility.zero_size = false;
    visibility.active = false;
    assert!(!visibility.visible());
    assert!(matches!(changed_result(false), MessageResult::Nop));
    assert!(matches!(changed_result(true), MessageResult::Action(())));
}

#[test]
fn asynchronous_wakeups_wait_for_acknowledgement_instead_of_accumulating() {
    use xilem::core::{SendMessage, ViewId, fork};
    let (sender, receiver) = mpsc::channel();
    let (_driver, _windows) = Xilem::new_simple(
        (),
        |_| {
            fork(
                xilem::view::label("Polling test"),
                polling(|_: &mut ()| (false, NOTIFICATION_POLL_INTERVAL)),
            )
        },
        WindowOptions::new("Test"),
    )
    .into_driver_and_windows(move |event| sender.send(event).map_err(|err| err.0));
    let first = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
    // A delayed GUI must not accumulate timer messages or catch-up bursts.
    assert!(matches!(
        receiver.recv_timeout(Duration::from_millis(60)),
        Err(mpsc::RecvTimeoutError::Timeout)
    ));
    let MasonryUserEvent::Action(_, action, id) = first else {
        panic!("expected wakeup");
    };
    assert_eq!(id, xilem::ASYNC_MARKER_WIDGET);
    let (_, message) = *action.downcast::<(Arc<[ViewId]>, SendMessage)>().unwrap();
    let ack = *message
        .downcast::<xilem::tokio::sync::oneshot::Sender<Duration>>()
        .unwrap();
    ack.send(NOTIFICATION_POLL_INTERVAL).unwrap();
    assert!(matches!(
        receiver.recv_timeout(Duration::from_secs(2)).unwrap(),
        MasonryUserEvent::Action(..)
    ));
    // Dropping the receiver/driver terminates the source; it owns no engine data.
}

// These notification-only tests do not run an engine request consumer.
fn poll_notifications(state: &mut AppState, notifications: &Receiver<TinexNotification>) -> bool {
    let (requests, _receiver) = mpsc::channel();
    super::dispatch::poll_notifications(state, notifications, &requests)
}

fn process(engine: &mut Tinex) {
    let arena = bumpalo::Bump::new();
    let mut left = [0.0; 8];
    let mut right = [0.0; 8];
    engine.process(ProcessArgs {
        sample_rate: 48_000,
        input: [&[], &[]],
        midi_input: &[],
        output: [&mut left, &mut right],
        _arena: &arena,
    });
}

fn send_actions(state: &mut AppState, sender: &Sender<TinexRequest>, action: UiAction) {
    for effect in update::update(state, Action::Ui(action)).effects {
        if let Some(failure) = execute_effect(sender, effect) {
            update::update(state, failure);
        }
    }
}

#[test]
fn engine_round_trip_confirms_creation_and_deletion() {
    let (sender, requests) = mpsc::channel();
    let (notifications, receiver) = mpsc::channel();
    let mut engine = Tinex::new(requests, notifications).unwrap();
    let mut state = AppState::default();
    send_actions(&mut state, &sender, UiAction::CreateTrack);
    assert!(state.tracks.is_empty());
    assert!(!poll_notifications(&mut state, &receiver));
    process(&mut engine);
    assert!(poll_notifications(&mut state, &receiver));
    assert_eq!(state.tracks[0].name, "Track 1");
    let id = state.tracks[0].id;
    send_actions(&mut state, &sender, UiAction::DeleteTrack(id));
    assert_eq!(state.tracks.len(), 1);
    assert!(state.tracks[0].deleting);
    process(&mut engine);
    assert!(poll_notifications(&mut state, &receiver));
    assert!(state.tracks.is_empty());
}

#[test]
fn engine_round_trip_arms_switches_and_disarms_tracks() {
    let (sender, requests) = mpsc::channel();
    let (notifications, receiver) = mpsc::channel();
    let mut engine = Tinex::new(requests, notifications).unwrap();
    let mut state = AppState::default();
    for _ in 0..2 {
        send_actions(&mut state, &sender, UiAction::CreateTrack);
    }
    process(&mut engine);
    poll_notifications(&mut state, &receiver);
    let first = state.tracks[0].id;
    let second = state.tracks[1].id;
    for (id, armed) in [
        (first, [true, false]),
        (second, [false, true]),
        (second, [false, false]),
    ] {
        let before: Vec<_> = state.tracks.iter().map(|track| track.armed).collect();
        assert!(dispatch_action(
            &mut state,
            Action::Ui(UiAction::ToggleTrackArm(id)),
            &sender,
            &receiver
        ));
        assert!(state.arm_request_in_flight);
        assert_eq!(
            state
                .tracks
                .iter()
                .map(|track| track.armed)
                .collect::<Vec<_>>(),
            before
        );
        process(&mut engine);
        assert!(super::dispatch::poll_notifications(
            &mut state, &receiver, &sender
        ));
        assert!(!state.arm_request_in_flight);
        assert_eq!(
            state
                .tracks
                .iter()
                .map(|track| track.armed)
                .collect::<Vec<_>>(),
            armed
        );
    }
}

#[test]
fn arming_dispatch_settles_queued_confirmation_before_the_next_toggle() {
    let (sender, requests) = mpsc::channel();
    let (notifications, receiver) = mpsc::channel();
    let mut state = AppState::default();
    send_actions(&mut state, &sender, UiAction::CreateTrack);
    let TinexRequest::NewTrack(track) = requests.recv().unwrap() else {
        panic!("expected track")
    };
    notifications
        .send(TinexNotification::TrackCreated(track.id()))
        .unwrap();
    dispatch_action(
        &mut state,
        Action::Ui(UiAction::ToggleTrackArm(track.id())),
        &sender,
        &receiver,
    );
    assert!(
        matches!(requests.recv().unwrap(), TinexRequest::ArmTracks(ids) if ids == [track.id()].into())
    );
    notifications
        .send(TinexNotification::ArmTracks([track.id()].into()))
        .unwrap();
    dispatch_action(
        &mut state,
        Action::Ui(UiAction::ToggleTrackArm(track.id())),
        &sender,
        &receiver,
    );
    assert!(state.tracks[0].armed && state.arm_request_in_flight);
    assert!(matches!(requests.recv().unwrap(), TinexRequest::ArmTracks(ids) if ids.is_empty()));

    // A closed request channel clears pending state without pretending disarm succeeded.
    notifications
        .send(TinexNotification::ArmTracks([track.id()].into()))
        .unwrap();
    drop(requests);
    assert!(dispatch_action(
        &mut state,
        Action::Ui(UiAction::ToggleTrackArm(track.id())),
        &sender,
        &receiver
    ));
    assert!(!state.connected && !state.arm_request_in_flight);
    assert!(state.tracks[0].armed);
}

#[test]
fn track_plugin_list_follows_engine_confirmation_and_chain_order() {
    let (sender, requests) = mpsc::channel();
    let (notifications, receiver) = mpsc::channel();
    let mut engine = Tinex::new(requests, notifications).unwrap();
    let mut state = AppState::default();
    for _ in 0..2 {
        send_actions(&mut state, &sender, UiAction::CreateTrack);
    }
    process(&mut engine);
    poll_notifications(&mut state, &receiver);
    let id = state.tracks[0].id;
    for index in [0, 4, 0] {
        send_actions(
            &mut state,
            &sender,
            UiAction::OpenPlugins {
                track_id: id,
                index: u16::MAX,
            },
        );
        send_actions(
            &mut state,
            &sender,
            UiAction::AddPlugin {
                track_id: id,
                index,
            },
        );
    }
    assert!(state.tracks[0].plugins.is_empty());
    let pending: HashMap<PluginInstanceId, UiPluginInstance> =
        state.tracks[0].pending_plugins.clone();
    assert_eq!(pending.len(), 3);
    process(&mut engine);
    assert!(poll_notifications(&mut state, &receiver));
    let confirmed = state.tracks[0].plugins.clone();
    assert_eq!(
        confirmed
            .iter()
            .map(|plugin| plugin.name)
            .collect::<Vec<_>>(),
        ["EPiano", "Delay", "EPiano"]
    );
    // Every confirmed entry is the pending plugin whose id the engine echoed back.
    assert!(confirmed.iter().all(|plugin| {
        pending
            .get(&plugin.id)
            .is_some_and(|entry| entry.name == plugin.name)
    }));
    assert_eq!(
        confirmed
            .iter()
            .map(|plugin| plugin.id)
            .collect::<HashSet<_>>()
            .len(),
        3
    );
    assert!(state.tracks[0].pending_plugins.is_empty());
    assert!(state.tracks[1].plugins.is_empty());

    // Xilem presentation/order and the fixed footer are covered by the
    // headless view harness tests; this test owns the engine round trip.

    send_actions(
        &mut state,
        &sender,
        UiAction::OpenPlugins {
            track_id: id,
            index: u16::MAX,
        },
    );
    send_actions(
        &mut state,
        &sender,
        UiAction::AddPlugin {
            track_id: id,
            index: 5,
        },
    );
    update::update(&mut state, Action::Disconnected);
    assert!(state.tracks[0].pending_plugins.is_empty());
    assert_eq!(state.tracks[0].plugins, confirmed);
}

#[test]
fn capacity_failure_is_reported_without_a_phantom_row() {
    let (sender, requests) = mpsc::channel();
    let (notifications, receiver) = mpsc::channel();
    let mut engine = Tinex::new(requests, notifications).unwrap();
    let mut state = AppState::default();
    for _ in 0..65 {
        send_actions(&mut state, &sender, UiAction::CreateTrack);
    }
    process(&mut engine);
    poll_notifications(&mut state, &receiver);
    assert_eq!(state.tracks.len(), 64);
    assert!(state.pending.is_empty());
    assert!(state.message.as_deref().unwrap().contains("Track 65"));
    let id = state.tracks[0].id;
    send_actions(&mut state, &sender, UiAction::DeleteTrack(id));
    process(&mut engine);
    poll_notifications(&mut state, &receiver);
    send_actions(&mut state, &sender, UiAction::CreateTrack);
    process(&mut engine);
    poll_notifications(&mut state, &receiver);
    assert_eq!(state.tracks.last().unwrap().name, "Track 66");
}

#[test]
fn drains_all_variants_and_disconnection_after_queued_confirmations() {
    let (sender, receiver) = mpsc::channel();
    let mut state = AppState::default();
    let mut tracks = Vec::new();
    for _ in 0..2 {
        let effect = update::update(&mut state, Action::Ui(UiAction::CreateTrack))
            .effects
            .remove(0);
        if let TinexRequest::NewTrack(track) = effect.request {
            tracks.push(track);
        }
    }
    let failed = tracks.pop().unwrap();
    let created = tracks.pop().unwrap();
    sender
        .send(TinexNotification::TrackCreated(created.id()))
        .unwrap();
    sender
        .send(TinexNotification::TrackCreationFailed(failed))
        .unwrap();
    sender
        .send(TinexNotification::OutputLevel {
            output_level: 0.5,
            tracks: HashMap::new(),
        })
        .unwrap();
    sender
        .send(TinexNotification::TrackDeleted(created))
        .unwrap();
    assert!(poll_notifications(&mut state, &receiver));
    assert!(matches!(receiver.try_recv(), Err(TryRecvError::Empty)));
    assert!(state.tracks.is_empty());
    assert!(state.pending.is_empty());

    let (request_sender, request_receiver) = mpsc::channel();
    drop(request_receiver);
    send_actions(&mut state, &request_sender, UiAction::CreateTrack);
    assert!(!state.connected);
    drop(sender);
    poll_notifications(&mut state, &receiver);
}

#[test]
fn queued_success_is_applied_before_notification_disconnection() {
    let (sender, receiver) = mpsc::channel();
    let mut state = AppState::default();
    let effect = update::update(&mut state, Action::Ui(UiAction::CreateTrack))
        .effects
        .remove(0);
    let TinexRequest::NewTrack(track) = effect.request else {
        panic!("expected create");
    };
    sender
        .send(TinexNotification::TrackCreated(track.id()))
        .unwrap();
    drop(sender);
    poll_notifications(&mut state, &receiver);
    assert_eq!(state.tracks[0].name, "Track 1");
    assert!(!state.connected);
}

#[test]
fn failed_send_preserves_already_queued_successes() {
    let (requests, request_receiver) = mpsc::channel();
    let (notifications, notification_receiver) = mpsc::channel();
    let mut state = AppState::default();
    let effect = update::update(&mut state, Action::Ui(UiAction::CreateTrack))
        .effects
        .remove(0);
    let TinexRequest::NewTrack(track) = effect.request else {
        panic!("expected create");
    };
    notifications
        .send(TinexNotification::TrackCreated(track.id()))
        .unwrap();
    drop(request_receiver);
    drop(notifications);
    dispatch_action(
        &mut state,
        Action::Ui(UiAction::CreateTrack),
        &requests,
        &notification_receiver,
    );
    assert_eq!(state.tracks.len(), 1);
    assert_eq!(state.tracks[0].name, "Track 1");
    assert!(state.pending.is_empty());
    assert!(!state.connected);
}

#[test]
fn failed_delete_send_restores_row_and_disables_engine_actions() {
    let (sender, receiver) = mpsc::channel();
    let mut state = AppState::default();
    // Use the normal creation path to reserve presentation metadata.
    let effect = update::update(&mut state, Action::Ui(UiAction::CreateTrack))
        .effects
        .remove(0);
    let TinexRequest::NewTrack(created) = effect.request else {
        panic!("expected create");
    };
    update::update(
        &mut state,
        Action::Notification(TinexNotification::TrackCreated(created.id())),
    );
    drop(receiver);
    send_actions(&mut state, &sender, UiAction::DeleteTrack(created.id()));
    assert_eq!(state.tracks.len(), 1);
    assert!(!state.tracks[0].deleting);
    assert!(!state.connected);
}

#[test]
fn returned_tracks_and_plugins_are_destroyed_on_the_polling_thread() {
    struct DropSpy(Arc<Mutex<Vec<std::thread::ThreadId>>>);
    impl Drop for DropSpy {
        fn drop(&mut self) {
            self.0.lock().unwrap().push(std::thread::current().id());
        }
    }
    impl tinex_core::plugin::Plugin for DropSpy {
        fn process(
            &mut self,
            _: &[(usize, wmidi::MidiMessage)],
            _: [&[f32]; 2],
            _: [&mut [f32]; 2],
        ) {
        }
    }
    let drops = Arc::new(Mutex::new(Vec::new()));
    let (sender, receiver) = mpsc::channel();
    let worker_drops = Arc::clone(&drops);
    std::thread::spawn(move || {
        sender
            .send(TinexNotification::TrackDeleted(Track::with_plugin(
                DropSpy(Arc::clone(&worker_drops)),
            )))
            .unwrap();
        sender
            .send(TinexNotification::TrackCreationFailed(Track::with_plugin(
                DropSpy(Arc::clone(&worker_drops)),
            )))
            .unwrap();
        sender
            .send(TinexNotification::PluginDeleted(PluginInstance::new(
                Box::new(DropSpy(worker_drops)),
            )))
            .unwrap();
    })
    .join()
    .unwrap();
    assert!(drops.lock().unwrap().is_empty());
    poll_notifications(&mut AppState::default(), &receiver);
    assert_eq!(*drops.lock().unwrap(), vec![std::thread::current().id(); 3]);
}

#[test]
fn weighted_peak_levels_above_unity_survive_the_engine_round_trip() {
    struct Loud;
    impl tinex_core::plugin::Plugin for Loud {
        fn process(
            &mut self,
            _: &[(usize, wmidi::MidiMessage)],
            _: [&[f32]; 2],
            outputs: [&mut [f32]; 2],
        ) {
            for output in outputs {
                output.fill(2.0);
            }
        }
    }
    let (sender, requests) = mpsc::channel();
    let (notifications, receiver) = mpsc::channel();
    let mut engine = Tinex::new(requests, notifications).unwrap();
    let mut state = AppState::default();
    let mut effect = update::update(&mut state, Action::Ui(UiAction::CreateTrack))
        .effects
        .remove(0);
    let TinexRequest::NewTrack(track) = &mut effect.request else {
        panic!("expected create");
    };
    let plugin = PluginInstance::new(Box::new(Loud));
    track.push_plugin(plugin);
    assert!(execute_effect(&sender, effect).is_none());
    // Allow existing smoothing to rise above unity; do not mistake it for an
    // instantaneous/sample-accurate peak detector.
    for _ in 0..600 {
        process(&mut engine);
    }
    super::dispatch::poll_notifications(&mut state, &receiver, &sender);
    assert_eq!(state.tracks[0].output_level, 0.0); // Creation alone carries no level.
    dispatch_action(&mut state, Action::SampleLevels, &sender, &receiver);
    assert!(state.level_request_in_flight);
    for _ in 0..10 {
        dispatch_action(&mut state, Action::SampleLevels, &sender, &receiver);
    }
    process(&mut engine);
    assert!(super::dispatch::poll_notifications(
        &mut state, &receiver, &sender
    ));
    assert!(matches!(receiver.try_recv(), Err(TryRecvError::Empty)));
    assert!(!state.level_request_in_flight);
    let level = state.tracks[0].output_level;
    assert!(
        level > 1.0 && level < 2.0,
        "weighted peak {level} should be above unity"
    );
    assert!(tinex_widgets::amplitude_to_dbfs(level) > 0.0);
    assert_eq!(state.master_output_level, level); // One track, so the mix is identical.

    let id = state.tracks[0].id;
    dispatch_action(
        &mut state,
        Action::Ui(UiAction::DeleteTrack(id)),
        &sender,
        &receiver,
    );
    process(&mut engine);
    super::dispatch::poll_notifications(&mut state, &receiver, &sender);
    assert!(state.tracks.is_empty());
    dispatch_action(&mut state, Action::SampleLevels, &sender, &receiver);
    process(&mut engine);
    super::dispatch::poll_notifications(&mut state, &receiver, &sender);
    assert!(state.master_output_level > 0.0 && state.master_output_level < level);
    // Even with no tracks, keep sampling the engine's decaying master envelope.
    for _ in 0..1800 {
        process(&mut engine);
    }
    dispatch_action(&mut state, Action::SampleLevels, &sender, &receiver);
    process(&mut engine);
    super::dispatch::poll_notifications(&mut state, &receiver, &sender);
    assert!(state.master_output_level < level * 0.1);
}

#[test]
fn master_level_comes_from_the_mix_not_from_adding_track_meters() {
    struct Constant(f32);
    impl tinex_core::plugin::Plugin for Constant {
        fn process(
            &mut self,
            _: &[(usize, wmidi::MidiMessage)],
            _: [&[f32]; 2],
            outputs: [&mut [f32]; 2],
        ) {
            for output in outputs {
                output.fill(self.0);
            }
        }
    }
    let (sender, requests) = mpsc::channel();
    let (notifications, receiver) = mpsc::channel();
    let mut engine = Tinex::new(requests, notifications).unwrap();
    let mut state = AppState::default();
    for value in [2.0, -2.0] {
        let mut effect = update::update(&mut state, Action::Ui(UiAction::CreateTrack))
            .effects
            .remove(0);
        let TinexRequest::NewTrack(track) = &mut effect.request else {
            panic!("expected create");
        };
        let plugin = PluginInstance::new(Box::new(Constant(value)));
        track.push_plugin(plugin);
        assert!(execute_effect(&sender, effect).is_none());
    }
    for _ in 0..600 {
        process(&mut engine);
    }
    super::dispatch::poll_notifications(&mut state, &receiver, &sender);
    dispatch_action(&mut state, Action::SampleLevels, &sender, &receiver);
    process(&mut engine);
    super::dispatch::poll_notifications(&mut state, &receiver, &sender);
    assert_eq!(state.tracks.len(), 2);
    assert!(state.tracks.iter().all(|row| row.output_level > 1.0));
    assert_eq!(state.master_output_level, 0.0); // Opposite signals cancel in the mix.
}

#[test]
fn sampling_send_failure_disconnects_without_losing_queued_confirmations() {
    let (requests, request_receiver) = mpsc::channel();
    let (notifications, receiver) = mpsc::channel();
    let mut state = AppState::default();
    let effect = update::update(&mut state, Action::Ui(UiAction::CreateTrack))
        .effects
        .remove(0);
    let TinexRequest::NewTrack(track) = effect.request else {
        panic!("expected create");
    };
    notifications
        .send(TinexNotification::TrackCreated(track.id()))
        .unwrap();
    drop(request_receiver);
    dispatch_action(&mut state, Action::SampleLevels, &requests, &receiver);
    assert_eq!(state.tracks[0].name, "Track 1");
    assert!(!state.connected);
    assert!(!state.level_request_in_flight);
    assert_eq!(state.tracks[0].output_level, 0.0);
    assert!(state.level_request_buffer.is_empty());
}

#[test]
fn volume_requests_change_the_audio_mix_without_confirmation() {
    struct Constant(f32);
    impl tinex_core::plugin::Plugin for Constant {
        fn process(
            &mut self,
            _: &[(usize, wmidi::MidiMessage)],
            _: [&[f32]; 2],
            output: [&mut [f32]; 2],
        ) {
            for channel in output {
                channel.fill(self.0);
            }
        }
    }
    let (sender, requests) = mpsc::channel();
    let (notifications, receiver) = mpsc::channel();
    let mut engine = Tinex::new(requests, notifications).unwrap();
    let mut state = AppState::default();
    for level in [0.25, 0.5] {
        let mut effect = update::update(&mut state, Action::Ui(UiAction::CreateTrack))
            .effects
            .remove(0);
        let TinexRequest::NewTrack(track) = &mut effect.request else {
            panic!("expected track");
        };
        track.push_plugin(PluginInstance::new(Box::new(Constant(level))));
        assert!(execute_effect(&sender, effect).is_none());
    }
    process(&mut engine);
    poll_notifications(&mut state, &receiver);
    let id = state.tracks[0].id;
    for (db, gain) in [
        (-61.0, 0.0),
        (-6.0, 10.0_f32.powf(-0.3)),
        (0.0, 1.0),
        (6.0, 10.0_f32.powf(0.3)),
        (12.0, 10.0_f32.powf(0.6)),
    ] {
        assert!(dispatch::dispatch_action(
            &mut state,
            Action::Ui(UiAction::SetTrackVolume { track_id: id, db }),
            &sender,
            &receiver
        ));
        let mut left = [0.0; 8];
        let mut right = [0.0; 8];
        let arena = bumpalo::Bump::new();
        engine.process(ProcessArgs {
            sample_rate: 48_000,
            input: [&[], &[]],
            midi_input: &[],
            output: [&mut left, &mut right],
            _arena: &arena,
        });
        for sample in left.into_iter().chain(right) {
            assert!((sample - (0.25 * gain + 0.5)).abs() < 1e-6);
        }
        assert_eq!(state.tracks[0].volume_db, db);
        assert_eq!(state.tracks[1].volume_db, 0.0);
        assert!(matches!(receiver.try_recv(), Err(TryRecvError::Empty)));
    }
    drop(engine);
    // Keep a notification sender alive to exercise the failed request send path.
    let (_notifications, receiver) = mpsc::channel();
    dispatch::dispatch_action(
        &mut state,
        Action::Ui(UiAction::SetTrackVolume {
            track_id: id,
            db: -12.0,
        }),
        &sender,
        &receiver,
    );
    assert!(!state.connected);
    assert_eq!(state.tracks[0].volume_db, 12.0);
}

#[test]
fn engine_round_trip_inserts_plugins_at_the_selected_positions() {
    let (sender, requests) = mpsc::channel();
    let (notifications, receiver) = mpsc::channel();
    let mut engine = Tinex::new(requests, notifications).unwrap();
    let mut state = AppState::default();
    for _ in 0..2 {
        send_actions(&mut state, &sender, UiAction::CreateTrack);
    }
    process(&mut engine);
    poll_notifications(&mut state, &receiver);
    let track_id = state.tracks[0].id;
    // Multiple additions may be queued while the confirmed GUI chain is empty.
    for (index, choice) in [(u16::MAX, 0), (u16::MAX, 4), (1, 0), (0, 4)] {
        send_actions(
            &mut state,
            &sender,
            UiAction::OpenPlugins { track_id, index },
        );
        send_actions(
            &mut state,
            &sender,
            UiAction::AddPlugin {
                track_id,
                index: choice,
            },
        );
        assert!(state.tracks[0].plugins.is_empty());
    }
    assert_eq!(state.tracks[0].pending_plugins.len(), 4);
    process(&mut engine);
    poll_notifications(&mut state, &receiver);
    assert_eq!(
        state.tracks[0]
            .plugins
            .iter()
            .map(|plugin| plugin.name)
            .collect::<Vec<_>>(),
        ["Delay", "EPiano", "EPiano", "Delay"]
    );
    assert!(state.tracks[0].pending_plugins.is_empty());
    assert!(state.tracks[1].plugins.is_empty());

    // If the chain shrinks while the picker is open, the engine appends and
    // confirms the resulting index so the GUI continues to match it.
    let plugin_id = state.tracks[0].plugins[0].id;
    send_actions(
        &mut state,
        &sender,
        UiAction::OpenPlugins { track_id, index: 4 },
    );
    send_actions(
        &mut state,
        &sender,
        UiAction::DeletePlugin {
            track_id,
            plugin_id,
        },
    );
    process(&mut engine);
    poll_notifications(&mut state, &receiver);
    send_actions(
        &mut state,
        &sender,
        UiAction::AddPlugin { track_id, index: 0 },
    );
    assert_eq!(state.tracks[0].plugins.len(), 3);
    process(&mut engine);
    poll_notifications(&mut state, &receiver);
    assert_eq!(
        state.tracks[0]
            .plugins
            .iter()
            .map(|plugin| plugin.name)
            .collect::<Vec<_>>(),
        ["EPiano", "EPiano", "Delay", "EPiano"]
    );
    assert_eq!(state.page, Page::Track(track_id));
}

#[test]
fn engine_round_trip_deletes_only_the_selected_plugin_instance() {
    let (sender, requests) = mpsc::channel();
    let (notifications, receiver) = mpsc::channel();
    let mut engine = Tinex::new(requests, notifications).unwrap();
    let mut state = AppState::default();
    for _ in 0..2 {
        send_actions(&mut state, &sender, UiAction::CreateTrack);
    }
    process(&mut engine);
    poll_notifications(&mut state, &receiver);
    let id = state.tracks[0].id;
    for track_id in [id, id, id, state.tracks[1].id] {
        send_actions(
            &mut state,
            &sender,
            UiAction::OpenPlugins {
                track_id,
                index: u16::MAX,
            },
        );
        send_actions(
            &mut state,
            &sender,
            UiAction::AddPlugin { track_id, index: 0 },
        );
    }
    process(&mut engine);
    poll_notifications(&mut state, &receiver);
    let mut expected = state.tracks[0].plugins.clone();
    let removed = expected.remove(1);
    let other = state.tracks[1].plugins.clone();
    send_actions(&mut state, &sender, UiAction::OpenTrack(id));
    send_actions(
        &mut state,
        &sender,
        UiAction::DeletePlugin {
            track_id: id,
            plugin_id: removed.id,
        },
    );
    assert_eq!(state.tracks[0].plugins.len(), 3);
    assert!(state.tracks[0].plugins[1].deleting);
    process(&mut engine);
    assert!(poll_notifications(&mut state, &receiver));
    assert_eq!(state.tracks[0].plugins, expected);
    assert_eq!(state.tracks[1].plugins, other);
    assert_eq!(state.page, Page::Track(id));
}
