use super::testing::Session;
use super::*;
use crate::gui::{action::Action, state::UiPluginInstance, update::update};
use masonry::{
    core::{
        TextEvent, WidgetId,
        keyboard::{Key, NamedKey},
    },
    kurbo::{Rect, Size, Vec2},
    widgets::{Flex, Portal, SizedBox, Slider},
};
use tinex_core::{TinexNotification, TinexRequest, plugin::PluginInstanceId, track::TrackId};

fn tracks(count: usize) -> AppState {
    let mut state = AppState::default();
    for _ in 0..count {
        let effect = update(&mut state, Action::Ui(UiAction::CreateTrack))
            .effects
            .remove(0);
        let TinexRequest::NewTrack(track) = effect.request else {
            unreachable!()
        };
        update(
            &mut state,
            Action::Notification(TinexNotification::TrackCreated(track.id())),
        );
    }
    state
}

fn plugin(name: &'static str) -> UiPluginInstance {
    UiPluginInstance {
        id: PluginInstanceId::new(),
        name,
        deleting: false,
    }
}

fn rect<V: WidgetView<AppState, UiAction>>(ui: &Session<V>, id: WidgetId) -> Rect {
    ui.harness.get_widget_with_id(id).ctx().bounding_rect()
}

fn scroll_control_into_view<V: WidgetView<AppState, UiAction>>(
    ui: &mut Session<V>,
    id: WidgetId,
    scale: f64,
) {
    let portal = ui.widgets::<Portal<Flex>>()[0];
    let viewport = rect(ui, portal);
    let center = viewport.center();
    ui.harness.mouse_move((center.x * scale, center.y * scale));
    for _ in 0..50 {
        let bounds = rect(ui, id);
        if viewport.contains_rect(bounds) {
            return;
        }
        // Small wheel steps avoid skipping an entire short viewport at HiDPI.
        let delta = if bounds.y0 < viewport.y0 { 24.0 } else { -24.0 };
        ui.harness.mouse_wheel(Vec2::new(0.0, delta * scale));
    }
    panic!(
        "control {:?} never entered viewport {viewport:?}",
        rect(ui, id)
    );
}

#[test]
fn create_and_track_controls_emit_semantic_actions_without_mutating_state() {
    let state = tracks(1);
    let id = state.tracks[0].id;
    let session = Session::new(state, view, Size::new(800.0, 450.0), 1.0);
    let mut ui = session.borrow_mut();
    for (text, expected) in [
        ("Create Track", UiAction::CreateTrack),
        ("Track 1", UiAction::OpenTrack(id)),
        ("Delete", UiAction::DeleteTrack(id)),
        ("Arm", UiAction::ToggleTrackArm(id)),
    ] {
        let button = ui.button(text);
        ui.harness.mouse_click_on(button);
        assert_eq!(ui.action(), Some(expected));
        assert!(
            ui.action().is_none(),
            "nested control must emit only one action"
        );
    }
    assert_eq!(ui.state.lock().unwrap().page, Page::Main);
    assert!(!ui.state.lock().unwrap().tracks[0].deleting);
}

#[test]
fn disabled_nested_controls_never_open_a_track_and_navigation_remains_local() {
    let mut state = tracks(1);
    state.connected = false;
    let id = state.tracks[0].id;
    let session = Session::new(state, view, Size::new(800.0, 450.0), 1.0);
    let mut ui = session.borrow_mut();
    for text in ["Create Track", "Delete", "Arm"] {
        let button = ui.button(text);
        assert!(ui.harness.get_widget_with_id(button).ctx().is_disabled());
        ui.harness.mouse_click_on(button);
        assert!(ui.action().is_none());
    }
    let open = ui.button("Track 1");
    ui.harness.mouse_click_on(open);
    assert_eq!(ui.action(), Some(UiAction::OpenTrack(id)));
    assert!(ui.action().is_none());
}

#[test]
fn physical_pointer_events_activate_the_same_controls_at_multiple_display_scales() {
    for scale in [1.0, 1.5, 2.0] {
        let session = Session::new(
            tracks(1),
            view,
            Size::new(800.0 * scale, 450.0 * scale),
            scale,
        );
        let mut ui = session.borrow_mut();
        let create = ui.button("Create Track");
        let center = rect(&ui, create).center();
        // The 0.4 harness's convenience mouse_click_on does not apply scale;
        // send physical coordinates as the native backend would.
        ui.harness.mouse_move((center.x * scale, center.y * scale));
        ui.harness
            .mouse_button_press(masonry::core::PointerButton::Primary);
        ui.harness
            .mouse_button_release(masonry::core::PointerButton::Primary);
        assert_eq!(ui.action(), Some(UiAction::CreateTrack));
        assert!(ui.action().is_none());
    }
}

#[test]
fn tab_shift_tab_enter_and_space_activate_controls() {
    let session = Session::new(tracks(1), view, Size::new(800.0, 450.0), 1.0);
    let mut ui = session.borrow_mut();
    let create = ui.button("Create Track");
    ui.harness.focus_on(None);
    ui.harness.press_tab_key(false);
    assert_eq!(ui.harness.focused_widget_id(), Some(create));
    for key in [Key::Named(NamedKey::Enter), Key::Character(" ".into())] {
        ui.harness
            .process_text_event(TextEvent::key_down(key.clone()));
        ui.harness.process_text_event(TextEvent::key_up(key));
        assert_eq!(ui.action(), Some(UiAction::CreateTrack));
    }
    ui.harness.press_tab_key(false);
    let track = ui.button("Track 1");
    assert_eq!(ui.harness.focused_widget_id(), Some(track));
    ui.harness.press_tab_key(true);
    assert_eq!(ui.harness.focused_widget_id(), Some(create));
}

#[test]
fn slider_supports_arrows_home_end_pointer_and_disabled_behavior() {
    let state = tracks(1);
    let id = state.tracks[0].id;
    let session = Session::new(state, view, Size::new(800.0, 450.0), 1.0);
    {
        let mut ui = session.borrow_mut();
        let slider = ui.widgets::<Slider>()[0];
        ui.harness.focus_on(Some(slider));
        for (key, db) in [
            (NamedKey::ArrowLeft, -1.0),
            (NamedKey::ArrowRight, 0.0),
            (NamedKey::Home, -61.0),
            (NamedKey::End, 12.0),
        ] {
            ui.harness
                .process_text_event(TextEvent::key_down(Key::Named(key)));
            assert_eq!(
                ui.action(),
                Some(UiAction::SetTrackVolume { track_id: id, db })
            );
        }
        ui.harness.mouse_click_on(slider);
        assert!(
            matches!(ui.action(), Some(UiAction::SetTrackVolume { track_id, db }) if track_id == id && db.is_finite() && (-61.0..=12.0).contains(&db))
        );
        assert!(ui.action().is_none());
        ui.state.lock().unwrap().tracks[0].deleting = true;
    }
    Session::refresh(&session, view);
    let mut ui = session.borrow_mut();
    let slider = ui.widgets::<Slider>()[0];
    ui.harness.mouse_click_on(slider);
    assert!(ui.action().is_none());
}

#[test]
fn plugin_pages_show_confirmation_order_and_per_instance_delete_and_insert_actions() {
    let mut state = tracks(1);
    let id = state.tracks[0].id;
    state.page = Page::Track(id);
    state.tracks[0].plugins = vec![plugin("EPiano"), plugin("Delay"), plugin("EPiano")];
    let first = state.tracks[0].plugins[0].id;
    let session = Session::new(state, view, Size::new(800.0, 450.0), 1.0);
    let mut ui = session.borrow_mut();
    let names: Vec<_> = ui
        .labels()
        .into_iter()
        .filter(|s| s == "EPiano" || s == "Delay")
        .collect();
    assert_eq!(names, ["EPiano", "Delay", "EPiano"]);
    let delete = ui.button("Delete");
    ui.harness.mouse_click_on(delete);
    assert_eq!(
        ui.action(),
        Some(UiAction::DeletePlugin {
            track_id: id,
            plugin_id: first
        })
    );
    let insert = ui.button("Insert Before");
    ui.harness.mouse_click_on(insert);
    assert_eq!(
        ui.action(),
        Some(UiAction::OpenPlugins {
            track_id: id,
            index: 0
        })
    );
    let add = ui.button("Add Plugin");
    ui.harness.mouse_click_on(add);
    assert_eq!(
        ui.action(),
        Some(UiAction::OpenPlugins {
            track_id: id,
            index: u16::MAX
        })
    );
    ui.state.lock().unwrap().page = Page::Plugins(id, 0);
    drop(ui);
    Session::refresh(&session, view);
    let mut ui = session.borrow_mut();
    assert!(ui.labels().iter().any(|s| s.starts_with("Insert Plugin")));
    for (index, factory) in tinex_plugins::FACTORY.iter().enumerate() {
        let choice = ui.button(factory.name());
        scroll_control_into_view(&mut ui, choice, 1.0);
        ui.harness.mouse_click_on(choice);
        assert_eq!(
            ui.action(),
            Some(UiAction::AddPlugin {
                track_id: id,
                index
            })
        );
    }
    ui.state.lock().unwrap().connected = false;
    drop(ui);
    Session::refresh(&session, view);
    let mut ui = session.borrow_mut();
    let choice = ui.button(tinex_plugins::FACTORY[0].name());
    scroll_control_into_view(&mut ui, choice, 1.0);
    ui.harness.mouse_click_on(choice);
    assert!(ui.action().is_none());
    let back = ui.button("Back to Track");
    ui.harness.mouse_click_on(back);
    assert_eq!(ui.action(), Some(UiAction::OpenTrack(id)));
}

#[test]
fn plugin_picker_describes_the_confirmed_insertion_or_append_destination() {
    for (index, title, destination) in [
        (0, "Insert Plugin", "Before 01 · EPiano"),
        (1, "Insert Plugin", "Before 02 · Delay"),
        (2, "Insert Plugin", "Before 03 · EPiano"),
        (3, "Add Plugin", "At the end of the chain · position 04"),
        (99, "Add Plugin", "At the end of the chain · position 04"),
        (
            u16::MAX,
            "Add Plugin",
            "At the end of the chain · position 04",
        ),
    ] {
        let mut state = tracks(1);
        state.page = Page::Plugins(state.tracks[0].id, index);
        state.tracks[0].plugins = vec![plugin("EPiano"), plugin("Delay"), plugin("EPiano")];
        let pending = plugin("Unconfirmed plugin");
        state.tracks[0].pending_plugins.insert(pending.id, pending);
        let session = Session::new(state, view, Size::new(400.0, 450.0), 1.0);
        let mut ui = session.borrow_mut();
        let labels = ui.labels();
        assert!(labels.contains(&title.into()));
        assert!(labels.contains(&destination.into()));
        assert!(labels.contains(&"Track · Track 1".into()));
        assert!(labels.contains(&"Instruments · 2".into()));
        assert!(labels.contains(&"Effects · 4".into()));
        assert!(
            !labels
                .iter()
                .any(|text| text.contains("Unconfirmed plugin"))
        );
        for factory in &tinex_plugins::FACTORY {
            assert!(labels.contains(&factory.description().into()));
        }
    }
    let mut state = tracks(1);
    state.page = Page::Plugins(state.tracks[0].id, u16::MAX);
    let session = Session::new(state, view, Size::new(400.0, 450.0), 1.0);
    assert!(
        session
            .borrow_mut()
            .labels()
            .contains(&"At the end of the chain · position 01".into())
    );
}

#[test]
fn plugin_picker_choices_fit_narrow_windows_and_keep_factory_indices_at_every_scale() {
    for scale in [1.0, 2.0] {
        for (width, height) in [(400.0, 280.0), (800.0, 450.0)] {
            let mut state = tracks(1);
            let track_id = state.tracks[0].id;
            state.page = Page::Plugins(track_id, 0);
            state.tracks[0].name =
                "A very long track name that should not move picker controls off screen".into();
            state.tracks[0].plugins = vec![plugin(
                "A very long plugin name that should not resize the destination header",
            )];
            let session =
                Session::new(state, view, Size::new(width * scale, height * scale), scale);
            let (portal, offset, back, back_bounds, last_choice) = {
                let mut ui = session.borrow_mut();
                let back = ui.button("Back to Track");
                let back_bounds = rect(&ui, back);
                assert!(Rect::new(0.0, 0.0, width, height - 32.0).contains_rect(back_bounds));
                let portal = ui.widgets::<Portal<Flex>>()[0];
                let viewport = rect(&ui, portal);
                ui.harness.focus_on(Some(back));
                let mut last_choice = back;
                for (index, factory) in tinex_plugins::FACTORY.iter().enumerate() {
                    let choice = ui.button(factory.name());
                    ui.harness.press_tab_key(false);
                    assert_eq!(ui.harness.focused_widget_id(), Some(choice));
                    scroll_control_into_view(&mut ui, choice, scale);
                    let bounds = rect(&ui, choice);
                    assert!(
                        viewport.contains_rect(bounds),
                        "{}: {bounds:?} in {viewport:?}",
                        factory.name()
                    );
                    assert_eq!(bounds.width(), 132.0);
                    assert_eq!(bounds.height(), 32.0);
                    assert_eq!(rect(&ui, back), back_bounds);
                    let center = bounds.center();
                    ui.harness.mouse_move((center.x * scale, center.y * scale));
                    ui.harness
                        .mouse_button_press(masonry::core::PointerButton::Primary);
                    ui.harness
                        .mouse_button_release(masonry::core::PointerButton::Primary);
                    assert_eq!(ui.action(), Some(UiAction::AddPlugin { track_id, index }));
                    assert!(ui.action().is_none());
                    for key in [Key::Named(NamedKey::Enter), Key::Character(" ".into())] {
                        ui.harness
                            .process_text_event(TextEvent::key_down(key.clone()));
                        ui.harness.process_text_event(TextEvent::key_up(key));
                        assert_eq!(ui.action(), Some(UiAction::AddPlugin { track_id, index }));
                        assert!(ui.action().is_none());
                    }
                    last_choice = choice;
                }
                assert_eq!(ui.state.lock().unwrap().page, Page::Plugins(track_id, 0));
                assert_eq!(ui.state.lock().unwrap().tracks[0].plugins.len(), 1);
                assert!(
                    ui.state.lock().unwrap().tracks[0]
                        .pending_plugins
                        .is_empty()
                );
                let offset = ui
                    .harness
                    .get_widget_with_id(portal)
                    .downcast::<Portal<Flex>>()
                    .unwrap()
                    .inner()
                    .get_viewport_pos();
                assert!(offset.y > 0.0);
                ui.state.lock().unwrap().master_output_level = 2.0;
                ui.state.lock().unwrap().cpu_load = Some(42.5);
                (portal, offset, back, back_bounds, last_choice)
            };
            Session::refresh(&session, view);
            let mut ui = session.borrow_mut();
            assert_eq!(ui.button("Back to Track"), back);
            assert_eq!(rect(&ui, back), back_bounds);
            assert_eq!(ui.harness.focused_widget_id(), Some(last_choice));
            assert_eq!(
                ui.harness
                    .get_widget_with_id(portal)
                    .downcast::<Portal<Flex>>()
                    .unwrap()
                    .inner()
                    .get_viewport_pos(),
                offset
            );
            ui.harness.press_tab_key(true);
            assert_eq!(ui.harness.focused_widget_id(), Some(ui.button("Delay")));
        }
    }
}

#[test]
fn plugin_picker_disables_all_choices_but_keeps_back_navigation_when_unavailable() {
    for deleting in [false, true] {
        let mut state = tracks(1);
        let track_id = state.tracks[0].id;
        state.page = Page::Plugins(track_id, u16::MAX);
        state.connected = deleting;
        state.tracks[0].deleting = deleting;
        let session = Session::new(state, view, Size::new(400.0, 280.0), 1.0);
        let mut ui = session.borrow_mut();
        let message = if deleting {
            "This track is being deleted. Adding plugins is unavailable."
        } else {
            "Audio engine disconnected. Adding plugins is unavailable."
        };
        assert!(ui.labels().contains(&message.into()));
        for factory in &tinex_plugins::FACTORY {
            let choice = ui.button(factory.name());
            assert!(ui.harness.get_widget_with_id(choice).ctx().is_disabled());
            scroll_control_into_view(&mut ui, choice, 1.0);
            ui.harness.mouse_click_on(choice);
            assert!(ui.action().is_none());
        }
        let back = ui.button("Back to Track");
        assert!(!ui.harness.get_widget_with_id(back).ctx().is_disabled());
        ui.harness.mouse_click_on(back);
        assert_eq!(ui.action(), Some(UiAction::OpenTrack(track_id)));
        ui.harness.focus_on(Some(back));
        ui.harness
            .process_text_event(TextEvent::key_down(Key::Named(NamedKey::Enter)));
        ui.harness
            .process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));
        assert_eq!(ui.action(), Some(UiAction::OpenTrack(track_id)));
    }
}

#[test]
fn track_page_explains_empty_pending_and_disconnected_chains() {
    let mut state = tracks(1);
    let id = state.tracks[0].id;
    state.page = Page::Track(id);
    let session = Session::new(state, view, Size::new(400.0, 450.0), 1.0);
    {
        let mut ui = session.borrow_mut();
        let labels = ui.labels();
        assert!(labels.contains(&"Plugin chain · 0".into()));
        assert!(labels.contains(&"No plugins yet".into()));
        assert!(
            labels.contains(&"Add an instrument or effect to start building this track.".into())
        );
        let pending = plugin("Unconfirmed plugin");
        ui.state.lock().unwrap().tracks[0]
            .pending_plugins
            .insert(pending.id, pending);
    }
    Session::refresh(&session, view);
    {
        let mut ui = session.borrow_mut();
        let labels = ui.labels();
        assert!(labels.contains(&"Plugin chain · 0".into()));
        assert!(labels.contains(&"Adding plugin…".into()));
        assert!(!labels.contains(&"Unconfirmed plugin".into()));
        assert!(!labels.contains(&"01".into()));
        // Confirmed duplicates remain separate, numbered chain entries.
        ui.state.lock().unwrap().tracks[0].plugins = vec![plugin("EPiano"), plugin("EPiano")];
        update(&mut ui.state.lock().unwrap(), Action::Disconnected);
    }
    Session::refresh(&session, view);
    let mut ui = session.borrow_mut();
    let labels = ui.labels();
    assert!(labels.contains(&"Plugin chain · 2".into()));
    assert!(labels.contains(&"01".into()));
    assert!(labels.contains(&"02".into()));
    assert_eq!(labels.iter().filter(|text| *text == "EPiano").count(), 2);
    assert!(!labels.contains(&"Adding plugin…".into()));
    assert!(labels.contains(&"Audio engine disconnected. Track controls are unavailable.".into()));
    for text in ["Add Plugin", "Arm", "Insert Before", "Delete"] {
        let button = ui.button(text);
        assert!(ui.harness.get_widget_with_id(button).ctx().is_disabled());
        ui.harness.mouse_click_on(button);
        assert!(ui.action().is_none());
    }
    let slider = ui.widgets::<Slider>()[0];
    assert!(ui.harness.get_widget_with_id(slider).ctx().is_disabled());
    let back = ui.button("Back to Tracks");
    ui.harness.focus_on(Some(back));
    ui.harness
        .process_text_event(TextEvent::key_down(Key::Named(NamedKey::Enter)));
    ui.harness
        .process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));
    assert_eq!(ui.action(), Some(UiAction::BackToMain));
    {
        let mut state = ui.state.lock().unwrap();
        state.connected = true;
        state.tracks[0].deleting = true;
    }
    drop(ui);
    Session::refresh(&session, view);
    let mut ui = session.borrow_mut();
    assert!(ui.labels().contains(&"Deleting track…".into()));
    for text in ["Add Plugin", "Arm", "Insert Before", "Delete"] {
        let button = ui.button(text);
        assert!(ui.harness.get_widget_with_id(button).ctx().is_disabled());
        ui.harness.mouse_click_on(button);
        assert!(ui.action().is_none());
    }
    let slider = ui.widgets::<Slider>()[0];
    assert!(ui.harness.get_widget_with_id(slider).ctx().is_disabled());
    let back = ui.button("Back to Tracks");
    ui.harness.mouse_click_on(back);
    assert_eq!(ui.action(), Some(UiAction::BackToMain));
}

#[test]
fn track_chain_controls_fit_long_names_and_do_not_shift_during_deletion() {
    for scale in [1.0, 2.0] {
        for (width, height) in [(400.0, 280.0), (400.0, 450.0), (800.0, 450.0)] {
            let mut state = tracks(1);
            state.page = Page::Track(state.tracks[0].id);
            state.tracks[0].name =
                "A very long track name that should not displace any controls".into();
            state.tracks[0].plugins = vec![
                plugin("A very long plugin name that must stay inside its chain card"),
                plugin("Delay"),
            ];
            let session =
                Session::new(state, view, Size::new(width * scale, height * scale), scale);
            let (before, name_bounds) = {
                let mut ui = session.borrow_mut();
                let portal = ui.widgets::<Portal<Flex>>()[0];
                let viewport = rect(&ui, portal);
                let window = Rect::new(0.0, 0.0, width, height - 32.0);
                let mut before = Vec::new();
                for text in [
                    "Back to Tracks",
                    "Arm",
                    "Add Plugin",
                    "Insert Before",
                    "Delete",
                ] {
                    let id = ui.button(text);
                    let bounds = rect(&ui, id);
                    assert!(
                        window.contains_rect(bounds),
                        "{text}: {bounds:?} in {window:?}"
                    );
                    if matches!(text, "Insert Before" | "Delete") {
                        assert!(
                            viewport.contains_rect(bounds),
                            "{text}: {bounds:?} in {viewport:?}"
                        );
                    }
                    before.push((id, bounds));
                }
                let slider = ui.widgets::<Slider>()[0];
                let bounds = rect(&ui, slider);
                assert!(window.contains_rect(bounds));
                assert!(bounds.width() >= 160.0, "usable slider: {bounds:?}");
                before.push((slider, bounds));
                let name = ui
                    .widgets::<masonry::widgets::Label>()
                    .into_iter()
                    .find(|id| {
                        ui.harness
                            .get_widget_with_id(*id)
                            .downcast::<masonry::widgets::Label>()
                            .unwrap()
                            .inner()
                            .text()
                            .starts_with("A very long plugin")
                    })
                    .unwrap();
                let name_bounds = rect(&ui, name);
                assert!(viewport.contains_rect(name_bounds));
                assert!(name_bounds.width() >= 80.0);
                assert!(name_bounds.x1 <= before[3].1.x0);
                ui.state.lock().unwrap().tracks[0].plugins[0].deleting = true;
                (before, (name, name_bounds))
            };
            Session::refresh(&session, view);
            let mut ui = session.borrow_mut();
            assert_eq!(ui.button("Deleting…"), before[4].0);
            assert!(
                ui.harness
                    .get_widget_with_id(before[4].0)
                    .ctx()
                    .is_disabled()
            );
            for (id, bounds) in before {
                assert_eq!(rect(&ui, id), bounds, "deletion must not shift controls");
            }
            assert_eq!(rect(&ui, name_bounds.0), name_bounds.1);
            assert!(ui.labels().contains(&"Plugin chain · 2".into()));
        }
    }
}

#[test]
fn track_controls_stay_fixed_and_chain_focus_survives_scrolling_and_meter_updates() {
    for scale in [1.0, 2.0] {
        let mut state = tracks(1);
        let track_id = state.tracks[0].id;
        state.page = Page::Track(track_id);
        state.tracks[0].plugins = (0..12).map(|_| plugin("EPiano")).collect();
        let session = Session::new(state, view, Size::new(400.0 * scale, 280.0 * scale), scale);
        let (portal, offset, fixed, insert) = {
            let mut ui = session.borrow_mut();
            let mut fixed = Vec::new();
            for text in ["Back to Tracks", "Arm", "Add Plugin"] {
                let id = ui.button(text);
                fixed.push((id, rect(&ui, id)));
            }
            let slider = ui.widgets::<Slider>()[0];
            let meter = ui.widgets::<tinex_widgets::MeterWidget>()[0];
            fixed.extend([(slider, rect(&ui, slider)), (meter, rect(&ui, meter))]);
            let insert = ui.button("Insert Before");
            let delete = ui.button("Delete");
            ui.harness.focus_on(None);
            for id in [fixed[0].0, fixed[1].0, slider, fixed[2].0, insert, delete] {
                ui.harness.press_tab_key(false);
                assert_eq!(ui.harness.focused_widget_id(), Some(id));
            }
            ui.harness.press_tab_key(true);
            assert_eq!(ui.harness.focused_widget_id(), Some(insert));
            let portal = ui.widgets::<Portal<Flex>>()[0];
            let center = rect(&ui, portal).center();
            ui.harness.mouse_move((center.x * scale, center.y * scale));
            ui.harness.mouse_wheel(Vec2::new(0.0, -120.0 * scale));
            let offset = ui
                .harness
                .get_widget_with_id(portal)
                .downcast::<Portal<Flex>>()
                .unwrap()
                .inner()
                .get_viewport_pos();
            assert!(offset.y > 0.0);
            for (id, bounds) in &fixed {
                assert_eq!(rect(&ui, *id), *bounds);
            }
            ui.state.lock().unwrap().tracks[0].output_level = 1.5;
            ui.state.lock().unwrap().master_output_level = 2.0;
            ui.state.lock().unwrap().cpu_load = Some(42.5);
            (portal, offset, fixed, insert)
        };
        Session::refresh(&session, view);
        let mut ui = session.borrow_mut();
        assert_eq!(
            ui.harness
                .get_widget_with_id(portal)
                .downcast::<Portal<Flex>>()
                .unwrap()
                .inner()
                .get_viewport_pos(),
            offset
        );
        assert_eq!(ui.harness.focused_widget_id(), Some(insert));
        assert_eq!(ui.button("Insert Before"), insert);
        for (id, bounds) in fixed {
            assert_eq!(rect(&ui, id), bounds);
        }
        ui.harness
            .process_text_event(TextEvent::key_down(Key::Named(NamedKey::Enter)));
        ui.harness
            .process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));
        assert_eq!(
            ui.action(),
            Some(UiAction::OpenPlugins { track_id, index: 0 })
        );
    }
}

#[test]
fn keyed_tracks_keep_focus_ids_and_routes_when_preceding_tracks_are_deleted() {
    let state = tracks(3);
    let survivor = state.tracks[1].id;
    let session = Session::new(state, view, Size::new(800.0, 450.0), 1.0);
    let button;
    {
        let mut ui = session.borrow_mut();
        button = ui.button("Track 2");
        ui.harness.focus_on(Some(button));
        ui.state.lock().unwrap().tracks.remove(0);
    }
    Session::refresh(&session, view);
    {
        let mut ui = session.borrow_mut();
        assert_eq!(ui.button("Track 2"), button);
        assert_eq!(ui.harness.focused_widget_id(), Some(button));
        ui.harness
            .process_text_event(TextEvent::key_down(Key::Named(NamedKey::Enter)));
        ui.harness
            .process_text_event(TextEvent::key_up(Key::Named(NamedKey::Enter)));
        assert_eq!(ui.action(), Some(UiAction::OpenTrack(survivor)));
        // An out-of-order track confirmation inserts before existing survivors.
        let mut extra = tracks(1).tracks.remove(0);
        extra.name = "Earlier".into();
        ui.state.lock().unwrap().tracks.insert(0, extra);
    }
    Session::refresh(&session, view);
    let mut ui = session.borrow_mut();
    assert_eq!(ui.button("Track 2"), button);
    ui.harness.mouse_click_on(button);
    assert_eq!(ui.action(), Some(UiAction::OpenTrack(survivor)));
}

#[test]
fn keyed_plugin_insertion_preserves_surviving_buttons_and_updates_indices() {
    let mut state = tracks(1);
    let id = state.tracks[0].id;
    state.page = Page::Track(id);
    state.tracks[0].plugins = vec![plugin("Delay")];
    let session = Session::new(state, view, Size::new(800.0, 450.0), 1.0);
    let insert;
    {
        let mut ui = session.borrow_mut();
        insert = ui.button("Insert Before");
        ui.harness.focus_on(Some(insert));
        ui.state.lock().unwrap().tracks[0]
            .plugins
            .insert(0, plugin("EPiano"));
    }
    Session::refresh(&session, view);
    let mut ui = session.borrow_mut();
    assert_eq!(ui.harness.focused_widget_id(), Some(insert));
    ui.harness.mouse_click_on(insert);
    assert_eq!(
        ui.action(),
        Some(UiAction::OpenPlugins {
            track_id: id,
            index: 1
        })
    );
}

#[test]
fn obsolete_keyed_messages_are_rejected_and_reorders_keep_actions_on_the_right_track() {
    let state = tracks(3);
    let first = state.tracks[0].id;
    let session = Session::new(state, view, Size::new(800.0, 450.0), 1.0);
    let stale_path;
    {
        let mut ui = session.borrow_mut();
        let old_button = ui.button("Track 2");
        stale_path = ui.path(old_button);
        ui.state.lock().unwrap().tracks.remove(1);
        ui.state.lock().unwrap().tracks.swap(0, 1);
    }
    Session::refresh(&session, view);
    let mut ui = session.borrow_mut();
    let action = || {
        Box::new(masonry::widgets::ButtonPress {
            button: Some(masonry::core::PointerButton::Primary),
        }) as masonry::core::ErasedAction
    };
    assert!(matches!(
        ui.route_path(stale_path, action()),
        xilem::core::MessageResult::Stale
    ));
    let button = ui.button("Track 1");
    ui.harness.mouse_click_on(button);
    assert_eq!(ui.action(), Some(UiAction::OpenTrack(first)));
}

#[test]
fn footer_is_fixed_square_and_flush_on_every_page_size_and_display_scale() {
    for scale in [1.0, 1.5, 2.0] {
        for (width, height) in [(400.0, 280.0), (800.0, 450.0)] {
            for count in [0, 12] {
                for page in 0..3 {
                    let mut state = tracks(count);
                    if let Some(track) = state.tracks.first_mut() {
                        track.plugins = (0..12).map(|_| plugin("EPiano")).collect();
                        state.page = match page {
                            0 => Page::Main,
                            1 => Page::Track(track.id),
                            _ => Page::Plugins(track.id, u16::MAX),
                        };
                    }
                    let session =
                        Session::new(state, view, Size::new(width * scale, height * scale), scale);
                    let mut ui = session.borrow_mut();
                    let footer = ui
                        .widgets::<SizedBox>()
                        .into_iter()
                        .find(|id| {
                            let bounds = rect(&ui, *id);
                            (bounds.height() - 32.0).abs() < 0.01
                                && bounds.x0 == 0.0
                                && (bounds.x1 - width).abs() < 0.01
                        })
                        .expect("full-width 32px footer");
                    let before = rect(&ui, footer);
                    assert!(
                        (before.y0 - (height - 32.0)).abs() < 0.01,
                        "footer: {before:?}"
                    );
                    assert!((before.y1 - height).abs() < 0.01);
                    assert_eq!(before.x0, 0.0);
                    assert!((before.x1 - width).abs() < 0.01);
                    let child = ui.harness.get_widget_with_id(footer).children()[0];
                    assert_eq!(
                        child.get_prop::<masonry::properties::CornerRadius>().radius,
                        0.0
                    );
                    let meters = ui.widgets::<tinex_widgets::MeterWidget>();
                    let progress = *meters.last().unwrap();
                    let meter = meters[meters.len() - 2];
                    assert!(before.contains_rect(rect(&ui, progress)));
                    assert!(before.contains_rect(rect(&ui, meter)));
                    let tempo = *ui.widgets::<Slider>().last().unwrap();
                    assert!(before.contains_rect(rect(&ui, tempo)));
                    let portal = ui.widgets::<Portal<Flex>>()[0];
                    let center = rect(&ui, portal).center();
                    ui.harness.mouse_move((center.x * scale, center.y * scale));
                    ui.harness.mouse_wheel(Vec2::new(0.0, -120.0 * scale));
                    assert_eq!(rect(&ui, footer), before);
                }
            }
        }
    }
}

#[test]
fn timeline_spans_the_window_above_the_footer_with_one_number_per_bar() {
    for (width, height) in [(400.0, 280.0), (800.0, 450.0)] {
        let state = AppState {
            beat: 6.5,
            measures: 8,
            ..tracks(3)
        };
        let session = Session::new(state, view, Size::new(width, height), 1.0);
        let mut ui = session.borrow_mut();
        let timeline = ui.widgets::<tinex_widgets::TimelineWidget>()[0];
        let bounds = rect(&ui, timeline);
        // Play/pause and stop sit at the row's left end.
        let start = 2.0 * TRANSPORT_BUTTON_WIDTH;
        assert_eq!(
            bounds,
            Rect::new(start, height - 60.0, width, height - 32.0)
        );
        let labels = ui.labels();
        let numbers: Vec<_> = (1..=8).map(|bar| bar.to_string()).collect();
        assert!(labels.windows(8).any(|window| window == numbers.as_slice()));
        let node = ui.harness.access_node(timeline).unwrap();
        assert_eq!(node.value().as_deref(), Some("Bar 2 of 8, beat 3"));
    }
}

/// The button around a painted transport symbol.
fn transport_button<V: WidgetView<AppState, UiAction>>(
    ui: &mut Session<V>,
    glyph: TransportGlyph,
) -> Option<WidgetId> {
    ui.widgets::<masonry::widgets::Button>()
        .into_iter()
        .find(|id| {
            ui.harness.get_widget_with_id(*id).children()[0]
                .downcast::<tinex_widgets::TransportIconWidget>()
                .is_some_and(|icon| icon.inner().glyph() == glyph)
        })
}

#[test]
fn transport_buttons_toggle_playing_and_stop_beside_the_timeline() {
    for (playing, shown, hidden) in [
        (false, TransportGlyph::Play, TransportGlyph::Pause),
        (true, TransportGlyph::Pause, TransportGlyph::Play),
    ] {
        let state = AppState {
            playing,
            ..tracks(1)
        };
        let session = Session::new(state, view, Size::new(800.0, 450.0), 1.0);
        let mut ui = session.borrow_mut();
        assert!(transport_button(&mut ui, hidden).is_none());
        let timeline = ui.widgets::<tinex_widgets::TimelineWidget>()[0];
        let timeline = rect(&ui, timeline);
        for (index, (glyph, action)) in [
            (shown, UiAction::TogglePlaying),
            (TransportGlyph::Stop, UiAction::Stop),
        ]
        .into_iter()
        .enumerate()
        {
            let button = transport_button(&mut ui, glyph).unwrap();
            // Only the play/pause button lights up, and only while playing.
            let background = if playing && action == UiAction::TogglePlaying {
                theme::PLAYING
            } else {
                xilem::Color::from_rgb8(68, 71, 90)
            };
            assert_eq!(
                *ui.harness
                    .get_widget_with_id(button)
                    .get_prop::<masonry::properties::Background>(),
                masonry::properties::Background::Color(background)
            );
            let bounds = rect(&ui, button);
            let left = index as f64 * TRANSPORT_BUTTON_WIDTH;
            assert_eq!(
                bounds,
                Rect::new(
                    left,
                    timeline.y0,
                    left + TRANSPORT_BUTTON_WIDTH,
                    timeline.y1
                )
            );
            ui.harness.mouse_move(bounds.center());
            ui.harness
                .mouse_button_press(masonry::core::PointerButton::Primary);
            ui.harness
                .mouse_button_release(masonry::core::PointerButton::Primary);
            assert_eq!(ui.action(), Some(action));
        }
        ui.state.lock().unwrap().connected = false;
    }
}

#[test]
fn transport_buttons_are_disabled_while_disconnected() {
    let state = AppState {
        connected: false,
        ..tracks(1)
    };
    let session = Session::new(state, view, Size::new(800.0, 450.0), 1.0);
    let mut ui = session.borrow_mut();
    for glyph in [TransportGlyph::Play, TransportGlyph::Stop] {
        let button = transport_button(&mut ui, glyph).unwrap();
        let center = rect(&ui, button).center();
        ui.harness.mouse_move(center);
        ui.harness
            .mouse_button_press(masonry::core::PointerButton::Primary);
        ui.harness
            .mouse_button_release(masonry::core::PointerButton::Primary);
        assert!(ui.action().is_none());
    }
}

#[test]
fn scroll_and_focus_survive_meter_rebuilds_and_deleted_pages_fall_back() {
    let session = Session::new(tracks(12), view, Size::new(800.0, 450.0), 1.0);
    let portal;
    let offset;
    {
        let mut ui = session.borrow_mut();
        portal = ui.widgets::<Portal<Flex>>()[0];
        let center = rect(&ui, portal).center();
        ui.harness.mouse_move(center);
        ui.harness.mouse_wheel(Vec2::new(0.0, -120.0));
        offset = ui
            .harness
            .get_widget_with_id(portal)
            .downcast::<Portal<Flex>>()
            .unwrap()
            .inner()
            .get_viewport_pos();
        assert!(offset.y > 0.0);
        ui.state.lock().unwrap().master_output_level = 2.0;
        ui.state.lock().unwrap().cpu_load = Some(42.5);
    }
    Session::refresh(&session, view);
    let mut ui = session.borrow_mut();
    assert_eq!(
        ui.harness
            .get_widget_with_id(portal)
            .downcast::<Portal<Flex>>()
            .unwrap()
            .inner()
            .get_viewport_pos(),
        offset
    );
    assert!(ui.labels().iter().any(|s| s == "42.5%"));
    ui.state.lock().unwrap().page = Page::Track(TrackId::new());
    drop(ui);
    Session::refresh(&session, view);
    let mut ui = session.borrow_mut();
    assert!(ui.labels().contains(&"Create Track".into()));
}

#[test]
fn silence_and_selected_arm_are_presented_without_losing_pending_state() {
    let mut state = tracks(1);
    state.tracks[0].volume_db = -61.0;
    state.tracks[0].output_level = f32::INFINITY;
    state.tracks[0].armed = true;
    state.arm_request_in_flight = true;
    let session = Session::new(state, view, Size::new(800.0, 450.0), 1.0);
    let mut ui = session.borrow_mut();
    assert!(ui.labels().contains(&"−∞ dB".into()));
    let arm = ui.button("Arm ●");
    assert!(ui.harness.get_widget_with_id(arm).ctx().is_disabled());
    assert_eq!(
        *ui.harness
            .get_widget_with_id(arm)
            .get_prop::<masonry::properties::Background>(),
        masonry::properties::Background::Color(theme::SELECTED)
    );
    ui.harness.mouse_click_on(arm);
    assert!(ui.action().is_none());
}

#[test]
fn main_page_explains_empty_pending_and_disconnected_states_without_phantom_tracks() {
    let session = Session::new(AppState::default(), view, Size::new(400.0, 280.0), 1.0);
    {
        let mut ui = session.borrow_mut();
        let labels = ui.labels();
        assert!(labels.contains(&"Tracks · 0".into()));
        assert!(labels.contains(&"Your session starts here".into()));
        assert!(
            labels
                .iter()
                .any(|text| text.contains("Arm tracks to play them"))
        );
        update(
            &mut ui.state.lock().unwrap(),
            Action::Ui(UiAction::CreateTrack),
        );
    }
    Session::refresh(&session, view);
    {
        let mut ui = session.borrow_mut();
        let labels = ui.labels();
        assert!(labels.contains(&"Tracks · 0".into()));
        assert!(labels.contains(&"Creating your first track…".into()));
        assert!(labels.contains(&"Creating 1 track…".into()));
        assert!(!labels.contains(&"Track 1".into()));
        update(&mut ui.state.lock().unwrap(), Action::Disconnected);
    }
    Session::refresh(&session, view);
    let mut ui = session.borrow_mut();
    assert!(
        ui.labels()
            .contains(&"Audio engine disconnected. Track settings remain available.".into())
    );
    let create = ui.button("Create Track");
    assert!(ui.harness.get_widget_with_id(create).ctx().is_disabled());
    ui.harness.mouse_click_on(create);
    assert!(ui.action().is_none());
}

#[test]
fn main_page_summarizes_confirmed_plugin_order_duplicates_and_pending_additions() {
    let mut state = tracks(2);
    state.tracks[0].plugins = vec![plugin("EPiano"), plugin("Delay"), plugin("EPiano")];
    let pending = plugin("Compressor");
    state.tracks[0].pending_plugins.insert(pending.id, pending);
    let session = Session::new(state, view, Size::new(800.0, 450.0), 1.0);
    let mut ui = session.borrow_mut();
    let labels = ui.labels();
    assert!(labels.contains(&"Tracks · 2".into()));
    assert!(labels.contains(&"EPiano · Delay · EPiano · Adding plugin…".into()));
    assert!(labels.contains(&"No plugins".into()));
    assert!(!labels.iter().any(|text| text.contains("Compressor")));
}

#[test]
fn main_track_controls_stay_aligned_and_inside_narrow_windows_with_long_names() {
    for scale in [1.0, 2.0] {
        for width in [400.0, 800.0] {
            let mut state = tracks(2);
            let long_name = "A very long track name that must not push controls off screen";
            state.tracks[0].name = long_name.into();
            state.tracks[0].plugins = (0..16).map(|_| plugin("Electric Piano")).collect();
            state.tracks[1].armed = true;
            state.tracks[1].deleting = true;
            let session = Session::new(state, view, Size::new(width * scale, 450.0 * scale), scale);
            let mut ui = session.borrow_mut();
            let portal = ui.widgets::<Portal<Flex>>()[0];
            let viewport = rect(&ui, portal);
            let sliders = ui.widgets::<Slider>();
            let first = rect(&ui, sliders[0]);
            let second = rect(&ui, sliders[1]);
            assert_eq!(first.x0, second.x0);
            assert_eq!(first.x1, second.x1);
            assert!(first.width() >= 80.0, "usable slider: {first:?}");
            for text in [long_name, "Track 2", "Arm", "Delete", "Arm ●", "Deleting…"] {
                let control = ui.button(text);
                let bounds = rect(&ui, control);
                assert!(
                    viewport.contains_rect(bounds),
                    "{text}: {bounds:?} in {viewport:?}"
                );
                assert!(bounds.width() >= 40.0);
            }
            // The footer's tempo slider comes last and sits outside the page.
            for id in &sliders[..sliders.len() - 1] {
                assert!(viewport.contains_rect(rect(&ui, *id)));
            }
        }
    }
}

#[test]
fn arm_confirmation_never_resizes_controls_or_moves_neighbors_on_either_page() {
    fn controls<V: WidgetView<AppState, UiAction>>(ui: &mut Session<V>) -> Vec<(WidgetId, Rect)> {
        let mut ids = ui.widgets::<masonry::widgets::Button>();
        ids.extend(ui.widgets::<Slider>());
        ids.extend(ui.widgets::<tinex_widgets::MeterWidget>());
        ids.into_iter().map(|id| (id, rect(ui, id))).collect()
    }

    for scale in [1.0, 2.0] {
        for width in [400.0, 800.0] {
            for track_page in [false, true] {
                let mut state = tracks(2);
                let id = state.tracks[0].id;
                if track_page {
                    state.page = Page::Track(id);
                    state.tracks[0].plugins = vec![plugin("EPiano")];
                }
                let session =
                    Session::new(state, view, Size::new(width * scale, 450.0 * scale), scale);
                let before = {
                    let mut ui = session.borrow_mut();
                    let arm = ui.button("Arm");
                    assert_eq!(rect(&ui, arm).width(), TRACK_ARM_WIDTH);
                    assert_eq!(rect(&ui, arm).height(), 32.0);
                    controls(&mut ui)
                };
                for armed in [true, false] {
                    update(
                        &mut session.borrow().state.lock().unwrap(),
                        Action::Ui(UiAction::ToggleTrackArm(id)),
                    );
                    Session::refresh(&session, view);
                    assert_eq!(
                        controls(&mut session.borrow_mut()),
                        before,
                        "pending arm request"
                    );
                    update(
                        &mut session.borrow().state.lock().unwrap(),
                        Action::Notification(TinexNotification::ArmTracks(if armed {
                            [id].into()
                        } else {
                            Default::default()
                        })),
                    );
                    Session::refresh(&session, view);
                    let mut ui = session.borrow_mut();
                    ui.button(if armed { "Arm ●" } else { "Arm" });
                    assert_eq!(controls(&mut ui), before, "confirmed arm state");
                }
            }
        }
    }
}

#[test]
fn status_includes_pending_activity_and_error() {
    let mut state = AppState::default();
    update(&mut state, Action::Ui(UiAction::CreateTrack));
    assert_eq!(status(&state).0, "Creating Track…");
    state.message = Some("Audio engine disconnected".into());
    assert_eq!(
        status(&state),
        (
            "Audio engine disconnected · Creating Track…".into(),
            theme::ERROR
        )
    );
}

#[test]
fn footer_bpm_slider_is_labelled_and_jumps_or_steps_the_tempo() {
    let state = AppState {
        bpm: 120.0,
        ..Default::default()
    };
    let session = Session::new(state, view, Size::new(800.0, 450.0), 1.0);
    {
        let mut ui = session.borrow_mut();
        let labels = ui.labels();
        let caption = labels.iter().position(|label| label == "BPM").unwrap();
        assert_eq!(labels[caption + 1], "120");
        // The main page has no tracks, so the only slider is the footer's.
        let slider = ui.widgets::<Slider>()[0];
        ui.harness.focus_on(Some(slider));
        for (key, bpm) in [
            (NamedKey::ArrowLeft, 119.0),
            (NamedKey::ArrowRight, 120.0),
            (NamedKey::Home, MIN_BPM),
            (NamedKey::End, MAX_BPM),
        ] {
            ui.harness
                .process_text_event(TextEvent::key_down(Key::Named(key)));
            assert_eq!(ui.action(), Some(UiAction::SetBpm(bpm)));
        }
        // A single click lands far from the current tempo.
        let bounds = rect(&ui, slider);
        ui.harness
            .mouse_move((bounds.x0 + bounds.width() * 0.1, bounds.center().y));
        ui.harness
            .mouse_button_press(masonry::core::PointerButton::Primary);
        ui.harness
            .mouse_button_release(masonry::core::PointerButton::Primary);
        assert!(matches!(ui.action(), Some(UiAction::SetBpm(bpm)) if bpm < 60.0));
        assert!(ui.action().is_none());
        ui.state.lock().unwrap().connected = false;
    }
    Session::refresh(&session, view);
    let mut ui = session.borrow_mut();
    let slider = ui.widgets::<Slider>()[0];
    ui.harness.mouse_click_on(slider);
    assert!(ui.action().is_none());
}
