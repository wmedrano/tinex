use tinex_widgets::{Tone, Widget};

use super::super::action::UiAction;
use super::super::state::{AppState, UiTrack};

pub(super) fn track_view(state: &AppState, track: &UiTrack) -> Vec<Widget<UiAction>> {
    let mut content = vec![
        Widget::Row {
            children: vec![
                Widget::label("Volume / output", Tone::Normal),
                super::track_volume_control(state, track, 240.0),
            ],
            surface: true,
        },
        Widget::Row {
            children: vec![
                Widget::button(
                    state.add_plugin_button,
                    "Add Plugin",
                    144.0,
                    state.connected && !track.deleting,
                    UiAction::OpenPlugins(track.id),
                ),
                super::track_arm_control(state, track),
            ],
            surface: false,
        },
        Widget::label("Plugins", Tone::Normal),
    ];
    if track.plugins.is_empty() {
        content.push(Widget::label("No plugins on this track yet.", Tone::Muted));
    } else {
        content.extend(track.plugins.iter().map(|plugin| Widget::Row {
            children: vec![
                Widget::label(plugin.name, Tone::Normal),
                Widget::button(
                    plugin.delete_button,
                    if plugin.deleting {
                        "Deleting…"
                    } else {
                        "Delete"
                    },
                    112.0,
                    state.connected && !track.deleting && !plugin.deleting,
                    UiAction::DeletePlugin {
                        track_id: track.id,
                        plugin_id: plugin.id,
                    },
                ),
            ],
            surface: true,
        }));
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
