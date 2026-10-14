# Tinex application guidance

## GUI architecture and file map

The GUI uses **Xilem 0.4** with its Masonry backend and an action → update → effect flow. Keep declarative view construction free of engine I/O and domain-state mutation. Xilem/Masonry own layout, input, focus, scrolling, text, accessibility, rendering, and surface lifecycle; do not reintroduce a parallel widget framework.

- `gui.rs`: Xilem startup, application/controller wiring, and lifecycle-bound asynchronous wakeups. The thin winit `Lifecycle` wrapper only observes suspension, occlusion, and zero-sized windows; every event is forwarded to Masonry. The Tokio task sends acknowledged wakeups only; it never touches JACK or engine-owned objects. Unchanged polls return `MessageResult::Nop` so they do not rebuild or redraw.
- `gui/state.rs`: application state, page selection, confirmed tracks/plugins, and pending-operation metadata. Domain IDs, not list positions or separately allocated widget IDs, identify dynamic controls.
- `gui/action.rs`: semantic, cloneable widget actions, engine operation tags, and request effects.
- `gui/update.rs`: state transitions and effect creation. Engine notifications settle pending operations here; engine-owned objects returned in notifications are disposed of on the GUI thread.
- `gui/dispatch.rs`: dispatches effects and polls engine notifications after widget traversal finishes. Settles queued notifications before new actions and handles failed sends/disconnection.
- `gui/meter_polling.rs`: clock-driven meter sampling, capped at 30 Hz with at most one request in flight. Hidden/inactive/occluded or disconnected views do not sample; missed deadlines do not create catch-up bursts.
- `gui/cpu_polling.rs`: visible/connected JACK CPU sampling, immediately on eligibility and then every five seconds.
- `gui/view.rs`: Xilem page routing, stale-track fallback, shared controls, and the fixed status footer.
- `gui/view/main.rs`: track list, creation, explicit track-opening buttons, and deletion/arm/volume controls.
- `gui/view/track.rs`: selected track's volume/output meter, Add Plugin control, and confirmed plugin chain with Delete and Insert Before controls.
- `gui/view/plugins.rs`: plugin picker for the selected track and Back to Track navigation. Choices come from `tinex_plugins::FACTORY`.
- `gui/theme.rs`: application colors and Masonry default properties. Standard control focus/interaction visuals come from Masonry.
- `crates/tinex-widgets/src/meter.rs`: noninteractive Masonry meter widgets and Xilem view adapters. CPU readout text is a native Xilem label composed over a percentage bar.
- `crates/tinex-widgets/src/level_meter.rs`: pure amplitude/dBFS conversion, sanitization, geometry, and custom meter painting using the backend's Vello re-export.
- `crates/tinex-widgets/src/keyed.rs`: keyed single-element flex sequence adapter for Xilem 0.4, whose native `Vec` sequence is positional. Insertions/deletions retain surviving identities; true moves are rebuilt with new generations because Masonry has no child-reordering API. Keys must be unique.
- `crates/tinex-widgets/src/fonts.rs`: bundled Noto fonts and the shared font stack. Fonts are registered with Xilem, not an independent text renderer; system sans-serif is the fallback for math symbols absent from the bundled subset.

## Servicing UI requests

- For page-specific layout or labels, edit the relevant `gui/view/` module. Keep shared routing/footer behavior in `gui/view.rs`; give new pages their own view module.
- For a new control, define a semantic `UiAction` and handle it in `update.rs`. Views read state and return actions; the application boundary maps them into the dispatch path. They do not send engine requests directly.
- For engine-backed changes, create an `Effect` in `update.rs` and use the existing dispatch path. Engine requests/notifications live in `crates/tinex-core/src/lib.rs`; track processing lives in `crates/tinex-core/src/track.rs`.
- Prefer standard Xilem controls and composition. Put unsupported application-specific widgets in `tinex-widgets` as Masonry widgets/Xilem views, not duplicate page-specific input/layout code.
- Plugin constructors and display names come from `crates/tinex-plugins/src/lib.rs`. Build plugins on the GUI thread using the current JACK sample rate, not in view construction or the audio callback.
- Keep scheduling and request/notification ownership on the GUI side. Wakeups are bounded to one pending tick and wait for acknowledgement; deadlines must not produce catch-up bursts or hidden-window busy loops. Notifications continue to settle while hidden.

## State and navigation invariants

- Reflect confirmed engine operations in track state. Pending creations must not appear as confirmed tracks; pending plugin additions must not appear as confirmed plugins.
- Plugin names are displayed in audio-chain order, including duplicates. Each add builds a `PluginInstance` whose `PluginInstanceId` keys that track's pending additions; `PluginAdded { track_id, plugin_id, index }` echoes the id and actual insertion index and moves the matching entry into the confirmed list at that position. Never pair a confirmation positionally with a queued request.
- Delete plugins by instance ID, retaining confirmed entries until the engine returns the instance. Dispose of returned instances on the GUI thread. Ignore unknown confirmations and reset pending/deleting flags on failure/disconnection without inventing success.
- Keep track identity separate from list position. Ignore stale IDs/invalid plugin selections; confirmed deletion of the selected track returns to the main page, including from its plugin picker.
- The plugin picker stores the requested chain index in `Page::Plugins`; `UiAction::AddPlugin.index` selects a factory entry. Selecting a plugin sends an add request at the stored chain index and returns to the track page. Out-of-range positions append. Back navigation is local and remains available after engine disconnection.
- Disable engine-backed controls when disconnected and prevent additions to tracks being deleted. Failed sends/disconnection must clear pending operations without inventing successful changes.
- Volume spans −61 dB (silence) to +12 dB in 1 dB steps. Engine metering is pre-gain; present estimated post-gain level, keeping silence silent even for infinite pre-gain readings. Do not clamp positive audio overloads in state.
- Arming reflects the confirmed engine set; serialize replacement requests across all tracks.

## Layout and interaction invariants

- Use logical coordinates; let Masonry handle display scaling and hit testing.
- Use `keyed` with `TrackId` / `PluginInstanceId` for dynamic collections so insertion/deletion does not redirect focus or actions to another item. Focus and scroll must survive ordinary state/meter rebuilds.
- Keep the shared status bar outside scrolling content on every page. Its background must be square-edged, 32 logical pixels tall, and flush with the bottom and both window sides. Compact master/CPU meters may shrink at small window sizes.
- Keep Delete, Arm, and volume interactions separate from track navigation, including disabled Delete. The track name is an explicit opening button; do not wrap interactive controls inside a clickable ancestor.
- Keep controls usable with pointer and Tab/Shift-Tab/Enter/Space navigation. Sliders support arrows/Home/End. Disabled controls must not emit actions.

## Tests and validation

- `gui/view/tests.rs`: headless Xilem/Masonry page layout, semantic actions, keyboard/pointer interaction, keyed identity, stale messages, clipping/scrolling, and footer geometry.
- `gui/view/testing.rs`: test-only transparent routing probes and a headless session. Xilem 0.4's `ViewCtx` constructor/action map are private, so contexts are obtained through its public builder. Test-only `Trace` wrappers do not change production behavior.
- `gui/update/tests.rs`: state transitions, navigation, and pending-operation handling.
- `gui/update/meter_tests.rs`: meter state, sanitization, and request-buffer reuse.
- `gui/tests.rs`: dispatch/engine round trips, failed sends, thread ownership, plugin ordering/deletion, and acknowledged wakeup queueing.
- `gui/meter_polling.rs`, `gui/cpu_polling.rs`, and `gui/theme.rs`: inline schedule and styling tests.
- `tinex-widgets`: pure meter geometry/conversion, native accessibility/noninteraction, painting/scaling, and bundled-font shaping tests.

For changes:

- Run `cargo test -p tinex gui::` for GUI changes and `cargo test -p tinex-widgets` for shared widget changes.
- Run `cargo test -p tinex-core` when engine requests, notifications, or processing change; run `cargo test --workspace` and `cargo check --workspace --all-targets` for migration-wide changes.
- Add regression tests at the appropriate layer for layout, clipping, pointer/keyboard interaction, and domain transitions.
- Test footer geometry with empty and overflowing content on every page at multiple sizes/scales; scrolling must not move the footer.
- Test engine-backed controls before/after confirmation and on send failure/disconnection; verify they affect only the intended track/instance.
- When a desktop and JACK server are available, smoke-test live meters/CPU, all navigation/controls, resize/HiDPI, minimize/restore, disconnect, and close.
- Preserve unrelated working-tree changes; scope formatting and edits to files involved in the request.
