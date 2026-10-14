# Tinex application guidance

## GUI architecture and file map

The GUI uses declarative widgets and an action → update → effect flow. Keep view construction free of engine I/O and domain-state mutation.

- `gui.rs`: winit event loop, input translation, redraw scheduling, and application wiring. Converts physical pointer coordinates to logical coordinates and supplies the JACK sample rate for plugin construction.
- `gui/state.rs`: application state, page selection, stable widget IDs, confirmed tracks/plugins, and pending-operation metadata.
- `gui/action.rs`: semantic, cloneable widget actions, engine operation tags, and request effects.
- `gui/update.rs`: state transitions and effect creation. Engine notifications settle pending operations here; engine-owned objects returned in notifications are disposed of on the GUI thread.
- `gui/dispatch.rs`: dispatches effects and polls engine notifications after widget traversal finishes. Settles queued notifications before new actions and handles failed sends/disconnection.
- `gui/meter_polling.rs`: clock-driven meter sampling, capped at 30 Hz with at most one request in flight. Hidden/inactive/occluded or disconnected views do not sample; missed deadlines do not create catch-up bursts.
- `gui/view.rs`: page routing, stale-track fallback, and the shared fixed status footer.
- `gui/view/main.rs`: track list, track creation, opening, and deletion controls.
- `gui/view/track.rs`: selected track's output meter, Add Plugin control for appending, and confirmed plugin chain with Insert Before controls.
- `gui/view/plugins.rs`: plugin picker for the selected track and Back to Track navigation. Choices come from `tinex_plugins::FACTORY`.
- `gui/theme.rs`: application styling.
- `gui/graphics.rs`: rendering infrastructure and surface lifecycle.
- `crates/tinex-widgets/src/framework.rs`: shared widget layout, rendering, focus, scrolling, and interaction. Meter rendering lives in `crates/tinex-widgets/src/level_meter.rs`; text rendering lives in `crates/tinex-widgets/src/text.rs`.

## Servicing UI requests

- For page-specific layout or labels, edit the relevant `gui/view/` module. Keep shared page routing/footer behavior in `gui/view.rs`; give new pages their own view module.
- For a new control, define a semantic `UiAction`, allocate stable IDs in state, and handle the action in `update.rs`. Views read state and attach actions; they do not send engine requests directly.
- For engine-backed changes, create an `Effect` in `update.rs` and use the existing dispatch path. Engine requests/notifications live in `crates/tinex-core/src/lib.rs`; track processing lives in `crates/tinex-core/src/track.rs`.
- Add shared widget capabilities in `tinex-widgets`, not duplicated page-specific input/layout code. Change `gui.rs` only when event-loop/input/application wiring needs to change.
- Plugin constructors and display names come from `crates/tinex-plugins/src/lib.rs`. Build plugins on the GUI thread using the current JACK sample rate, not in view construction or the audio callback.

## State and navigation invariants

- Reflect confirmed engine operations in track state. Pending creations must not appear as confirmed tracks; pending plugin additions must not appear as confirmed plugins.
- Plugin names are displayed in audio-chain order, including duplicate instances. Each add builds a `PluginInstance` whose `PluginInstanceId` keys that track's pending additions; `PluginAdded { track_id, plugin_id, index }` echoes the id and actual insertion index and moves the matching entry into the confirmed list at that position. Keep the id so later per-instance operations can address one plugin instead of a name. Ignore confirmations for unknown track or plugin ids; never pair a confirmation positionally with a queued request.
- Keep track identity separate from list position. Ignore stale IDs/invalid plugin selections; confirmed deletion of the selected track returns to the main page, including from its plugin picker.
- The plugin picker stores the requested chain index in `Page::Plugins`; `UiAction::AddPlugin.index` selects a factory entry. Selecting a plugin sends an add request at the stored chain index and returns to the track page. Out-of-range positions append. Back navigation is local and remains available after engine disconnection.
- Disable engine-backed controls when disconnected and prevent additions to tracks being deleted. Failed sends/disconnection must clear pending operations without inventing successful changes.

## Layout and interaction invariants

- Use logical coordinates for layout and hit testing; apply display scaling during rendering.
- Preserve stable widget IDs across view rebuilds to retain focus and scroll state. Allocate IDs in application/track state, never during view construction.
- Keep the status bar outside scrolling content on every page. Its background must be square-edged and flush with the bottom and both sides of the window.
- Use the compact meter in the 32-pixel status bar. Keep the larger meters in track content.
- Preserve nested-control precedence: clicking Delete must not open the track, including when Delete is disabled.
- Keep controls usable with both pointer and Tab/Enter/Space navigation; disabled controls must not emit actions.

## Tests and validation

- `gui/view/tests.rs`: declarative page structure, rendering, and pointer/keyboard interaction.
- `gui/update/tests.rs`: state transitions, navigation, and pending-operation handling.
- `gui/update/meter_tests.rs`: meter state, sanitization, and request-buffer reuse.
- `gui/tests.rs`: dispatch/engine round trips, failed sends, notification handling, and plugin confirmation/order.
- `gui/meter_polling.rs` and `gui/theme.rs`: inline tests for sampling schedules and styling.
- `crates/tinex-widgets/src/framework/tests.rs`: shared layout, focus, scrolling, and widget interaction.

For changes:

- Run `cargo test -p tinex gui::` for GUI changes.
- Run `cargo test -p tinex-widgets` for shared widget changes.
- Run `cargo test -p tinex-core` when engine requests, notifications, or track processing change.
- Add regression tests at the appropriate layer for layout, clipping, pointer/keyboard interaction, and state transitions.
- Test footer geometry with empty and overflowing content at multiple window sizes; scrolling must not move the footer. Include display scaling when changing geometry or hit testing.
- Test engine-backed controls before confirmation, after confirmation, and on send failure/disconnection. Verify operations affect only the intended track.
- Preserve unrelated working-tree changes; scope formatting and edits to the files involved in the request.
