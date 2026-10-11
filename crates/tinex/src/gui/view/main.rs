use tinex_widgets::{Tone, Widget};

use super::super::action::UiAction;
use super::super::state::AppState;

pub(super) fn main_view(state: &AppState) -> Vec<Widget<UiAction>> {
    let mut children = vec![Widget::Row {
        children: vec![Widget::button(
            state.create_button,
            "Create Track",
            144.0,
            state.connected,
            UiAction::CreateTrack,
        )],
        surface: false,
    }];
    let rows = if state.tracks.is_empty() {
        vec![Widget::label(
            "No tracks yet. Create a track to get started.",
            Tone::Muted,
        )]
    } else {
        state
            .tracks
            .iter()
            .map(|track| {
                Widget::clickable_row(
                    track.row,
                    vec![
                        Widget::label(&track.name, Tone::Normal),
                        Widget::level_meter(track.output_level, 144.0),
                        Widget::button(
                            track.open_button,
                            "Open",
                            72.0,
                            true,
                            UiAction::OpenTrack(track.id),
                        ),
                        Widget::button(
                            track.delete_button,
                            if track.deleting {
                                "Deleting…"
                            } else {
                                "Delete"
                            },
                            112.0,
                            state.connected && !track.deleting,
                            UiAction::DeleteTrack(track.id),
                        ),
                    ],
                    true,
                    UiAction::OpenTrack(track.id),
                )
            })
            .collect()
    };
    children.push(Widget::Scroll {
        id: state.track_list,
        child: Box::new(Widget::Column(rows)),
    });
    children
}
