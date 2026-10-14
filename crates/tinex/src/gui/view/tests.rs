use super::*;
use crate::gui::theme::Theme;
use tinex_widgets::TextRenderer;
use vello::Scene;
use vello::kurbo::Size;

fn confirmed_tracks(count: usize) -> AppState {
    use crate::gui::{action::Action, update::update};
    use tinex_core::{TinexNotification, TinexRequest};
    let mut state = AppState::default();
    for _ in 0..count {
        let effect = update(&mut state, Action::Ui(UiAction::CreateTrack))
            .effects
            .remove(0);
        let TinexRequest::NewTrack(track) = effect.request else {
            panic!("expected track")
        };
        update(
            &mut state,
            Action::Notification(TinexNotification::TrackCreated(track.id())),
        );
    }
    state
}

fn arm_buttons(widget: &Widget<UiAction>) -> Vec<(&str, bool, tinex_widgets::WidgetId, bool)> {
    match widget {
        Widget::Button {
            id,
            label,
            enabled,
            selected,
            action: UiAction::ToggleTrackArm(_),
            ..
        } => {
            vec![(label.as_str(), *enabled, *id, *selected)]
        }
        Widget::Row { children, .. }
        | Widget::ClickableRow { children, .. }
        | Widget::Column(children)
        | Widget::StatusBar(children) => children.iter().flat_map(arm_buttons).collect(),
        Widget::Scroll { child, .. } => arm_buttons(child),
        _ => Vec::new(),
    }
}

#[test]
fn armed_button_color_remains_distinct_when_hovered_pressed_or_disabled() {
    use tinex_widgets::{Input, Ui};
    use vello::kurbo::Point;

    let mut state = confirmed_tracks(1);
    let theme = Theme::default();
    let mut text = TextRenderer::new();
    for (enabled, hovered, pressed) in [
        (true, false, false),
        (true, true, false),
        (true, true, true),
        (false, true, false),
    ] {
        state.arm_request_in_flight = !enabled;
        let mut paints = Vec::new();
        for armed in [false, true] {
            state.tracks[0].armed = armed;
            let control = track_arm_control(&state, &state.tracks[0]);
            assert_eq!(arm_buttons(&control)[0].0, "Arm");
            let mut ui = Ui::new(control, Size::new(400.0, 280.0));
            if hovered {
                ui.input(Input::PointerMoved(Point::new(50.0, 34.0)));
            }
            if pressed {
                ui.input(Input::PointerPressed);
            }
            let mut scene = Scene::new();
            ui.render(&mut scene, &mut text, &theme.widgets, 1.0);
            paints.push(scene.encoding().draw_data.clone());
        }
        assert_ne!(
            paints[0], paints[1],
            "armed color lost for enabled={enabled}, hovered={hovered}, pressed={pressed}"
        );
    }
}

#[test]
fn arm_controls_support_pointer_keyboard_confirmation_and_disabled_precedence() {
    use crate::gui::{action::Action, update::update};
    use tinex_core::{TinexNotification, TinexRequest};
    use tinex_widgets::{ActivationKey, Input, Ui};
    use vello::kurbo::Point;

    for size in [Size::new(400.0, 280.0), Size::new(800.0, 450.0)] {
        for scale in [1.0, 2.0] {
            for main_page in [true, false] {
                let mut state = confirmed_tracks(2);
                let id = state.tracks[0].id;
                let button = state.tracks[0].arm_button;
                let (page, point, tabs) = if main_page {
                    (Page::Main, Point::new(76.0, 194.0), 4)
                } else {
                    (Page::Track(id), Point::new(220.0, 186.0), 4)
                };
                state.page = page;
                update(
                    &mut state,
                    Action::Notification(TinexNotification::ArmTracks(Default::default())),
                );
                let mut ui = Ui::new(view(&state), size);
                let physical = Point::new(point.x * scale, point.y * scale);
                let logical = Point::new(physical.x / scale, physical.y / scale);
                ui.input(Input::PointerMoved(logical));
                ui.input(Input::PointerPressed);
                let actions = ui.input(Input::PointerReleased);
                assert!(
                    matches!(actions.as_slice(), [UiAction::ToggleTrackArm(track)] if *track == id)
                );
                let result = update(&mut state, Action::Ui(actions[0]));
                assert!(
                    matches!(&result.effects[0].request, TinexRequest::ArmTracks(ids) if *ids == [id].into())
                );
                assert_eq!(state.page, page);
                assert!(!state.tracks[0].armed);
                assert!(
                    arm_buttons(&view(&state))
                        .iter()
                        .all(|(label, enabled, _, selected)| *label == "Arm"
                            && !enabled
                            && !selected)
                );
                ui.set_view(view(&state), size);
                ui.input(Input::PointerPressed);
                assert!(ui.input(Input::PointerReleased).is_empty());

                update(
                    &mut state,
                    Action::Notification(TinexNotification::ArmTracks([id].into())),
                );
                assert_eq!(arm_buttons(&view(&state))[0], ("Arm", true, button, true));
                ui.set_view(view(&state), size);
                // Both activation keys reach the arm control through Tab navigation.
                for key in [ActivationKey::Enter, ActivationKey::Space] {
                    ui.input(Input::Cancel);
                    for _ in 0..tabs {
                        ui.input(Input::Tab { backwards: false });
                    }
                    ui.input(Input::Key {
                        key,
                        pressed: true,
                        repeat: false,
                    });
                    let actions = ui.input(Input::Key {
                        key,
                        pressed: false,
                        repeat: false,
                    });
                    assert!(
                        matches!(actions.as_slice(), [UiAction::ToggleTrackArm(track)] if *track == id)
                    );
                    let result = update(&mut state, Action::Ui(actions[0]));
                    assert!(
                        matches!(&result.effects[0].request, TinexRequest::ArmTracks(ids) if ids.is_empty())
                    );
                    assert!(state.tracks[0].armed);
                    update(
                        &mut state,
                        Action::Notification(TinexNotification::ArmTracks([id].into())),
                    );
                    ui.set_view(view(&state), size);
                }

                for disconnected in [false, true] {
                    state.tracks[0].deleting = !disconnected;
                    if disconnected {
                        update(&mut state, Action::Disconnected);
                    }
                    ui.set_view(view(&state), size);
                    assert_eq!(arm_buttons(&view(&state))[0], ("Arm", false, button, true));
                    ui.input(Input::PointerMoved(logical));
                    ui.input(Input::PointerPressed);
                    assert!(ui.input(Input::PointerReleased).is_empty());
                    assert_eq!(state.page, page);
                }
            }
        }
    }
}

#[test]
fn scrolling_track_cards_preserves_the_footer_and_targets_the_visible_track() {
    use crate::gui::{action::Action, update::update};
    use tinex_core::TinexNotification;
    use tinex_widgets::{ActivationKey, Input, Ui};
    use vello::kurbo::Point;

    for size in [Size::new(400.0, 280.0), Size::new(800.0, 450.0)] {
        for scale in [1.0, 2.0] {
            for count in [0, 10] {
                let mut state = confirmed_tracks(count);
                let mut ui = Ui::new(view(&state), size);
                let mut text = TextRenderer::new();
                let theme = Theme::default();
                let mut scene = Scene::new();
                ui.render(&mut scene, &mut text, &theme.widgets, scale);
                // The final two text runs belong to Master and CPU.
                let footer: Vec<_> = scene
                    .encoding()
                    .resources
                    .glyph_runs
                    .iter()
                    .rev()
                    .take(2)
                    .map(|run| run.transform)
                    .collect();
                assert_eq!(footer.len(), 2);
                assert!(
                    footer.iter().all(|transform| transform.translation[1]
                        == ((size.height - 26.0) * scale) as f32),
                    "footer positions: {footer:?}; size: {size:?}; count: {count}"
                );
                ui.input(Input::PointerMoved(Point::new(36.0, 100.0)));
                ui.input(Input::Scroll(10_000.0));
                ui.set_view(view(&state), size);
                scene.reset();
                ui.render(&mut scene, &mut text, &theme.widgets, scale);
                let scrolled_footer: Vec<_> = scene
                    .encoding()
                    .resources
                    .glyph_runs
                    .iter()
                    .rev()
                    .take(2)
                    .map(|run| run.transform)
                    .collect();
                assert_eq!(scrolled_footer, footer);
                // Track controls cannot receive clicks through the fixed footer.
                for x in [1.0, 156.0, size.width - 1.0] {
                    ui.input(Input::PointerMoved(Point::new(x, size.height - 16.0)));
                    ui.input(Input::PointerPressed);
                    assert!(ui.input(Input::PointerReleased).is_empty());
                }
                if let Some(track) = state.tracks.last() {
                    let id = track.id;
                    let button = track.arm_button;
                    ui.input(Input::PointerMoved(Point::new(76.0, size.height - 74.0)));
                    ui.input(Input::PointerPressed);
                    let actions = ui.input(Input::PointerReleased);
                    assert!(
                        matches!(actions.as_slice(), [UiAction::ToggleTrackArm(track)] if *track == id)
                    );
                    update(&mut state, Action::Ui(actions[0]));
                    update(
                        &mut state,
                        Action::Notification(TinexNotification::ArmTracks([id].into())),
                    );
                    ui.set_view(view(&state), size);
                    assert_eq!(
                        arm_buttons(&view(&state)).last().copied(),
                        Some(("Arm", true, button, true))
                    );
                    // Confirmation rebuilds keep focus and scroll on the same control.
                    ui.input(Input::Key {
                        key: ActivationKey::Enter,
                        pressed: true,
                        repeat: false,
                    });
                    let actions = ui.input(Input::Key {
                        key: ActivationKey::Enter,
                        pressed: false,
                        repeat: false,
                    });
                    assert!(
                        matches!(actions.as_slice(), [UiAction::ToggleTrackArm(track)] if *track == id)
                    );
                    assert_eq!(state.page, Page::Main);
                }
            }
        }
    }
}

#[test]
fn cpu_meter_is_in_the_shared_footer_on_every_page() {
    use crate::gui::{action::Action, update::update};
    use tinex_core::{TinexNotification, TinexRequest};

    let mut state = AppState::default();
    let effect = update(&mut state, Action::Ui(UiAction::CreateTrack))
        .effects
        .remove(0);
    let TinexRequest::NewTrack(track) = effect.request else {
        panic!("expected track");
    };
    update(
        &mut state,
        Action::Notification(TinexNotification::TrackCreated(track.id())),
    );
    for page in [
        Page::Main,
        Page::Track(track.id()),
        Page::Plugins(track.id(), u16::MAX),
    ] {
        state.page = page;
        for load in [None, Some(0.0), Some(12.3), Some(100.0)] {
            state.cpu_load = load;
            let Widget::Column(children) = view(&state) else {
                panic!("expected column")
            };
            let Widget::StatusBar(footer) = children.last().unwrap() else {
                panic!("expected footer")
            };
            assert_eq!(footer.len(), 5);
            assert!(matches!(
                &footer[4],
                Widget::PercentageMeter { percentage, width: 88.0 }
                    if *percentage == load.map_or(0.0, f64::from)
            ));
            assert!(
                matches!(&footer[3], Widget::Label { text, width: Some(32.0), .. } if text == "CPU")
            );
            for size in [Size::new(400.0, 280.0), Size::new(800.0, 450.0)] {
                let ui = tinex_widgets::Ui::new(view(&state), size);
                let mut scene = Scene::new();
                ui.render(
                    &mut scene,
                    &mut TextRenderer::new(),
                    &Theme::default().widgets,
                    2.0,
                );
                assert!(!scene.encoding().resources.glyph_runs.is_empty());
                assert!(
                    scene
                        .encoding()
                        .resources
                        .glyphs
                        .iter()
                        .all(|glyph| glyph.id != 0)
                );
            }
        }
    }
}

#[test]
fn confirmed_rows_emit_delete_actions_and_pending_rows_disable_them() {
    use crate::gui::action::Action;
    use crate::gui::update::update;
    use tinex_core::{TinexNotification, TinexRequest};
    use tinex_widgets::{Input, Ui};
    use vello::kurbo::Point;

    let mut state = AppState::default();
    let effect = update(&mut state, Action::Ui(UiAction::CreateTrack))
        .effects
        .remove(0);
    let TinexRequest::NewTrack(track) = effect.request else {
        panic!("expected create");
    };
    let size = Size::new(400.0, 280.0);
    let mut ui = Ui::new(view(&state), size);
    let delete_position = Point::new(184.0, 194.0);
    ui.input(Input::PointerMoved(delete_position));
    ui.input(Input::PointerPressed);
    assert!(ui.input(Input::PointerReleased).is_empty());

    update(
        &mut state,
        Action::Notification(TinexNotification::TrackCreated(track.id())),
    );
    ui.set_view(view(&state), size);
    ui.input(Input::PointerPressed);
    let actions = ui.input(Input::PointerReleased);
    assert!(matches!(actions.as_slice(), [UiAction::DeleteTrack(id)] if *id == track.id()));
    update(&mut state, Action::Ui(actions[0]));
    assert_eq!(state.page, Page::Main); // Delete is not a navigation click.
    ui.set_view(view(&state), size);
    ui.input(Input::PointerPressed);
    assert!(ui.input(Input::PointerReleased).is_empty());
    let mut scene = Scene::new();
    ui.render(
        &mut scene,
        &mut TextRenderer::new(),
        &Theme::default().widgets,
        1.0,
    );
    assert!(!scene.encoding().resources.glyph_runs.is_empty());
    assert!(
        scene
            .encoding()
            .resources
            .glyphs
            .iter()
            .all(|glyph| glyph.id != 0)
    );
}

#[test]
fn meter_bars_render_silence_hot_and_over_range_values_at_hidpi() {
    use crate::gui::action::Action;
    use crate::gui::update::update;
    use tinex_core::{TinexNotification, TinexRequest};
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
    let mut text = TextRenderer::new();
    let theme = Theme::default();
    for level in [0.0, 0.5, 1.0, 2.0, 16.0, f32::INFINITY, f32::NAN] {
        state.tracks[0].output_level = level;
        state.master_output_level = level;
        let ui = tinex_widgets::Ui::new(view(&state), Size::new(400.0, 280.0));
        let mut scene = Scene::new();
        ui.render(&mut scene, &mut text, &theme.widgets, 2.0);
        assert!(!scene.encoding().is_empty());
        assert!(
            scene
                .encoding()
                .resources
                .glyphs
                .iter()
                .all(|glyph| glyph.id != 0 && glyph.x.is_finite() && glyph.y.is_finite())
        );
    }
}

#[test]
fn loading_and_error_status_follow_confirmed_engine_operations() {
    use crate::gui::action::Action;
    use crate::gui::update::update;
    use tinex_core::{TinexNotification, TinexRequest, track::Track};

    fn create(state: &mut AppState) -> Track {
        let effect = update(state, Action::Ui(UiAction::CreateTrack))
            .effects
            .remove(0);
        let TinexRequest::NewTrack(track) = effect.request else {
            panic!("expected create");
        };
        track
    }
    let mut state = AppState::default();
    assert_eq!(status(&state).0, "Ready");
    let first = create(&mut state);
    assert_eq!(status(&state).0, "Creating Track…");
    let second = create(&mut state);
    assert_eq!(status(&state).0, "Creating 2 Tracks…");
    update(
        &mut state,
        Action::Notification(TinexNotification::TrackCreated(first.id())),
    );
    assert_eq!(status(&state).0, "Creating Track…");
    update(&mut state, Action::Ui(UiAction::DeleteTrack(first.id())));
    assert_eq!(status(&state).0, "Creating Track… · Deleting Track…");
    update(
        &mut state,
        Action::Notification(TinexNotification::TrackCreated(second.id())),
    );
    assert_eq!(status(&state).0, "Deleting Track…");
    update(
        &mut state,
        Action::Notification(TinexNotification::TrackDeleted(first)),
    );
    assert_eq!(status(&state).0, "Ready");

    let failed = create(&mut state);
    let _pending = create(&mut state);
    update(
        &mut state,
        Action::Notification(TinexNotification::TrackCreationFailed(failed)),
    );
    let (text, tone) = status(&state);
    assert!(text.contains("Could not create Track 3"));
    assert!(text.contains("Creating Track…"));
    assert!(matches!(tone, Tone::Error));
    update(&mut state, Action::Disconnected);
    assert!(status(&state).0.contains("Audio engine disconnected"));
}

#[test]
fn status_and_master_level_are_in_the_footer_not_the_header_or_track_list() {
    use crate::gui::action::Action;
    use crate::gui::update::update;
    let mut state = AppState::default();
    update(&mut state, Action::Ui(UiAction::CreateTrack));
    state.master_output_level = 2.0;
    let Widget::Column(children) = view(&state) else {
        panic!("expected column");
    };
    assert_eq!(children.len(), 3);
    let Widget::Row {
        children: header, ..
    } = &children[0]
    else {
        panic!("expected header");
    };
    assert_eq!(header.len(), 1);
    assert!(matches!(children[1], Widget::Scroll { .. }));
    let Widget::StatusBar(footer) = &children[2] else {
        panic!("expected footer");
    };
    assert!(matches!(&footer[0], Widget::Label { text, .. } if text == "Creating Track…"));
    assert!(
        matches!(&footer[1], Widget::Label { text, width: Some(56.0), .. } if text == "Master")
    );
    assert!(
        matches!(&footer[2], Widget::CompactLevelMeter { meter, .. } if meter.amplitude == 2.0)
    );
}

#[test]
fn track_clicks_open_the_selected_page_and_back_returns_to_the_main_view() {
    use crate::gui::action::Action;
    use crate::gui::update::update;
    use tinex_core::{TinexNotification, TinexRequest};
    use tinex_widgets::{ActivationKey, Input, Ui};
    use vello::kurbo::Point;

    fn click(ui: &mut Ui<UiAction>, point: Point) -> Vec<UiAction> {
        ui.input(Input::PointerMoved(point));
        ui.input(Input::PointerPressed);
        ui.input(Input::PointerReleased)
    }
    let mut state = AppState::default();
    let effect = update(&mut state, Action::Ui(UiAction::CreateTrack))
        .effects
        .remove(0);
    let TinexRequest::NewTrack(track) = effect.request else {
        panic!("expected track");
    };
    let size = Size::new(400.0, 280.0);
    let mut ui = Ui::new(view(&state), size);
    assert!(click(&mut ui, Point::new(36.0, 100.0)).is_empty()); // Still waiting for confirmation.
    update(
        &mut state,
        Action::Notification(TinexNotification::TrackCreated(track.id())),
    );
    state.tracks[0].output_level = 2.0;
    ui.set_view(view(&state), size);

    for point in [Point::new(20.0, 100.0), Point::new(300.0, 194.0)] {
        let actions = click(&mut ui, point); // Track row outside its nested controls.
        assert!(matches!(actions.as_slice(), [UiAction::OpenTrack(id)] if *id == track.id()));
        let result = update(&mut state, Action::Ui(actions[0]));
        assert!(result.changed && result.effects.is_empty());
        assert_eq!(state.page, Page::Track(track.id()));
        ui.set_view(view(&state), size);

        let Widget::Column(children) = view(&state) else {
            panic!("expected page");
        };
        let Widget::Row {
            children: header, ..
        } = &children[0]
        else {
            panic!("expected header");
        };
        assert!(
            matches!(&header[0], Widget::Button { label, enabled: true, action: UiAction::BackToMain, .. } if label == "Back to Tracks")
        );
        assert!(matches!(&header[1], Widget::Label { text, .. } if text == "Track 1"));
        let Widget::Scroll { child, .. } = &children[1] else {
            panic!("expected content");
        };
        let Widget::Column(content) = &**child else {
            panic!("expected column");
        };
        let Widget::Row {
            children: output, ..
        } = &content[0]
        else {
            panic!("expected output row");
        };
        assert!(matches!(&output[1], Widget::MeterSlider { meter, .. } if meter.amplitude == 2.0));
        let Widget::StatusBar(footer) = &children[2] else {
            panic!("expected footer");
        };
        assert!(matches!(&footer[1], Widget::Label { text, .. } if text == "Master"));
        let mut scene = Scene::new();
        ui.render(
            &mut scene,
            &mut TextRenderer::new(),
            &Theme::default().widgets,
            2.0,
        );
        assert!(!scene.encoding().resources.glyph_runs.is_empty());
        assert!(
            scene
                .encoding()
                .resources
                .glyphs
                .iter()
                .all(|glyph| glyph.id != 0)
        );

        let actions = click(&mut ui, Point::new(60.0, 42.0));
        assert!(matches!(actions.as_slice(), [UiAction::BackToMain]));
        let result = update(&mut state, Action::Ui(actions[0]));
        assert!(result.changed && result.effects.is_empty());
        assert_eq!(state.page, Page::Main);
        ui.set_view(view(&state), size);
        assert_eq!(state.tracks.len(), 1);
    }

    // The clickable row also participates in Tab/Enter navigation.
    ui.input(Input::Cancel);
    ui.input(Input::Tab { backwards: false }); // Create.
    ui.input(Input::Tab { backwards: false }); // Track row.
    ui.input(Input::Key {
        key: ActivationKey::Enter,
        pressed: true,
        repeat: false,
    });
    let actions = ui.input(Input::Key {
        key: ActivationKey::Enter,
        pressed: false,
        repeat: false,
    });
    assert!(matches!(actions.as_slice(), [UiAction::OpenTrack(id)] if *id == track.id()));
    update(&mut state, Action::Ui(actions[0]));
    update(&mut state, Action::Disconnected);
    ui.set_view(view(&state), size);
    let actions = click(&mut ui, Point::new(60.0, 42.0));
    assert!(matches!(actions.as_slice(), [UiAction::BackToMain]));
    update(&mut state, Action::Ui(actions[0]));
    assert_eq!(state.page, Page::Main); // Back doesn't depend on engine connectivity.
}

#[test]
fn add_plugin_picker_supports_pointer_keyboard_and_back_navigation() {
    use crate::gui::action::Action;
    use crate::gui::update::update;
    use tinex_core::{TinexNotification, TinexRequest};
    use tinex_widgets::{ActivationKey, Input, Ui};
    use vello::kurbo::Point;

    let mut state = AppState::default();
    let effect = update(&mut state, Action::Ui(UiAction::CreateTrack))
        .effects
        .remove(0);
    let TinexRequest::NewTrack(track) = effect.request else {
        panic!("expected track")
    };
    let id = track.id();
    update(
        &mut state,
        Action::Notification(TinexNotification::TrackCreated(id)),
    );
    update(&mut state, Action::Ui(UiAction::OpenTrack(id)));
    let size = Size::new(400.0, 280.0);
    let mut ui = Ui::new(view(&state), size);
    ui.input(Input::PointerMoved(Point::new(80.0, 186.0)));
    ui.input(Input::PointerPressed);
    let actions = ui.input(Input::PointerReleased);
    assert!(
        matches!(actions.as_slice(), [UiAction::OpenPlugins { track_id, index: u16::MAX }] if *track_id == id)
    );
    update(&mut state, Action::Ui(actions[0]));
    assert_eq!(state.page, Page::Plugins(id, u16::MAX));
    ui.set_view(view(&state), size);

    let Widget::Column(children) = view(&state) else {
        panic!("expected page")
    };
    assert!(matches!(children[2], Widget::StatusBar(_)));
    let Widget::Scroll { child, .. } = &children[1] else {
        panic!("expected scroll")
    };
    let Widget::Column(rows) = &**child else {
        panic!("expected rows")
    };
    assert_eq!(rows.len(), tinex_plugins::FACTORY.len());
    for (index, row) in rows.iter().enumerate() {
        let Widget::Row { children, .. } = row else {
            panic!("expected row")
        };
        assert!(
            matches!(&children[0], Widget::Button { label, enabled: true, action: UiAction::AddPlugin { track_id, index: choice }, .. }
            if label == tinex_plugins::FACTORY[index].name() && *track_id == id && *choice == index)
        );
    }

    ui.input(Input::Cancel);
    ui.input(Input::Tab { backwards: false }); // Back.
    ui.input(Input::Tab { backwards: false }); // First plugin.
    ui.input(Input::Key {
        key: ActivationKey::Enter,
        pressed: true,
        repeat: false,
    });
    let actions = ui.input(Input::Key {
        key: ActivationKey::Enter,
        pressed: false,
        repeat: false,
    });
    assert!(
        matches!(actions.as_slice(), [UiAction::AddPlugin { track_id, index: 0 }] if *track_id == id)
    );
    let result = update(&mut state, Action::Ui(actions[0]));
    assert!(
        matches!(&result.effects[0].request, TinexRequest::AddPlugin { track_id, .. } if *track_id == id)
    );
    assert_eq!(state.page, Page::Track(id));
    // Replayed selection must not add another plugin after leaving the picker.
    assert!(
        update(&mut state, Action::Ui(actions[0]))
            .effects
            .is_empty()
    );

    update(
        &mut state,
        Action::Ui(UiAction::OpenPlugins {
            track_id: id,
            index: u16::MAX,
        }),
    );
    update(&mut state, Action::Disconnected);
    ui.set_view(view(&state), size);
    ui.input(Input::PointerMoved(Point::new(80.0, 100.0)));
    ui.input(Input::PointerPressed);
    assert!(ui.input(Input::PointerReleased).is_empty()); // Selection disabled.
    ui.input(Input::PointerMoved(Point::new(80.0, 42.0)));
    ui.input(Input::PointerPressed);
    let actions = ui.input(Input::PointerReleased);
    assert!(matches!(actions.as_slice(), [UiAction::OpenTrack(track_id)] if *track_id == id));
    update(&mut state, Action::Ui(actions[0]));
    assert_eq!(state.page, Page::Track(id));
}

#[test]
fn plugin_picker_rejects_stale_tracks_and_invalid_selections() {
    use crate::gui::action::Action;
    use crate::gui::update::update;
    use tinex_core::{TinexNotification, TinexRequest};
    let mut state = AppState::default();
    let effect = update(&mut state, Action::Ui(UiAction::CreateTrack))
        .effects
        .remove(0);
    let TinexRequest::NewTrack(track) = effect.request else {
        panic!("expected track")
    };
    let id = track.id();
    assert!(
        !update(
            &mut state,
            Action::Ui(UiAction::OpenPlugins {
                track_id: id,
                index: u16::MAX
            })
        )
        .changed
    );
    update(
        &mut state,
        Action::Notification(TinexNotification::TrackCreated(id)),
    );
    update(
        &mut state,
        Action::Ui(UiAction::OpenPlugins {
            track_id: id,
            index: u16::MAX,
        }),
    );
    assert!(
        update(
            &mut state,
            Action::Ui(UiAction::AddPlugin {
                track_id: id,
                index: usize::MAX
            })
        )
        .effects
        .is_empty()
    );
    update(&mut state, Action::Ui(UiAction::DeleteTrack(id)));
    assert!(
        update(
            &mut state,
            Action::Ui(UiAction::AddPlugin {
                track_id: id,
                index: 0
            })
        )
        .effects
        .is_empty()
    );
    update(
        &mut state,
        Action::Notification(TinexNotification::TrackDeleted(track)),
    );
    assert_eq!(state.page, Page::Main);
    assert!(
        !update(
            &mut state,
            Action::Ui(UiAction::OpenPlugins {
                track_id: id,
                index: u16::MAX
            })
        )
        .changed
    );
}

#[test]
fn initial_view_draws_bundled_labels() {
    let state = AppState::default();
    let ui = tinex_widgets::Ui::new(view(&state), Size::new(800.0, 450.0));
    let mut scene = Scene::new();
    ui.render(
        &mut scene,
        &mut TextRenderer::new(),
        &Theme::default().widgets,
        2.0,
    );
    assert!(!scene.encoding().is_empty());
    assert!(!scene.encoding().resources.glyph_runs.is_empty());
    assert!(
        scene
            .encoding()
            .resources
            .glyphs
            .iter()
            .all(|glyph| glyph.id != 0)
    );
}

#[test]
fn track_volume_fader_supports_keyboard_and_pointer_at_multiple_sizes_and_scales() {
    use crate::gui::{action::Action, update::update};
    use tinex_core::{TinexNotification, TinexRequest};
    use tinex_widgets::{AdjustmentKey, Input, Ui};
    use vello::kurbo::Point;
    let mut state = AppState::default();
    let effect = update(&mut state, Action::Ui(UiAction::CreateTrack))
        .effects
        .remove(0);
    let TinexRequest::NewTrack(track) = effect.request else {
        panic!("expected track");
    };
    let id = track.id();
    update(
        &mut state,
        Action::Notification(TinexNotification::TrackCreated(id)),
    );
    state.page = Page::Track(id);
    for size in [Size::new(400.0, 280.0), Size::new(800.0, 450.0)] {
        state.tracks[0].volume_db = 0.0;
        let mut ui = Ui::new(view(&state), size);
        ui.input(Input::Tab { backwards: false }); // Back.
        ui.input(Input::Tab { backwards: false }); // Volume.
        let actions = ui.input(Input::Adjust(AdjustmentKey::Decrease));
        assert!(
            matches!(actions.as_slice(), [UiAction::SetTrackVolume { track_id, db: -1.0 }] if *track_id == id)
        );
        update(&mut state, Action::Ui(actions[0]));
        ui.set_view(view(&state), size);
        let actions = ui.input(Input::Adjust(AdjustmentKey::Home));
        update(&mut state, Action::Ui(actions[0]));
        ui.set_view(view(&state), size);
        let Widget::Column(page) = view(&state) else {
            panic!("expected page");
        };
        let Widget::Scroll { child, .. } = &page[1] else {
            panic!("expected scroll");
        };
        let Widget::Column(content) = &**child else {
            panic!("expected content");
        };
        let Widget::Row { children, .. } = &content[0] else {
            panic!("expected volume row");
        };
        assert!(
            matches!(&children[1], Widget::MeterSlider { readout, meter, .. } if readout == "−∞ dB" && meter.amplitude == 0.0)
        );
        for scale in [1.0, 2.0] {
            let mut scene = Scene::new();
            ui.render(
                &mut scene,
                &mut TextRenderer::new(),
                &Theme::default().widgets,
                scale,
            );
            assert!(!scene.encoding().resources.glyph_runs.is_empty());
            // Model the event loop's physical-to-logical pointer conversion.
            let physical = Point::new((size.width - 220.0) * scale, 102.0 * scale);
            ui.input(Input::PointerMoved(Point::new(
                physical.x / scale,
                physical.y / scale,
            )));
            let actions = ui.input(Input::PointerPressed);
            assert!(
                matches!(actions.as_slice(), [UiAction::SetTrackVolume { track_id, .. }] if *track_id == id)
            );
            ui.input(Input::PointerReleased);
            // Restore silence for the next pointer interaction.
            ui.set_view(view(&state), size);
        }
        state.tracks[0].deleting = true;
        ui.set_view(view(&state), size);
        assert!(ui.input(Input::Adjust(AdjustmentKey::End)).is_empty());
        ui.input(Input::PointerPressed);
        assert!(ui.input(Input::PointerReleased).is_empty());
        state.tracks[0].deleting = false;
    }
    update(&mut state, Action::Disconnected);
    let mut ui = Ui::new(view(&state), Size::new(400.0, 280.0));
    ui.input(Input::Tab { backwards: false });
    ui.input(Input::Tab { backwards: false }); // Only Back remains focusable.
    assert!(ui.input(Input::Adjust(AdjustmentKey::End)).is_empty());
}

#[test]
fn combined_meter_displays_post_gain_on_both_pages_and_preserves_stable_ids() {
    use crate::gui::{action::Action, update::update};
    use tinex_core::{TinexNotification, TinexRequest};
    let mut state = AppState::default();
    let effect = update(&mut state, Action::Ui(UiAction::CreateTrack))
        .effects
        .remove(0);
    let TinexRequest::NewTrack(track) = effect.request else {
        panic!("expected track");
    };
    let id = track.id();
    update(
        &mut state,
        Action::Notification(TinexNotification::TrackCreated(id)),
    );
    let slider_id = state.tracks[0].volume_slider;
    for (db, level, expected) in [
        (-61.0, f32::INFINITY, 0.0),
        (-60.0, 1.0, 0.001),
        (-6.0, 1.0, 10.0_f32.powf(-0.3)),
        (0.0, 1.0, 1.0),
        (6.0, 1.0, 10.0_f32.powf(0.3)),
        (12.0, 1.0, 10.0_f32.powf(0.6)),
        (0.0, f32::NAN, 0.0),
        (0.0, -1.0, 0.0),
        (0.0, f32::INFINITY, f32::INFINITY),
    ] {
        state.tracks[0].volume_db = db;
        state.tracks[0].output_level = level;
        for (page, width) in [(Page::Main, 144.0), (Page::Track(id), 240.0)] {
            state.page = page;
            let Widget::Column(page) = view(&state) else {
                panic!("expected page");
            };
            let Widget::Scroll { child, .. } = &page[1] else {
                panic!("expected scroll");
            };
            let Widget::Column(content) = &**child else {
                panic!("expected column");
            };
            let children = match &content[0] {
                Widget::Row { children, .. } => children,
                Widget::ClickableRow { children, .. } => {
                    let Widget::Column(rows) = &children[0] else {
                        panic!("expected track card");
                    };
                    let Widget::Row { children, .. } = &rows[0] else {
                        panic!("expected volume row");
                    };
                    children
                }
                _ => panic!("expected row"),
            };
            let Widget::MeterSlider {
                slider,
                meter,
                readout,
            } = &children[1]
            else {
                panic!("expected combined control");
            };
            assert_eq!(slider.id, slider_id);
            assert_eq!(slider.value, db);
            assert_eq!(slider.width, width);
            assert_eq!(slider.max, 12.0);
            assert_eq!(meter.amplitude, expected);
            assert_eq!(
                readout,
                &if db == -61.0 {
                    "−∞ dB".into()
                } else {
                    format!("{db:+.0} dB")
                }
            );
        }
    }
}

#[test]
fn main_page_volume_controls_target_each_track_and_never_navigate() {
    use crate::gui::{action::Action, update::update};
    use tinex_core::{TinexNotification, TinexRequest};
    use tinex_widgets::{AdjustmentKey, Input, Ui};
    use vello::kurbo::Point;
    let mut state = AppState::default();
    for _ in 0..2 {
        let effect = update(&mut state, Action::Ui(UiAction::CreateTrack))
            .effects
            .remove(0);
        let TinexRequest::NewTrack(track) = effect.request else {
            panic!("expected track");
        };
        update(
            &mut state,
            Action::Notification(TinexNotification::TrackCreated(track.id())),
        );
    }
    let first = state.tracks[0].id;
    let second = state.tracks[1].id;
    for size in [Size::new(400.0, 400.0), Size::new(800.0, 450.0)] {
        for scale in [1.0, 2.0] {
            state.tracks[0].volume_db = 0.0;
            state.tracks[1].volume_db = 0.0;
            let mut ui = Ui::new(view(&state), size);
            for _ in 0..3 {
                ui.input(Input::Tab { backwards: false });
            } // Create, row, volume.
            let actions = ui.input(Input::Adjust(AdjustmentKey::Increase));
            assert!(
                matches!(actions.as_slice(), [UiAction::SetTrackVolume { track_id, db: 1.0 }] if *track_id == first)
            );
            update(&mut state, Action::Ui(actions[0]));
            assert_eq!(state.page, Page::Main);
            assert_eq!(state.tracks[1].volume_db, 0.0);
            ui.set_view(view(&state), size);
            let physical = Point::new((size.width - 176.0) * scale, 270.0 * scale);
            ui.input(Input::PointerMoved(Point::new(
                physical.x / scale,
                physical.y / scale,
            )));
            let actions = ui.input(Input::PointerPressed);
            assert!(
                matches!(actions.as_slice(), [UiAction::SetTrackVolume { track_id, .. }] if *track_id == second)
            );
            let result = update(&mut state, Action::Ui(actions[0]));
            assert!(
                matches!(&result.effects[0].request, TinexRequest::SetTrackVolume(id, _) if *id == second)
            );
            assert_eq!(state.page, Page::Main);
            update(
                &mut state,
                Action::Notification(TinexNotification::OutputLevel {
                    output_level: 0.5,
                    tracks: [(first, 0.25), (second, 0.5)].into(),
                }),
            );
            ui.set_view(view(&state), size);
            // Capture stays on the second track even when dragging over the first.
            let actions = ui.input(Input::PointerMoved(Point::new(size.width, 102.0)));
            assert!(
                matches!(actions.as_slice(), [UiAction::SetTrackVolume { track_id, db: 12.0 }] if *track_id == second)
            );
            update(&mut state, Action::Ui(actions[0]));
            assert!(ui.input(Input::PointerReleased).is_empty());
            assert_eq!(state.tracks[0].volume_db, 1.0);
            assert_eq!(state.tracks[1].volume_db, 12.0);
            assert_eq!(state.page, Page::Main);
            state.tracks[1].deleting = true;
            ui.set_view(view(&state), size);
            ui.input(Input::PointerMoved(Point::new(size.width - 176.0, 270.0)));
            assert!(ui.input(Input::PointerPressed).is_empty());
            assert!(ui.input(Input::PointerReleased).is_empty());
            assert_eq!(state.page, Page::Main);
            state.tracks[1].deleting = false;
        }
    }
    update(&mut state, Action::Disconnected);
    let mut ui = Ui::new(view(&state), Size::new(400.0, 280.0));
    ui.input(Input::PointerMoved(Point::new(270.0, 110.0)));
    assert!(ui.input(Input::PointerPressed).is_empty());
    assert!(ui.input(Input::PointerReleased).is_empty());
    assert_eq!(state.page, Page::Main);
}

#[test]
fn plugin_insertion_controls_select_positions_with_pointer_and_keyboard() {
    use crate::gui::{action::Action, update::update};
    use tinex_core::{TinexNotification, TinexRequest};
    use tinex_widgets::{ActivationKey, Input, Ui};
    use vello::kurbo::Point;

    let mut state = confirmed_tracks(2);
    let track_id = state.tracks[0].id;
    for index in 0..2 {
        update(
            &mut state,
            Action::Ui(UiAction::OpenPlugins {
                track_id,
                index: u16::MAX,
            }),
        );
        let effect = update(
            &mut state,
            Action::Ui(UiAction::AddPlugin { track_id, index: 0 }),
        )
        .effects
        .remove(0);
        let TinexRequest::AddPlugin { plugin, .. } = effect.request else {
            panic!("expected plugin")
        };
        update(
            &mut state,
            Action::Notification(TinexNotification::PluginAdded {
                track_id,
                plugin_id: plugin.id(),
                index,
            }),
        );
    }
    let before = state.tracks[0].plugins.clone();
    for size in [Size::new(400.0, 480.0), Size::new(800.0, 450.0)] {
        for scale in [1.0, 2.0] {
            for (position, tabs, logical) in [
                (0, 6, Point::new(size.width - 92.0, 282.0)),
                (1, 8, Point::new(size.width - 92.0, 342.0)),
                (u16::MAX, 3, Point::new(80.0, 186.0)),
            ] {
                for keyboard in [false, true] {
                    state.page = Page::Track(track_id);
                    state.tracks[0].plugins = before.clone();
                    let mut ui = Ui::new(view(&state), size);
                    let actions = if keyboard {
                        for _ in 0..tabs {
                            ui.input(Input::Tab { backwards: false });
                        }
                        ui.input(Input::Key {
                            key: ActivationKey::Space,
                            pressed: true,
                            repeat: false,
                        });
                        ui.input(Input::Key {
                            key: ActivationKey::Space,
                            pressed: false,
                            repeat: false,
                        })
                    } else {
                        let physical = Point::new(logical.x * scale, logical.y * scale);
                        ui.input(Input::PointerMoved(Point::new(
                            physical.x / scale,
                            physical.y / scale,
                        )));
                        ui.input(Input::PointerPressed);
                        ui.input(Input::PointerReleased)
                    };
                    assert!(
                        matches!(actions.as_slice(), [UiAction::OpenPlugins { track_id: id, index }]
                        if *id == track_id && *index == position)
                    );
                    update(&mut state, Action::Ui(actions[0]));
                    assert_eq!(state.page, Page::Plugins(track_id, position));
                    let mut picker = Ui::new(view(&state), size);
                    picker.input(Input::Tab { backwards: false });
                    picker.input(Input::Tab { backwards: false });
                    picker.input(Input::Key {
                        key: ActivationKey::Enter,
                        pressed: true,
                        repeat: false,
                    });
                    let selection = picker.input(Input::Key {
                        key: ActivationKey::Enter,
                        pressed: false,
                        repeat: false,
                    });
                    let effect = update(&mut state, Action::Ui(selection[0]))
                        .effects
                        .remove(0);
                    let TinexRequest::AddPlugin {
                        track_id: id,
                        plugin,
                        index,
                    } = effect.request
                    else {
                        panic!("expected plugin addition")
                    };
                    assert_eq!((id, index), (track_id, position));
                    assert_eq!(state.page, Page::Track(track_id));
                    assert_eq!(state.tracks[0].plugins, before);
                    assert_eq!(state.tracks[0].pending_plugins.len(), 1);
                    let actual = position.min(before.len() as u16);
                    update(
                        &mut state,
                        Action::Notification(TinexNotification::PluginAdded {
                            track_id,
                            plugin_id: plugin.id(),
                            index: actual,
                        }),
                    );
                    assert_eq!(state.tracks[0].plugins[usize::from(actual)].id, plugin.id());
                    assert!(state.tracks[0].pending_plugins.is_empty());
                    assert!(state.tracks[1].plugins.is_empty());
                }
            }
        }
    }
}

#[test]
fn insertion_buttons_keep_identity_and_disable_for_unavailable_tracks() {
    use crate::gui::state::UiPluginInstance;
    use tinex_core::plugin::PluginInstanceId;
    use tinex_widgets::{ActivationKey, Input, Ui, WidgetId};
    use vello::kurbo::Point;

    let mut state = confirmed_tracks(1);
    let track_id = state.tracks[0].id;
    let plugin = UiPluginInstance {
        id: PluginInstanceId::new(),
        name: "EPiano",
        delete_button: WidgetId::new(),
        insert_before_button: WidgetId::new(),
        deleting: false,
    };
    state.tracks[0].plugins.push(plugin.clone());
    state.page = Page::Track(track_id);
    let size = Size::new(400.0, 480.0);
    let mut ui = Ui::new(view(&state), size);
    // Moving the same plugin in the chain changes its action's index while
    // its widget identity and keyboard focus survive the view rebuild.
    for _ in 0..6 {
        ui.input(Input::Tab { backwards: false });
    }
    ui.input(Input::Key {
        key: ActivationKey::Enter,
        pressed: true,
        repeat: false,
    });
    assert!(
        matches!(ui.input(Input::Key { key: ActivationKey::Enter, pressed: false, repeat: false }).as_slice(),
        [UiAction::OpenPlugins { track_id: id, index: 0 }] if *id == track_id)
    );
    let mut earlier = plugin.clone();
    earlier.id = PluginInstanceId::new();
    earlier.delete_button = WidgetId::new();
    earlier.insert_before_button = WidgetId::new();
    state.tracks[0].plugins.insert(0, earlier);
    ui.set_view(view(&state), size);
    ui.input(Input::Key {
        key: ActivationKey::Enter,
        pressed: true,
        repeat: false,
    });
    assert!(
        matches!(ui.input(Input::Key { key: ActivationKey::Enter, pressed: false, repeat: false }).as_slice(),
        [UiAction::OpenPlugins { track_id: id, index: 1 }] if *id == track_id)
    );
    for (connected, deleting) in [(true, true), (false, false)] {
        state.connected = connected;
        state.tracks[0].deleting = deleting;
        ui.set_view(view(&state), size);
        for y in [186.0, 282.0, 342.0] {
            let x = if y == 186.0 { 80.0 } else { 308.0 };
            ui.input(Input::PointerMoved(Point::new(x, y)));
            ui.input(Input::PointerPressed);
            assert!(ui.input(Input::PointerReleased).is_empty());
        }
    }
}

#[test]
fn scrolling_plugin_insertion_controls_preserves_footer_and_targets_visible_position() {
    use crate::gui::state::UiPluginInstance;
    use tinex_core::plugin::PluginInstanceId;
    use tinex_widgets::{Input, Ui, WidgetId};
    use vello::kurbo::Point;

    for size in [Size::new(400.0, 280.0), Size::new(800.0, 450.0)] {
        for scale in [1.0, 2.0] {
            for count in [0, 20] {
                let mut state = confirmed_tracks(1);
                let track_id = state.tracks[0].id;
                state.page = Page::Track(track_id);
                state.tracks[0].plugins = (0..count)
                    .map(|_| UiPluginInstance {
                        id: PluginInstanceId::new(),
                        name: "EPiano",
                        delete_button: WidgetId::new(),
                        insert_before_button: WidgetId::new(),
                        deleting: false,
                    })
                    .collect();
                let mut ui = Ui::new(view(&state), size);
                let mut text = TextRenderer::new();
                let theme = Theme::default();
                let mut scene = Scene::new();
                ui.render(&mut scene, &mut text, &theme.widgets, scale);
                let footer: Vec<_> = scene
                    .encoding()
                    .resources
                    .glyph_runs
                    .iter()
                    .rev()
                    .take(2)
                    .map(|run| run.transform)
                    .collect();
                assert_eq!(footer.len(), 2);
                assert!(
                    footer.iter().all(|transform| transform.translation[1]
                        == ((size.height - 26.0) * scale) as f32)
                );
                ui.input(Input::PointerMoved(Point::new(36.0, 100.0)));
                ui.input(Input::Scroll(10_000.0));
                ui.set_view(view(&state), size);
                scene.reset();
                ui.render(&mut scene, &mut text, &theme.widgets, scale);
                let scrolled_footer: Vec<_> = scene
                    .encoding()
                    .resources
                    .glyph_runs
                    .iter()
                    .rev()
                    .take(2)
                    .map(|run| run.transform)
                    .collect();
                assert_eq!(scrolled_footer, footer);
                ui.input(Input::PointerMoved(Point::new(
                    size.width - 92.0,
                    size.height - 16.0,
                )));
                ui.input(Input::PointerPressed);
                assert!(ui.input(Input::PointerReleased).is_empty());
                if count > 0 {
                    let physical =
                        Point::new((size.width - 92.0) * scale, (size.height - 66.0) * scale);
                    ui.input(Input::PointerMoved(Point::new(
                        physical.x / scale,
                        physical.y / scale,
                    )));
                    ui.input(Input::PointerPressed);
                    assert!(matches!(ui.input(Input::PointerReleased).as_slice(),
                        [UiAction::OpenPlugins { track_id: id, index: 19 }] if *id == track_id));
                }
            }
        }
    }
}

#[test]
fn plugin_delete_buttons_support_pointer_keyboard_and_pending_states() {
    use crate::gui::{action::Action, update::update};
    use tinex_core::{TinexNotification, TinexRequest};
    use tinex_widgets::{ActivationKey, Input, Ui};
    use vello::kurbo::Point;

    let mut state = confirmed_tracks(1);
    let track_id = state.tracks[0].id;
    update(
        &mut state,
        Action::Ui(UiAction::OpenPlugins {
            track_id,
            index: u16::MAX,
        }),
    );
    let effect = update(
        &mut state,
        Action::Ui(UiAction::AddPlugin { track_id, index: 0 }),
    )
    .effects
    .remove(0);
    let TinexRequest::AddPlugin { plugin, .. } = effect.request else {
        panic!("expected plugin")
    };
    let plugin_id = plugin.id();
    update(
        &mut state,
        Action::Notification(TinexNotification::PluginAdded {
            track_id,
            plugin_id,
            index: 0,
        }),
    );
    let size = Size::new(400.0, 480.0);
    let button_id = state.tracks[0].plugins[0].delete_button;
    let assert_button = |state: &AppState, expected_label: &str, expected_enabled: bool| {
        let Widget::Column(page) = view(state) else {
            panic!("expected page")
        };
        let Widget::Scroll { child, .. } = &page[1] else {
            panic!("expected scroll")
        };
        let Widget::Column(rows) = &**child else {
            panic!("expected content")
        };
        let Widget::Row { children, .. } = &rows[3] else {
            panic!("expected plugin row")
        };
        assert!(matches!(&children[1], Widget::Button {
            id, label, enabled, action: UiAction::DeletePlugin { track_id: track, plugin_id: instance }, ..
        } if *id == button_id && label == expected_label && *enabled == expected_enabled
            && *track == track_id && *instance == plugin_id));
    };
    assert_button(&state, "Delete", true);
    for key in [ActivationKey::Enter, ActivationKey::Space] {
        let mut ui = Ui::new(view(&state), size);
        for _ in 0..5 {
            // Back, volume, Add Plugin, Arm, Delete.
            ui.input(Input::Tab { backwards: false });
        }
        ui.input(Input::Key {
            key,
            pressed: true,
            repeat: false,
        });
        let actions = ui.input(Input::Key {
            key,
            pressed: false,
            repeat: false,
        });
        assert!(
            matches!(actions.as_slice(), [UiAction::DeletePlugin { track_id: track, plugin_id: instance }]
            if *track == track_id && *instance == plugin_id)
        );
    }
    let mut ui = Ui::new(view(&state), size);
    ui.input(Input::PointerMoved(Point::new(176.0, 282.0)));
    ui.input(Input::PointerPressed);
    let actions = ui.input(Input::PointerReleased);
    assert!(
        matches!(actions.as_slice(), [UiAction::DeletePlugin { track_id: track, plugin_id: instance }]
        if *track == track_id && *instance == plugin_id)
    );
    update(&mut state, Action::Ui(actions[0]));
    assert_button(&state, "Deleting…", false);
    ui.set_view(view(&state), size);
    ui.input(Input::PointerPressed);
    assert!(ui.input(Input::PointerReleased).is_empty());
    state.tracks[0].plugins[0].deleting = false;
    update(&mut state, Action::Ui(UiAction::DeleteTrack(track_id)));
    assert_button(&state, "Delete", false);
    ui.set_view(view(&state), size);
    ui.input(Input::PointerPressed);
    assert!(ui.input(Input::PointerReleased).is_empty());
    update(&mut state, Action::Disconnected);
    assert_button(&state, "Delete", false);
    ui.set_view(view(&state), size);
    ui.input(Input::PointerPressed);
    assert!(ui.input(Input::PointerReleased).is_empty());
}
