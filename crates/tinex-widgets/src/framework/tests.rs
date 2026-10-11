use super::*;

fn button(id: WidgetId, enabled: bool, action: u32) -> Widget<u32> {
    Widget::button(id, "Button", 120.0, enabled, action)
}

fn row(children: Vec<Widget<u32>>) -> Widget<u32> {
    Widget::Row {
        children,
        surface: true,
    }
}

fn button_rect(ui: &Ui<u32>, id: WidgetId) -> Rect {
    ui.items
        .iter()
        .find_map(|item| match item.kind {
            ItemKind::Button { id: found, .. } if found == id => Some(item.rect),
            _ => None,
        })
        .unwrap()
}

fn click(ui: &mut Ui<u32>, point: Point) -> Vec<u32> {
    ui.input(Input::PointerMoved(point));
    assert!(ui.input(Input::PointerPressed).is_empty());
    ui.input(Input::PointerReleased)
}

#[test]
fn percentage_meter_is_clamped_and_noninteractive() {
    for (input, expected) in [
        (-10.0, 0.0),
        (0.0, 0.0),
        (50.0, 50.0),
        (80.0, 80.0),
        (95.0, 95.0),
        (100.0, 100.0),
        (150.0, 100.0),
        (f64::NAN, 0.0),
        (f64::INFINITY, 100.0),
        (f64::NEG_INFINITY, 0.0),
    ] {
        let mut ui = Ui::<u32>::new(Widget::percentage_meter(input, 88.0), Size::new(88.0, 28.0));
        let item = &ui.items[0];
        assert!(matches!(item.kind, ItemKind::PercentageMeter(value) if value == expected));
        let center = item.rect.center();
        assert!(click(&mut ui, center).is_empty());
    }
}

#[test]
fn buttons_require_matching_press_and_release() {
    let id = WidgetId::new();
    let mut ui = Ui::new(row(vec![button(id, true, 42)]), Size::new(400.0, 84.0));
    let inside = button_rect(&ui, id).center();
    ui.input(Input::PointerMoved(inside));
    assert_eq!(ui.hovered, Some(id));
    assert!(ui.input(Input::PointerReleased).is_empty());
    assert_eq!(click(&mut ui, inside), [42]);
    ui.input(Input::PointerPressed);
    ui.input(Input::PointerMoved(Point::ZERO));
    assert!(ui.input(Input::PointerReleased).is_empty());
    ui.input(Input::PointerMoved(inside));
    ui.input(Input::PointerPressed);
    ui.input(Input::PointerLeft);
    assert!(ui.input(Input::PointerReleased).is_empty());
    ui.input(Input::PointerMoved(inside));
    ui.input(Input::PointerPressed);
    ui.input(Input::Cancel);
    assert!(ui.input(Input::PointerReleased).is_empty());
    assert!(ui.focus.is_none());
}

#[test]
fn clickable_rows_support_keyboard_and_pointer_without_intercepting_nested_controls() {
    let open = WidgetId::new();
    let delete = WidgetId::new();
    let make_view = |enabled, level| {
        Widget::clickable_row(
            open,
            vec![
                Widget::label("Track 1", Tone::Normal),
                Widget::level_meter(level, 144.0),
                button(delete, enabled, 2),
            ],
            true,
            1,
        )
    };
    let size = Size::new(500.0, 100.0);
    let mut ui = Ui::new(make_view(true, 0.0), size);
    let name = ui
        .items
        .iter()
        .find(|item| matches!(item.kind, ItemKind::Label { .. }))
        .unwrap()
        .rect
        .center();
    let meter = ui
        .items
        .iter()
        .find(|item| matches!(item.kind, ItemKind::LevelMeter(_)))
        .unwrap()
        .rect
        .center();
    let padding = Point::new(
        button_rect(&ui, open).x0 + 2.0,
        button_rect(&ui, open).center().y,
    );
    assert_eq!(click(&mut ui, name), [1]);
    assert_eq!(click(&mut ui, meter), [1]);
    assert_eq!(click(&mut ui, padding), [1]);
    let child = button_rect(&ui, delete).center();
    assert_eq!(click(&mut ui, child), [2]);
    ui.set_view(make_view(false, 0.0), size);
    assert!(click(&mut ui, child).is_empty()); // Disabled Delete must not fall through to Open.

    ui.input(Input::Cancel);
    ui.input(Input::Tab { backwards: false });
    assert_eq!(ui.focus, Some(open));
    for key in [ActivationKey::Enter, ActivationKey::Space] {
        ui.input(Input::Key {
            key,
            pressed: true,
            repeat: false,
        });
        assert_eq!(
            ui.input(Input::Key {
                key,
                pressed: false,
                repeat: false
            }),
            [1]
        );
    }
    ui.input(Input::PointerMoved(meter));
    ui.input(Input::PointerPressed);
    ui.set_view(make_view(false, 2.0), size);
    assert_eq!(ui.captured, Some(open));
    assert_eq!(ui.focus, Some(open));
    assert_eq!(ui.input(Input::PointerReleased), [1]);
    ui.input(Input::PointerPressed);
    ui.set_view(Widget::Column(vec![]), size);
    assert!(ui.input(Input::PointerReleased).is_empty());
}

#[test]
fn clickable_rows_obey_scroll_clipping_and_keyboard_reveal() {
    let scroll = WidgetId::new();
    let ids: Vec<_> = (0..10).map(|_| WidgetId::new()).collect();
    let view = Widget::Scroll {
        id: scroll,
        child: Box::new(Widget::Column(
            ids.iter()
                .enumerate()
                .map(|(i, id)| {
                    Widget::clickable_row(
                        *id,
                        vec![Widget::label(format!("Track {i}"), Tone::Normal)],
                        true,
                        i as u32,
                    )
                })
                .collect(),
        )),
    };
    let mut ui = Ui::new(view, Size::new(400.0, 180.0));
    assert!(ui.hit(button_rect(&ui, ids[9]).center()).is_none());
    ui.input(Input::Tab { backwards: true });
    assert_eq!(ui.focus, Some(ids[9]));
    assert!(ui.offsets[&scroll] > 0.0);
    let last = button_rect(&ui, ids[9]).center();
    assert_eq!(click(&mut ui, last), [9]);
    assert!(ui.hit(button_rect(&ui, ids[0]).center()).is_none());
}

#[test]
fn disabled_buttons_never_activate_or_receive_focus() {
    let id = WidgetId::new();
    let mut ui = Ui::new(row(vec![button(id, false, 1)]), Size::new(400.0, 84.0));
    let point = button_rect(&ui, id).center();
    assert!(click(&mut ui, point).is_empty());
    ui.input(Input::Tab { backwards: false });
    assert!(ui.focus.is_none());
    assert!(
        ui.input(Input::Key {
            key: ActivationKey::Enter,
            pressed: true,
            repeat: false
        })
        .is_empty()
    );
    assert!(
        ui.input(Input::Key {
            key: ActivationKey::Enter,
            pressed: false,
            repeat: false
        })
        .is_empty()
    );
}

#[test]
fn keyboard_focus_and_activation_ignore_repeats() {
    let a = WidgetId::new();
    let b = WidgetId::new();
    let mut ui = Ui::new(
        row(vec![button(a, true, 1), button(b, true, 2)]),
        Size::new(400.0, 84.0),
    );
    ui.input(Input::Tab { backwards: false });
    assert_eq!(ui.focus, Some(a));
    for key in [ActivationKey::Enter, ActivationKey::Space] {
        assert!(
            ui.input(Input::Key {
                key,
                pressed: true,
                repeat: false
            })
            .is_empty()
        );
        assert!(
            ui.input(Input::Key {
                key,
                pressed: true,
                repeat: true
            })
            .is_empty()
        );
        assert_eq!(
            ui.input(Input::Key {
                key,
                pressed: false,
                repeat: false
            }),
            [1]
        );
        assert!(
            ui.input(Input::Key {
                key,
                pressed: false,
                repeat: false
            })
            .is_empty()
        );
    }
    ui.input(Input::Tab { backwards: true });
    assert_eq!(ui.focus, Some(b));
    ui.input(Input::Key {
        key: ActivationKey::Space,
        pressed: true,
        repeat: false,
    });
    ui.input(Input::Cancel);
    assert!(
        ui.input(Input::Key {
            key: ActivationKey::Space,
            pressed: false,
            repeat: false
        })
        .is_empty()
    );
}

#[test]
fn stable_ids_preserve_focus_and_removal_cancels_capture() {
    let a = WidgetId::new();
    let b = WidgetId::new();
    let size = Size::new(400.0, 180.0);
    let mut ui = Ui::new(
        Widget::Column(vec![
            row(vec![button(a, true, 1)]),
            row(vec![button(b, true, 2)]),
        ]),
        size,
    );
    let point = button_rect(&ui, b).center();
    click(&mut ui, point);
    ui.set_view(Widget::Column(vec![row(vec![button(b, true, 2)])]), size);
    assert_eq!(ui.focus, Some(b));
    let point = button_rect(&ui, b).center();
    ui.input(Input::PointerMoved(point));
    ui.input(Input::PointerPressed);
    ui.set_view(Widget::Column(vec![]), size);
    assert!(ui.focus.is_none());
    assert!(ui.captured.is_none());
    assert!(ui.input(Input::PointerReleased).is_empty());
}

#[test]
fn disabling_captured_or_keyboard_pressed_widget_cancels_activation() {
    let id = WidgetId::new();
    let size = Size::new(400.0, 84.0);
    let mut ui = Ui::new(row(vec![button(id, true, 1)]), size);
    ui.input(Input::Tab { backwards: false });
    ui.input(Input::Key {
        key: ActivationKey::Enter,
        pressed: true,
        repeat: false,
    });
    ui.set_view(row(vec![button(id, false, 1)]), size);
    assert!(
        ui.input(Input::Key {
            key: ActivationKey::Enter,
            pressed: false,
            repeat: false
        })
        .is_empty()
    );
    ui.set_view(row(vec![button(id, true, 1)]), size);
    let point = button_rect(&ui, id).center();
    ui.input(Input::PointerMoved(point));
    ui.input(Input::PointerPressed);
    ui.set_view(row(vec![button(id, false, 1)]), size);
    assert!(ui.input(Input::PointerReleased).is_empty());
}

#[test]
fn scrolling_clips_hit_testing_clamps_offsets_and_reveals_keyboard_focus() {
    let header = WidgetId::new();
    let scroll = WidgetId::new();
    let ids: Vec<_> = (0..12).map(|_| WidgetId::new()).collect();
    let make_view = |count: usize| {
        Widget::Column(vec![
            row(vec![button(header, true, 99)]),
            Widget::Scroll {
                id: scroll,
                child: Box::new(Widget::Column(
                    ids.iter()
                        .take(count)
                        .enumerate()
                        .map(|(i, id)| row(vec![button(*id, true, i as u32)]))
                        .collect(),
                )),
            },
        ])
    };
    let size = Size::new(400.0, 240.0);
    let mut ui = Ui::new(make_view(12), size);
    assert!(ui.hit(button_rect(&ui, ids[11]).center()).is_none());
    let region = ui.regions[0].rect;
    ui.input(Input::PointerMoved(region.center()));
    ui.input(Input::Scroll(10_000.0));
    assert_eq!(ui.offsets[&scroll], ui.regions[0].max_offset);
    let point = button_rect(&ui, ids[11]).center();
    assert_eq!(click(&mut ui, point), [11]);
    assert_eq!(button_rect(&ui, header).y0, 24.0); // Header never scrolls.
    ui.input(Input::Tab { backwards: false }); // Wrap to header.
    ui.input(Input::Tab { backwards: false }); // Reveal first row.
    assert_eq!(ui.focus, Some(ids[0]));
    assert!(ui.hit(button_rect(&ui, ids[0]).center()).is_some());
    ui.set_view(make_view(1), size);
    assert_eq!(ui.offsets[&scroll], 0.0);
    assert!(click(&mut ui, Point::new(30.0, region.y1 + 4.0)).is_empty());
}

#[test]
fn meter_is_noninteractive_and_leaves_names_and_delete_buttons_usable() {
    let id = WidgetId::new();
    let mut ui = Ui::new(
        row(vec![
            Widget::label("Track 64", Tone::Normal),
            Widget::level_meter(2.0, 144.0),
            Widget::button(id, "Delete", 112.0, true, 1),
        ]),
        Size::new(400.0, 280.0),
    );
    let meter = ui
        .items
        .iter()
        .find(|item| matches!(item.kind, ItemKind::LevelMeter(_)))
        .unwrap();
    let point = meter.rect.center();
    assert_eq!(meter.rect.width(), 144.0);
    assert_eq!(meter.rect.height(), LevelMeter::HEIGHT);
    assert!(click(&mut ui, point).is_empty());
    ui.input(Input::Tab { backwards: false });
    assert_eq!(ui.focus, Some(id));
    ui.input(Input::Tab { backwards: true });
    assert_eq!(ui.focus, Some(id)); // Only the Delete button can receive focus.
    let rect = button_rect(&ui, id);
    assert_eq!(rect.width(), 112.0);
    assert_eq!(rect.height(), 36.0);
    assert_eq!(click(&mut ui, rect.center()), [1]);
    let label = ui
        .items
        .iter()
        .find(|item| matches!(item.kind, ItemKind::Label { .. }))
        .unwrap();
    assert!(label.rect.width() >= 80.0);
}

#[test]
fn meter_updates_preserve_scroll_focus_and_pointer_capture() {
    let scroll = WidgetId::new();
    let ids: Vec<_> = (0..12).map(|_| WidgetId::new()).collect();
    let make_view = |level| Widget::Scroll {
        id: scroll,
        child: Box::new(Widget::Column(
            ids.iter()
                .enumerate()
                .map(|(i, id)| {
                    row(vec![
                        Widget::label(format!("Track {i}"), Tone::Normal),
                        Widget::level_meter(level, 144.0),
                        Widget::button(*id, "Delete", 112.0, true, i as u32),
                    ])
                })
                .collect(),
        )),
    };
    let size = Size::new(400.0, 280.0);
    let mut ui = Ui::new(make_view(0.0), size);
    ui.input(Input::PointerMoved(Point::new(100.0, 100.0)));
    ui.input(Input::Scroll(10_000.0));
    let offset = ui.offsets[&scroll];
    let point = button_rect(&ui, ids[11]).center();
    ui.input(Input::PointerMoved(point));
    ui.input(Input::PointerPressed);
    ui.set_view(make_view(2.0), size);
    assert_eq!(ui.offsets[&scroll], offset);
    assert_eq!(ui.focus, Some(ids[11]));
    assert_eq!(ui.captured, Some(ids[11]));
    assert_eq!(ui.input(Input::PointerReleased), [11]);
    ui.input(Input::Tab { backwards: false });
    assert_eq!(ui.focus, Some(ids[0]));
    assert_eq!(ui.hit(button_rect(&ui, ids[0]).center()), Some(ids[0]));
}

#[test]
fn status_bar_stays_at_bottom_outside_the_scrolling_content() {
    let scroll = WidgetId::new();
    let header = WidgetId::new();
    let ids: Vec<_> = (0..20).map(|_| WidgetId::new()).collect();
    let make_view = |count: usize, status: &str, level| {
        Widget::Column(vec![
            row(vec![button(header, true, 100)]),
            Widget::Scroll {
                id: scroll,
                child: Box::new(Widget::Column(
                    ids.iter()
                        .take(count)
                        .enumerate()
                        .map(|(i, id)| {
                            row(vec![
                                Widget::label(format!("Track {i}"), Tone::Normal),
                                button(*id, true, i as u32),
                            ])
                        })
                        .collect(),
                )),
            },
            Widget::StatusBar(vec![
                Widget::label(status, Tone::Muted),
                Widget::fixed_label("Master", Tone::Muted, 56.0),
                Widget::compact_level_meter(level, 176.0),
            ]),
        ])
    };
    for size in [
        Size::new(400.0, 280.0),
        Size::new(800.0, 450.0),
        Size::new(600.0, 700.0),
    ] {
        for count in [0, 20] {
            let mut ui = Ui::new(make_view(count, "Ready", 0.0), size);
            let footer = ui
                .items
                .iter()
                .rev()
                .find(|item| matches!(item.kind, ItemKind::StatusSurface))
                .unwrap()
                .rect;
            assert_eq!(footer.y1, size.height);
            assert_eq!(footer.x0, 0.0);
            assert_eq!(footer.x1, size.width);
            assert_eq!(button_rect(&ui, header).y0, 24.0);
            assert_eq!(footer.height(), 32.0);
            assert!(ui.regions[0].rect.y1 <= footer.y0);
            let master = ui
                .items
                .iter()
                .find(|item| matches!(&item.kind, ItemKind::Label { text, .. } if text == "Master"))
                .unwrap();
            assert_eq!(master.rect.width(), 56.0);
            let meter = ui
                .items
                .iter()
                .find(|item| matches!(item.kind, ItemKind::CompactLevelMeter(_)))
                .unwrap()
                .rect;
            assert_eq!(meter.width(), 176.0);
            assert!(click(&mut ui, meter.center()).is_empty());
            let status = ui
                .items
                .iter()
                .find(|item| matches!(&item.kind, ItemKind::Label { text, .. } if text == "Ready"))
                .unwrap();
            assert!(status.rect.width() >= 136.0); // Enough room at the minimum window size.
            ui.input(Input::PointerMoved(ui.regions[0].rect.center()));
            ui.input(Input::Scroll(10_000.0));
            let offset = ui.offsets[&scroll];
            if count > 0 {
                assert!(offset > 0.0);
            }
            ui.set_view(make_view(count, "Creating Track…", 2.0), size);
            let updated_footer = ui
                .items
                .iter()
                .rev()
                .find(|item| matches!(item.kind, ItemKind::StatusSurface))
                .unwrap();
            assert_eq!(updated_footer.rect, footer);
            assert_eq!(ui.offsets[&scroll], offset);
            assert!(
                ui.items
                    .iter()
                    .filter(|item| item.scroll == Some(scroll))
                    .all(|item| item.clip.y1 <= footer.y0)
            );
            ui.input(Input::PointerMoved(meter.center()));
            ui.input(Input::Scroll(-1000.0));
            assert_eq!(ui.offsets[&scroll], offset); // Footer scrolling doesn't affect the list.
        }
    }
}

#[test]
fn narrow_layout_keeps_rectangles_nonnegative() {
    let id = WidgetId::new();
    let ui = Ui::new(
        row(vec![
            Widget::label("Name", Tone::Normal),
            Widget::level_meter(f32::INFINITY, 144.0),
            button(id, true, 1),
        ]),
        Size::new(0.0, 0.0),
    );
    for item in &ui.items {
        assert!(item.rect.width() >= 0.0);
        assert!(item.rect.height() >= 0.0);
    }
}
