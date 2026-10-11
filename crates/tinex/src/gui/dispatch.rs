//! GUI-thread coordination: finish action traversal before executing effects.
use std::collections::VecDeque;
use std::sync::mpsc::{Receiver, Sender, TryRecvError};

use tinex_core::{TinexNotification, TinexRequest};

use super::action::{Action, Effect};
use super::state::AppState;
use super::update::update;

pub(super) fn dispatch_action(
    state: &mut AppState,
    action: Action,
    requests: &Sender<TinexRequest>,
    notifications: &Receiver<TinexNotification>,
) -> bool {
    process_actions(state, Some(action), requests, notifications)
}

pub(super) fn poll_notifications(
    state: &mut AppState,
    notifications: &Receiver<TinexNotification>,
    requests: &Sender<TinexRequest>,
) -> bool {
    process_actions(state, None, requests, notifications)
}

fn process_actions(
    state: &mut AppState,
    initial: Option<Action>,
    requests: &Sender<TinexRequest>,
    notifications: &Receiver<TinexNotification>,
) -> bool {
    let mut actions = VecDeque::new();
    let mut effects = VecDeque::new();
    // Settle existing responses before sampling or processing a new UI action.
    drain_notifications(notifications, &mut actions);
    actions.extend(initial);
    let mut changed = false;
    loop {
        while let Some(action) = actions.pop_front() {
            let result = update(state, action);
            changed |= result.changed;
            // Notification-produced effects use exactly the same path as UI effects.
            effects.extend(result.effects);
        }
        let Some(effect) = effects.pop_front() else {
            break;
        };
        if !state.connected {
            // Disconnect settled pending operations; drop request-owned tracks here.
            continue;
        }
        if let Some(failure) = execute_effect(requests, effect) {
            // Apply any last confirmations before clearing pending metadata.
            drain_notifications(notifications, &mut actions);
            actions.push_back(failure);
        }
    }
    changed
}

fn drain_notifications(
    notifications: &Receiver<TinexNotification>,
    actions: &mut VecDeque<Action>,
) {
    loop {
        match notifications.try_recv() {
            Ok(notification) => actions.push_back(Action::Notification(notification)),
            Err(TryRecvError::Empty) => break,
            Err(TryRecvError::Disconnected) => {
                actions.push_back(Action::Disconnected);
                break;
            }
        }
    }
}

pub(super) fn execute_effect(sender: &Sender<TinexRequest>, effect: Effect) -> Option<Action> {
    if sender.send(effect.request).is_err() {
        // Failed sends dispose of request-owned tracks/maps on this GUI thread.
        Some(Action::RequestFailed(effect.operation))
    } else {
        None
    }
}
