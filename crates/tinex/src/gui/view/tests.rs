use super::*;
use crate::gui::theme::Theme;
use tinex_widgets::TextRenderer;
use vello::Scene;
use vello::kurbo::Size;

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
        Page::Plugins(track.id()),
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
    let delete_position = Point::new(300.0, 100.0);
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

    for point in [Point::new(20.0, 100.0), Point::new(230.0, 100.0)] {
        let actions = click(&mut ui, point); // Track row or Open button.
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
    ui.input(Input::Tab { backwards: false }); // Volume.
    ui.input(Input::Tab { backwards: false }); // Open button.
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
    assert!(matches!(actions.as_slice(), [UiAction::OpenPlugins(track_id)] if *track_id == id));
    update(&mut state, Action::Ui(actions[0]));
    assert_eq!(state.page, Page::Plugins(id));
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

    update(&mut state, Action::Ui(UiAction::OpenPlugins(id)));
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
    assert!(!update(&mut state, Action::Ui(UiAction::OpenPlugins(id))).changed);
    update(
        &mut state,
        Action::Notification(TinexNotification::TrackCreated(id)),
    );
    update(&mut state, Action::Ui(UiAction::OpenPlugins(id)));
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
    assert!(!update(&mut state, Action::Ui(UiAction::OpenPlugins(id))).changed);
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
                Widget::Row { children, .. } | Widget::ClickableRow { children, .. } => children,
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
    for size in [Size::new(400.0, 280.0), Size::new(800.0, 450.0)] {
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
            let physical = Point::new((size.width - 296.0) * scale, 186.0 * scale);
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
            ui.input(Input::PointerMoved(Point::new(size.width - 296.0, 186.0)));
            assert!(ui.input(Input::PointerPressed).is_empty());
            assert!(ui.input(Input::PointerReleased).is_empty());
            assert_eq!(state.page, Page::Main);
            state.tracks[1].deleting = false;
        }
    }
    update(&mut state, Action::Disconnected);
    let mut ui = Ui::new(view(&state), Size::new(400.0, 280.0));
    ui.input(Input::PointerMoved(Point::new(104.0, 102.0)));
    assert!(ui.input(Input::PointerPressed).is_empty());
    assert!(ui.input(Input::PointerReleased).is_empty());
    assert_eq!(state.page, Page::Main);
}
