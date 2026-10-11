use tinex_widgets::{Tone, Widget};

use super::super::action::UiAction;
use super::super::state::{AppState, TrackRow};

pub(super) fn plugin_view(state: &AppState, track: &TrackRow) -> Vec<Widget<UiAction>> {
    vec![
        Widget::Row {
            children: vec![
                Widget::button(
                    state.plugin_back_button,
                    "Back to Track",
                    160.0,
                    true,
                    UiAction::OpenTrack(track.id),
                ),
                Widget::label(format!("Add Plugin · {}", track.name), Tone::Normal),
            ],
            surface: false,
        },
        Widget::Scroll {
            id: state.plugin_page,
            child: Box::new(Widget::Column(
                tinex_plugins::FACTORY
                    .iter()
                    .enumerate()
                    .map(|(index, builder)| Widget::Row {
                        children: vec![Widget::button(
                            state.plugin_buttons[index],
                            builder.name(),
                            200.0,
                            state.connected && !track.deleting,
                            UiAction::AddPlugin {
                                track_id: track.id,
                                index,
                            },
                        )],
                        surface: true,
                    })
                    .collect(),
            )),
        },
    ]
}
