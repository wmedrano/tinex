use wmidi::MidiMessage;

use crate::Beat;

const INITIAL_SEQUENCE_CAPACITY: usize = 4096;

#[derive(Clone, PartialEq, Debug)]
pub struct MidiEvent {
    pub beat: Beat,
    pub message: MidiMessage<'static>,
}

/// Recorded MIDI events for a track, kept sorted by [`Beat`].
#[derive(Clone, Debug)]
pub struct MidiSequence {
    events: Vec<MidiEvent>,
}

impl MidiSequence {
    pub fn new() -> Self {
        Self {
            events: Vec::with_capacity(INITIAL_SEQUENCE_CAPACITY),
        }
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub fn events(&self) -> &[MidiEvent] {
        &self.events
    }

    pub fn clear(&mut self) {
        self.events.clear();
    }

    /// Inserts an event at `beat`, preserving recording order for simultaneous beats.
    pub fn insert(&mut self, beat: Beat, message: MidiMessage<'static>) {
        if !beat.as_f64().is_finite() || beat < Beat::ZERO {
            return;
        }
        let index = self.events.partition_point(|event| event.beat <= beat);
        self.events.insert(index, MidiEvent { beat, message });
    }

    /// Merges recorded events falling within each frame's beat span with `live_midi`
    /// in frame order. Recorded events at a frame precede live events at that frame.
    pub(crate) fn collect_events(
        &self,
        beats: &[Beat],
        live_midi: &[(usize, MidiMessage<'static>)],
        out: &mut Vec<(usize, MidiMessage<'static>)>,
    ) {
        out.clear();
        let samples = beats.len().saturating_sub(1);
        if self.events.is_empty() || samples == 0 || beats[0] == beats[1] {
            out.extend_from_slice(live_midi);
            return;
        }
        let mut cursor = self.events.partition_point(|event| event.beat < beats[0]);
        let mut live = live_midi.iter().peekable();
        for frame in 0..samples {
            let start = beats[frame];
            let end = beats[frame + 1];
            if start < end {
                while cursor < self.events.len() && self.events[cursor].beat < end {
                    out.push((frame, self.events[cursor].message.clone()));
                    cursor += 1;
                }
            } else if end < start {
                while cursor < self.events.len() {
                    out.push((frame, self.events[cursor].message.clone()));
                    cursor += 1;
                }
                cursor = 0;
                while cursor < self.events.len() && self.events[cursor].beat < end {
                    out.push((frame, self.events[cursor].message.clone()));
                    cursor += 1;
                }
            }
            while let Some((offset, message)) = live.peek() {
                if *offset > frame {
                    break;
                }
                out.push((*offset, message.clone()));
                live.next();
            }
        }
        for (offset, message) in live {
            out.push((*offset, message.clone()));
        }
    }

    /// Overdubs `midi_input` into the sequence at each event's frame beat.
    pub(crate) fn record_events(
        &mut self,
        beats: &[Beat],
        midi_input: &[(usize, MidiMessage<'static>)],
    ) {
        let samples = beats.len().saturating_sub(1);
        if samples == 0 || beats[0] == beats[1] {
            return;
        }
        for (frame, message) in midi_input {
            if let Some(&beat) = beats.get(..samples).and_then(|slice| slice.get(*frame)) {
                self.insert(beat, message.clone());
            }
        }
    }
}

impl Default for MidiSequence {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wmidi::{Channel, Note, U7};

    fn note_on(note: Note) -> MidiMessage<'static> {
        MidiMessage::NoteOn(Channel::Ch1, note, U7::try_from(100).unwrap())
    }

    fn note_off(note: Note) -> MidiMessage<'static> {
        MidiMessage::NoteOff(Channel::Ch1, note, U7::try_from(0).unwrap())
    }

    #[test]
    fn insert_keeps_events_sorted_by_beat_and_fifo_at_equal_beats() {
        let mut sequence = MidiSequence::new();
        assert!(sequence.is_empty());
        sequence.insert(Beat(2.0), note_on(Note::E4));
        sequence.insert(Beat(0.5), note_on(Note::C4));
        sequence.insert(Beat(2.0), note_on(Note::G4));
        sequence.insert(Beat(0.5), note_off(Note::C4));
        sequence.insert(Beat(-1.0), note_on(Note::A4));
        sequence.insert(Beat(f64::NAN), note_on(Note::B4));

        assert_eq!(
            sequence.events(),
            &[
                MidiEvent {
                    beat: Beat(0.5),
                    message: note_on(Note::C4),
                },
                MidiEvent {
                    beat: Beat(0.5),
                    message: note_off(Note::C4),
                },
                MidiEvent {
                    beat: Beat(2.0),
                    message: note_on(Note::E4),
                },
                MidiEvent {
                    beat: Beat(2.0),
                    message: note_on(Note::G4),
                },
            ]
        );
    }

    #[test]
    fn collect_events_maps_beats_to_frames_across_loop_wraps_and_merges_live_midi() {
        let mut sequence = MidiSequence::new();
        sequence.insert(Beat(0.0), note_on(Note::C4));
        sequence.insert(Beat(0.2), note_on(Note::D4));
        sequence.insert(Beat(3.8), note_on(Note::E4));

        let mut out = Vec::new();
        // Frame 0: [3.5, 3.8), Frame 1: [3.8, 4.0) U [0.0, 0.1), Frame 2: [0.1, 0.4)
        let beats = [3.5, 3.8, 0.1, 0.4].map(Beat);
        let live = [(1, note_on(Note::G4)), (2, note_off(Note::C4))];
        sequence.collect_events(&beats, &live, &mut out);

        assert_eq!(
            out,
            vec![
                (1, note_on(Note::E4)),
                (1, note_on(Note::C4)),
                (1, note_on(Note::G4)),
                (2, note_on(Note::D4)),
                (2, note_off(Note::C4)),
            ]
        );
    }
}
