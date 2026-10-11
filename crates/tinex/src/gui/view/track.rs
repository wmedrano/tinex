use tinex_widgets::{Tone, Widget};

use super::super::action::UiAction;
use super::super::state::{AppState, TrackRow};

pub(super) fn track_view(state: &AppState, track: &TrackRow) -> Vec<Widget<UiAction>> {
    let mut content = vec![
        Widget::Row {
            children: vec![
                Widget::label("Output level", Tone::Normal),
                Widget::level_meter(track.output_level, 144.0),
            ],
            surface: true,
        },
        Widget::Row {
            children: vec![Widget::button(
                state.add_plugin_button,
                "Add Plugin",
                144.0,
                state.connected && !track.deleting,
                UiAction::OpenPlugins(track.id),
            )],
            surface: false,
        },
        Widget::label("Plugins", Tone::Normal),
    ];
    if track.plugins.is_empty() {
        content.push(Widget::label("No plugins on this track yet.", Tone::Muted));
    } else {
        content.extend(
            track
                .plugins
                .iter()
                .enumerate()
                .map(|(index, name)| Widget::Row {
                    children: vec![Widget::label(
                        format!("{}. {name}", index + 1),
                        Tone::Normal,
                    )],
                    surface: true,
                }),
        );
    }
    if !track.pending_plugins.is_empty() {
        content.push(Widget::label("Adding plugin…", Tone::Muted));
    }
    vec![
        Widget::Row {
            children: vec![
                Widget::button(
                    state.back_button,
                    "Back to Tracks",
                    160.0,
                    true,
                    UiAction::BackToMain,
                ),
                Widget::label(&track.name, Tone::Normal),
            ],
            surface: false,
        },
        Widget::Scroll {
            id: state.track_page,
            child: Box::new(Widget::Column(content)),
        },
    ]
}
