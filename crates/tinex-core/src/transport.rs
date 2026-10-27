#[derive(Copy, Clone, Debug)]
pub struct Transport {
    pub bpm: f32,
    pub playing: bool,
    pub beat: Beat,
    pub time_signature: TimeSignature,
    /// Length of the arrangement in measures.
    pub measures: u32,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct TimeSignature {
    /// Beats per measure.
    pub numerator: u8,
    /// Note value that receives one beat, such as 4 for a quarter note.
    pub denominator: u8,
}

impl Default for TimeSignature {
    fn default() -> TimeSignature {
        TimeSignature {
            numerator: 4,
            denominator: 4,
        }
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub struct Beat(f64);

impl Beat {
    /// The start of the arrangement.
    pub const ZERO: Beat = Beat(0.0);

    /// Beats elapsed since the start of the arrangement.
    pub fn as_f64(self) -> f64 {
        self.0
    }
}

impl Default for Transport {
    fn default() -> Transport {
        Transport {
            bpm: 120.0,
            playing: false,
            beat: Beat::ZERO,
            time_signature: TimeSignature::default(),
            measures: 4,
        }
    }
}

impl Transport {
    pub(crate) fn compute_beat_at_frame<'a>(
        &mut self,
        samples: usize,
        sample_rate: u32,
        arena: &'a bumpalo::Bump,
    ) -> &'a [Beat] {
        if !self.playing {
            return arena.alloc_slice_fill_copy(samples + 1, self.beat);
        }
        let step = f64::from(self.bpm) / 60.0 / f64::from(sample_rate);
        let length = self.length_in_beats();
        // An empty arrangement has nowhere to loop back to. `rem_euclid` keeps negative tempos
        // looping back to the end instead of going below zero.
        let wrap = |beat: f64| if length > 0.0 { beat.rem_euclid(length) } else { beat };
        // The arrangement may have shrunk since the last block, leaving the beat past the end.
        self.beat = Beat(wrap(self.beat.0));
        arena.alloc_slice_fill_with(samples + 1, |idx| {
            if idx > 0 {
                self.beat = Beat(wrap(self.beat.0 + step));
            }
            self.beat
        })
    }

    /// Length of the arrangement, counting one beat per time-signature numerator unit.
    fn length_in_beats(&self) -> f64 {
        f64::from(self.measures) * f64::from(self.time_signature.numerator)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A playing transport at 90 bpm. At 8 Hz each frame advances 0.1875 beats.
    fn playing_at_90_bpm() -> Transport {
        Transport {
            bpm: 90.0,
            playing: true,
            ..Transport::default()
        }
    }

    /// A playing transport whose arrangement is 2 bars of 3/4, or 6 beats. At 60 bpm and
    /// 2 Hz each frame advances half a beat.
    fn playing_six_beat_arrangement(beat: f64) -> Transport {
        Transport {
            bpm: 60.0,
            playing: true,
            beat: Beat(beat),
            time_signature: TimeSignature {
                numerator: 3,
                denominator: 4,
            },
            measures: 2,
        }
    }

    #[test]
    fn playing_advances_each_frame_by_tempo() {
        let arena = bumpalo::Bump::new();
        let mut transport = playing_at_90_bpm();
        let beats = transport.compute_beat_at_frame(4, 8, &arena);
        assert_eq!(
            beats,
            [0.0, 0.1875, 0.375, 0.5625, 0.75].map(Beat).as_slice()
        );
    }

    #[test]
    fn playing_stores_last_beat_in_transport() {
        let arena = bumpalo::Bump::new();
        let mut transport = playing_at_90_bpm();
        transport.compute_beat_at_frame(4, 8, &arena);
        assert_eq!(transport.beat, Beat(0.75));
    }

    #[test]
    fn consecutive_blocks_continue_from_previous_block() {
        let arena = bumpalo::Bump::new();
        let mut transport = playing_at_90_bpm();
        transport.compute_beat_at_frame(4, 8, &arena);
        let beats = transport.compute_beat_at_frame(1, 8, &arena);
        assert_eq!(beats, [Beat(0.75), Beat(0.9375)].as_slice());
    }

    #[test]
    fn playing_with_zero_samples_returns_current_beat_only() {
        let arena = bumpalo::Bump::new();
        let mut transport = Transport {
            beat: Beat(1.5),
            ..playing_at_90_bpm()
        };
        let beats = transport.compute_beat_at_frame(0, 8, &arena);
        assert_eq!(beats, [Beat(1.5)].as_slice());
        assert_eq!(transport.beat, Beat(1.5));
    }

    #[test]
    fn stopped_holds_current_beat() {
        let arena = bumpalo::Bump::new();
        let mut transport = Transport {
            playing: false,
            beat: Beat(1.5),
            ..playing_at_90_bpm()
        };
        let beats = transport.compute_beat_at_frame(2, 8, &arena);
        assert_eq!(beats, [Beat(1.5); 3].as_slice());
        assert_eq!(transport.beat, Beat(1.5));
    }

    #[test]
    fn reaching_arrangement_end_wraps_to_zero() {
        let arena = bumpalo::Bump::new();
        let mut transport = playing_six_beat_arrangement(5.0);
        let beats = transport.compute_beat_at_frame(3, 2, &arena);
        assert_eq!(beats, [5.0, 5.5, 0.0, 0.5].map(Beat).as_slice());
    }

    #[test]
    fn starting_past_arrangement_end_wraps_first_beat() {
        let arena = bumpalo::Bump::new();
        let mut transport = playing_six_beat_arrangement(10.0);
        let beats = transport.compute_beat_at_frame(3, 2, &arena);
        assert_eq!(beats, [4.0, 4.5, 5.0, 5.5].map(Beat).as_slice());
    }

    #[test]
    fn negative_tempo_wraps_before_zero_to_arrangement_end() {
        let arena = bumpalo::Bump::new();
        let mut transport = Transport {
            bpm: -60.0,
            ..playing_six_beat_arrangement(0.5)
        };
        let beats = transport.compute_beat_at_frame(3, 2, &arena);
        assert_eq!(beats, [0.5, 0.0, 5.5, 5.0].map(Beat).as_slice());
    }

    #[test]
    fn zero_measures_never_wraps() {
        let arena = bumpalo::Bump::new();
        let mut transport = Transport {
            measures: 0,
            ..playing_six_beat_arrangement(5.0)
        };
        let beats = transport.compute_beat_at_frame(3, 2, &arena);
        assert_eq!(beats, [5.0, 5.5, 6.0, 6.5].map(Beat).as_slice());
    }
}
