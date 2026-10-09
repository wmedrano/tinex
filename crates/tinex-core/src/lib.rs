//! Tinex audio processing engine and plugin interfaces.
use std::{
    collections::HashMap,
    sync::mpsc::{Receiver, Sender},
};

use rayon::prelude::*;
use wmidi::MidiMessage;

use crate::{
    id::Id,
    track::{Track, TrackId},
};

pub mod id;
pub mod plugin;
pub mod track;

const TRACK_CAPACITY: usize = 64;
/// Meter smoothing time constant in seconds; silence falls by 90% in about 230 ms.
const OUTPUT_LEVEL_TIME_CONSTANT: f64 = 0.1;

fn output_level_weight(frames: usize, sample_rate: u32) -> f64 {
    if sample_rate == 0 {
        return 0.0;
    }
    let seconds = frames as f64 / f64::from(sample_rate);
    -(-seconds / OUTPUT_LEVEL_TIME_CONSTANT).exp_m1()
}

pub struct Tinex {
    tracks: Vec<Track>,
    output_level: f64,
    requests: Receiver<TinexRequest>,
    notifications: Sender<TinexNotification>,
}

#[allow(unused)]
pub enum TinexRequest {
    NewTrack(Track),
    AddPlugin {
        track_id: TrackId,
        plugin: Box<dyn plugin::Plugin>,
    },
    DeleteTrack(Id<Track>),
    /// Updates the supplied track entries with exponentially weighted block peak levels.
    /// Missing tracks receive zero; the response returns the same map allocation.
    OutputLevel(HashMap<TrackId, f32>),
}

#[derive(Debug)]
#[allow(unused)]
pub enum TinexNotification {
    TrackCreated(Id<Track>),
    TrackCreationFailed(Track),
    TrackDeleted(Track),
    OutputLevel {
        output_level: f32,
        tracks: HashMap<TrackId, f32>,
    },
}

pub struct ProcessArgs<'a, 'out> {
    /// Current audio sample rate in Hz, used for meter smoothing.
    pub sample_rate: u32,
    pub input: [&'a [f32]; 2],
    pub midi_input: &'a [(usize, MidiMessage<'static>)],
    pub output: [&'out mut [f32]; 2],
    pub _arena: &'a bumpalo::Bump,
}

impl Tinex {
    pub fn new(requests: Receiver<TinexRequest>, notifications: Sender<TinexNotification>) -> Self {
        // Start Rayon before the first audio callback needs its workers.
        let _ = rayon::current_num_threads();
        Self {
            tracks: Vec::with_capacity(TRACK_CAPACITY),
            output_level: 0.0,
            requests,
            notifications,
        }
    }

    /// Handles pending requests, processes audio, and updates the weighted peak output level.
    pub fn process(&mut self, mut args: ProcessArgs<'_, '_>) {
        self.handle_requests();
        self.process_tracks(&mut args);
        self.update_output_level(&args);
    }

    fn handle_requests(&mut self) {
        for request in self.requests.try_iter() {
            match request {
                TinexRequest::NewTrack(track) => {
                    // Zero-sized tracks give Vec a capacity of usize::MAX.
                    if self.tracks.len() >= self.tracks.capacity().min(TRACK_CAPACITY) {
                        let _ = self
                            .notifications
                            .send(TinexNotification::TrackCreationFailed(track));
                        continue;
                    }
                    let id = track.id();
                    self.tracks.push(track);
                    let _ = self.notifications.send(TinexNotification::TrackCreated(id));
                }
                TinexRequest::AddPlugin { track_id, plugin } => {
                    if let Some(track) = self.tracks.iter_mut().find(|track| track.id() == track_id)
                    {
                        track.push_boxed_plugin(plugin);
                    }
                }
                TinexRequest::OutputLevel(mut tracks) => {
                    for (id, level) in tracks.iter_mut() {
                        *level = self
                            .tracks
                            .iter()
                            .find(|track| track.id() == *id)
                            .map_or(0.0, Track::output_level);
                    }
                    let _ = self.notifications.send(TinexNotification::OutputLevel {
                        output_level: self.output_level as f32,
                        tracks,
                    });
                }
                TinexRequest::DeleteTrack(id) => {
                    if let Some(index) = self.tracks.iter().position(|track| track.id() == id) {
                        let track = self.tracks.remove(index);
                        let _ = self
                            .notifications
                            .send(TinexNotification::TrackDeleted(track));
                    }
                }
            }
        }
    }

    fn process_tracks(&mut self, args: &mut ProcessArgs<'_, '_>) {
        for channel in args.output.iter_mut() {
            channel.fill(0.0);
        }
        let worker_count = rayon::current_num_threads();
        if self.tracks.len() < 2 || worker_count == 1 {
            self.process_tracks_serial(args);
        } else {
            self.process_tracks_parallel(args, worker_count);
        }
    }

    fn process_tracks_serial(&mut self, args: &mut ProcessArgs<'_, '_>) {
        let mut buffers: [_; 4] = std::array::from_fn(|index| {
            args._arena
                .alloc_slice_fill_copy(args.output[index % 2].len(), 0.0)
        });
        for track in self.tracks.iter_mut() {
            let output = track.process(
                args.midi_input,
                args.input,
                buffers.each_mut().map(|buffer| &mut **buffer),
                args.sample_rate,
            );
            for (output, samples) in args.output.iter_mut().zip(output) {
                for (output, sample) in output.iter_mut().zip(samples.iter()) {
                    *output += sample;
                }
            }
        }
    }

    fn process_tracks_parallel(&mut self, args: &mut ProcessArgs<'_, '_>, worker_count: usize) {
        let chunk_size = self.tracks.len().div_ceil(worker_count);
        let chunk_count = self.tracks.len().div_ceil(chunk_size);
        let mut buffers = bumpalo::collections::Vec::with_capacity_in(chunk_count, args._arena);
        for _ in 0..chunk_count {
            // Four channels for Track::process and two for this chunk's mix.
            buffers.push(std::array::from_fn::<_, 6, _>(|index| {
                args._arena
                    .alloc_slice_fill_copy(args.output[index % 2].len(), 0.0)
            }));
        }
        let midi_input = args.midi_input;
        let input = args.input;
        let sample_rate = args.sample_rate;
        self.tracks
            .par_chunks_mut(chunk_size)
            .zip(buffers.par_iter_mut())
            .for_each(|(tracks, buffers)| {
                let [
                    left,
                    right,
                    scratch_left,
                    scratch_right,
                    mix_left,
                    mix_right,
                ] = buffers;
                for track in tracks {
                    let output = track.process(
                        midi_input,
                        input,
                        [
                            &mut **left,
                            &mut **right,
                            &mut **scratch_left,
                            &mut **scratch_right,
                        ],
                        sample_rate,
                    );
                    for (mix, sample) in mix_left.iter_mut().zip(output[0].iter()) {
                        *mix += sample;
                    }
                    for (mix, sample) in mix_right.iter_mut().zip(output[1].iter()) {
                        *mix += sample;
                    }
                }
            });
        for buffers in buffers.iter() {
            for (output, mix) in args.output.iter_mut().zip(&buffers[4..]) {
                for (output, sample) in output.iter_mut().zip(mix.iter()) {
                    *output += sample;
                }
            }
        }
    }

    fn update_output_level(&mut self, args: &ProcessArgs<'_, '_>) {
        let output_level = args
            .output
            .iter()
            .flat_map(|channel| channel.iter())
            .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
        if args.output.iter().any(|channel| !channel.is_empty()) {
            let frames = args
                .output
                .iter()
                .map(|channel| channel.len())
                .max()
                .unwrap_or(0);
            self.output_level += (f64::from(output_level) - self.output_level)
                * output_level_weight(frames, args.sample_rate);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn process(
        tinex: &mut Tinex,
        received: &Receiver<TinexNotification>,
    ) -> Vec<TinexNotification> {
        let arena = bumpalo::Bump::new();
        let mut left = [1.0; 8];
        let mut right = [1.0; 8];
        tinex.process(ProcessArgs {
            sample_rate: 48_000,
            input: [&[], &[]],
            midi_input: &[],
            output: [&mut left, &mut right],
            _arena: &arena,
        });
        assert_eq!(left, [0.0; 8]);
        assert_eq!(right, [0.0; 8]);
        received.try_iter().collect()
    }

    #[test]
    fn sends_levels_only_when_requested() {
        let (sender, receiver) = mpsc::channel();
        let (notifications, received) = mpsc::channel();
        let mut tinex = Tinex::new(receiver, notifications);
        assert!(process(&mut tinex, &received).is_empty());
        assert!(process(&mut tinex, &received).is_empty());

        sender
            .send(TinexRequest::OutputLevel(HashMap::new()))
            .unwrap();
        sender
            .send(TinexRequest::OutputLevel(HashMap::new()))
            .unwrap();
        assert!(matches!(
            process(&mut tinex, &received).as_slice(),
            [
                TinexNotification::OutputLevel {
                    output_level: 0.0,
                    ..
                },
                TinexNotification::OutputLevel {
                    output_level: 0.0,
                    ..
                }
            ]
        ));
        assert!(process(&mut tinex, &received).is_empty());
    }

    #[test]
    fn mixes_parallel_chunks_with_reused_track_buffers() {
        struct Fill(f32);

        impl plugin::Plugin for Fill {
            fn process(
                &mut self,
                _: &[(usize, MidiMessage)],
                _: [&[f32]; 2],
                output: [&mut [f32]; 2],
            ) {
                for (index, channel) in output.into_iter().enumerate() {
                    channel.fill(self.0 * (index as f32 + 1.0));
                }
            }
        }

        let (sender, receiver) = mpsc::channel();
        let (notifications, received) = mpsc::channel();
        let mut tinex = Tinex::new(receiver, notifications);
        for value in [0.25, 0.5, -0.125] {
            sender
                .send(TinexRequest::NewTrack(Track::with_plugin(Fill(value))))
                .unwrap();
        }
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(2)
            .build()
            .unwrap();
        let mut left = [99.0; 3];
        let mut right = [99.0; 2];
        pool.install(|| {
            let mut arena = bumpalo::Bump::new();
            for _ in 0..2 {
                arena.reset();
                tinex.process(ProcessArgs {
                    sample_rate: 48_000,
                    input: [&[], &[]],
                    midi_input: &[],
                    output: [&mut left, &mut right],
                    _arena: &arena,
                });
            }
        });
        assert_eq!(left, [0.625; 3]);
        assert_eq!(right, [1.25; 2]);
        assert_eq!(received.try_iter().count(), 3);
    }

    #[test]
    fn returns_weighted_peaks_and_preserves_levels_for_empty_blocks() {
        struct Passthrough;

        impl plugin::Plugin for Passthrough {
            fn process(
                &mut self,
                _: &[(usize, MidiMessage)],
                input: [&[f32]; 2],
                output: [&mut [f32]; 2],
            ) {
                for (input, output) in input.into_iter().zip(output) {
                    output.copy_from_slice(input);
                }
            }
        }

        let (sender, receiver) = mpsc::channel();
        let (notifications, received) = mpsc::channel();
        let mut tinex = Tinex::new(receiver, notifications);
        sender
            .send(TinexRequest::OutputLevel(HashMap::new()))
            .unwrap();
        sender
            .send(TinexRequest::NewTrack(Track::with_plugin(Passthrough)))
            .unwrap();
        let arena = bumpalo::Bump::new();
        for (left, right) in [
            (&[-0.8, 0.1][..], &[0.2, 0.3][..]),
            (&[0.2][..], &[-0.1][..]),
        ] {
            let mut out_l = vec![0.0; left.len()];
            let mut out_r = vec![0.0; right.len()];
            tinex.process(ProcessArgs {
                sample_rate: 48_000,
                input: [left, right],
                midi_input: &[],
                output: [&mut out_l, &mut out_r],
                _arena: &arena,
            });
        }
        assert!(matches!(
            received.try_recv().unwrap(),
            TinexNotification::OutputLevel {
                output_level: 0.0,
                ..
            }
        ));
        assert!(matches!(
            received.try_recv().unwrap(),
            TinexNotification::TrackCreated(_)
        ));
        assert!(received.try_recv().is_err());

        let expected = 0.8 * (1.0 - (-2.0_f32 / 4800.0).exp()) * (-1.0_f32 / 4800.0).exp()
            + 0.2 * (1.0 - (-1.0_f32 / 4800.0).exp());
        for _ in 0..2 {
            sender
                .send(TinexRequest::OutputLevel(HashMap::new()))
                .unwrap();
            tinex.process(ProcessArgs {
                sample_rate: 48_000,
                input: [&[], &[]],
                midi_input: &[],
                output: [&mut [], &mut []],
                _arena: &arena,
            });
            assert!(
                matches!(received.try_recv().unwrap(), TinexNotification::OutputLevel { output_level: level, .. } if (level - expected).abs() < 1e-6)
            );
            assert!(received.try_recv().is_err());
        }
    }

    #[test]
    fn returns_selected_track_weighted_levels_and_reuses_map() {
        struct Level(f32);
        impl plugin::Plugin for Level {
            fn process(
                &mut self,
                _: &[(usize, MidiMessage)],
                input: [&[f32]; 2],
                output: [&mut [f32]; 2],
            ) {
                for channel in output {
                    channel.fill(input[0].first().copied().unwrap_or(0.0) * self.0);
                }
            }
        }

        let (sender, receiver) = mpsc::channel();
        let (notifications, received) = mpsc::channel();
        let mut tinex = Tinex::new(receiver, notifications);
        let first = Track::with_plugin(Level(1.0));
        let second = Track::with_plugin(Level(0.5));
        let omitted = Track::with_plugin(Level(0.25));
        let ids = [first.id(), second.id(), omitted.id()];
        let missing = TrackId::new();
        assert_eq!(first.output_level(), 0.0);
        for track in [first, second, omitted, Track::new()] {
            sender.send(TinexRequest::NewTrack(track)).unwrap();
        }
        let arena = bumpalo::Bump::new();
        for sample in [-0.8, 0.2] {
            let mut left = [99.0; 2];
            let mut right = [99.0; 1];
            tinex.process(ProcessArgs {
                sample_rate: 48_000,
                input: [&[sample], &[]],
                midi_input: &[],
                output: [&mut left, &mut right],
                _arena: &arena,
            });
            for output in left.into_iter().chain(right) {
                assert!((output - sample * 1.75).abs() < 1e-6);
            }
        }
        assert!(
            received
                .try_iter()
                .all(|notification| matches!(notification, TinexNotification::TrackCreated(_)))
        );

        let mut tracks = HashMap::with_capacity(16);
        for id in [ids[0], ids[1], missing] {
            tracks.insert(id, -1.0);
        }
        let expected = 0.8 * (1.0 - (-2.0_f32 / 4800.0).exp()) * (-2.0_f32 / 4800.0).exp()
            + 0.2 * (1.0 - (-2.0_f32 / 4800.0).exp());
        let capacity = tracks.capacity();
        let addresses: HashMap<_, _> = tracks
            .iter()
            .map(|(id, level)| (*id, level as *const f32))
            .collect();
        for _ in 0..2 {
            sender.send(TinexRequest::OutputLevel(tracks)).unwrap();
            tinex.process(ProcessArgs {
                sample_rate: 48_000,
                input: [&[], &[]],
                midi_input: &[],
                output: [&mut [], &mut []],
                _arena: &arena,
            });
            let TinexNotification::OutputLevel {
                output_level,
                tracks: returned,
            } = received.try_recv().unwrap()
            else {
                panic!("expected output levels");
            };
            tracks = returned;
            assert!((output_level - expected * 1.75).abs() < 1e-6);
            assert!((tracks[&ids[0]] - expected).abs() < 1e-6);
            assert!((tracks[&ids[1]] - expected * 0.5).abs() < 1e-6);
            assert_eq!(tracks[&missing], 0.0);
            assert!(!tracks.contains_key(&ids[2]));
            assert_eq!(tracks.len(), 3);
            assert_eq!(tracks.capacity(), capacity);
            for (id, level) in &tracks {
                assert_eq!(level as *const f32, addresses[id]);
            }
            assert!(received.try_recv().is_err());
        }

        // Deletion is handled before the following level request.
        sender.send(TinexRequest::DeleteTrack(ids[0])).unwrap();
        sender.send(TinexRequest::OutputLevel(tracks)).unwrap();
        tinex.process(ProcessArgs {
            sample_rate: 48_000,
            input: [&[], &[]],
            midi_input: &[],
            output: [&mut [], &mut []],
            _arena: &arena,
        });
        assert!(matches!(
            received.try_recv().unwrap(),
            TinexNotification::TrackDeleted(_)
        ));
        let TinexNotification::OutputLevel { tracks, .. } = received.try_recv().unwrap() else {
            panic!("expected output levels");
        };
        assert_eq!(tracks[&ids[0]], 0.0);
    }

    #[test]
    fn level_requests_follow_track_creation_order() {
        let (sender, receiver) = mpsc::channel();
        let (notifications, received) = mpsc::channel();
        let mut tinex = Tinex::new(receiver, notifications);
        let track = Track::with_plugin(plugin::Silence);
        let id = track.id();
        sender
            .send(TinexRequest::OutputLevel(HashMap::from([(id, -1.0)])))
            .unwrap();
        sender.send(TinexRequest::NewTrack(track)).unwrap();
        sender
            .send(TinexRequest::OutputLevel(HashMap::from([(id, -1.0)])))
            .unwrap();
        let notifications = process(&mut tinex, &received);
        assert!(matches!(notifications.as_slice(), [
            TinexNotification::OutputLevel { tracks: before, .. },
            TinexNotification::TrackCreated(created),
            TinexNotification::OutputLevel { tracks: after, .. },
        ] if before[&id] == 0.0 && *created == id && after[&id] == 0.0));
    }

    #[test]
    fn rejects_tracks_at_capacity_and_accepts_after_deletion() {
        let (sender, receiver) = mpsc::channel();
        let (notifications, received) = mpsc::channel();
        let mut tinex = Tinex::new(receiver, notifications);
        let capacity = TRACK_CAPACITY;
        let vector_capacity = tinex.tracks.capacity();
        for _ in 0..capacity {
            sender
                .send(TinexRequest::NewTrack(Track::with_plugin(plugin::Silence)))
                .unwrap();
        }
        let notifications = process(&mut tinex, &received);
        assert_eq!(notifications.len(), capacity);
        for (index, notification) in notifications.iter().enumerate() {
            assert!(
                matches!(notification, TinexNotification::TrackCreated(created) if *created == tinex.tracks[index].id())
            );
        }
        assert_eq!(tinex.tracks.len(), capacity);

        sender
            .send(TinexRequest::NewTrack(Track::with_plugin(plugin::Silence)))
            .unwrap();
        assert!(matches!(
            process(&mut tinex, &received).as_slice(),
            [TinexNotification::TrackCreationFailed(Track { .. })]
        ));
        assert_eq!(tinex.tracks.len(), capacity);
        assert_eq!(tinex.tracks.capacity(), vector_capacity);

        sender
            .send(TinexRequest::DeleteTrack(tinex.tracks[0].id()))
            .unwrap();
        let replacement = Track::with_plugin(plugin::Silence);
        let replacement_id = replacement.id();
        sender.send(TinexRequest::NewTrack(replacement)).unwrap();
        assert!(matches!(
            process(&mut tinex, &received).as_slice(),
            [TinexNotification::TrackDeleted(Track { .. }), TinexNotification::TrackCreated(id)]
                if *id == replacement_id
        ));
        assert_eq!(tinex.tracks.len(), capacity);
        assert_eq!(tinex.tracks.capacity(), vector_capacity);
    }

    #[test]
    fn processes_queued_requests_in_order_across_callbacks() {
        let (sender, receiver) = mpsc::channel();
        let (notifications, received) = mpsc::channel();
        let mut tinex = Tinex::new(receiver, notifications);
        let first = Track::with_plugin(plugin::Silence);
        let first_id = first.id();
        let second = Track::with_plugin(plugin::Silence);
        let second_id = second.id();
        sender.send(TinexRequest::NewTrack(first)).unwrap();
        sender.send(TinexRequest::DeleteTrack(first_id)).unwrap();
        sender.send(TinexRequest::NewTrack(second)).unwrap();
        assert!(matches!(
            process(&mut tinex, &received).as_slice(),
            [
                TinexNotification::TrackCreated(created_first),
                TinexNotification::TrackDeleted(deleted),
                TinexNotification::TrackCreated(created_second)
            ] if *created_first == first_id && deleted.id() == first_id && *created_second == second_id
        ));
        assert_eq!(tinex.tracks.len(), 1);

        assert!(process(&mut tinex, &received).is_empty());
        assert_eq!(tinex.tracks.len(), 1);

        sender.send(TinexRequest::DeleteTrack(second_id)).unwrap();
        assert!(matches!(
            process(&mut tinex, &received).as_slice(),
            [TinexNotification::TrackDeleted(Track { .. })]
        ));
        assert!(tinex.tracks.is_empty());
    }

    #[test]
    fn ignores_unknown_track_ids() {
        let (sender, receiver) = mpsc::channel();
        let (notifications, received) = mpsc::channel();
        let mut tinex = Tinex::new(receiver, notifications);
        sender.send(TinexRequest::DeleteTrack(Id::new())).unwrap();
        sender
            .send(TinexRequest::NewTrack(Track::with_plugin(plugin::Silence)))
            .unwrap();
        sender.send(TinexRequest::DeleteTrack(Id::new())).unwrap();
        assert!(matches!(
            process(&mut tinex, &received).as_slice(),
            [TinexNotification::TrackCreated(id)] if *id == tinex.tracks[0].id()
        ));
        assert_eq!(tinex.tracks.len(), 1);
    }

    #[test]
    fn deletes_by_id_after_indices_shift_and_ignores_repeated_deletion() {
        let (sender, receiver) = mpsc::channel();
        let (notifications, received) = mpsc::channel();
        let mut tinex = Tinex::new(receiver, notifications);
        let tracks = [
            Track::with_plugin(plugin::Silence),
            Track::with_plugin(plugin::Silence),
            Track::with_plugin(plugin::Silence),
        ];
        let ids = tracks.each_ref().map(Track::id);
        for track in tracks {
            sender.send(TinexRequest::NewTrack(track)).unwrap();
        }
        process(&mut tinex, &received);
        sender.send(TinexRequest::DeleteTrack(ids[0])).unwrap();
        sender.send(TinexRequest::DeleteTrack(ids[0])).unwrap();
        sender.send(TinexRequest::DeleteTrack(ids[2])).unwrap();
        assert!(matches!(
            process(&mut tinex, &received).as_slice(),
            [TinexNotification::TrackDeleted(first), TinexNotification::TrackDeleted(last)]
                if first.id() == ids[0] && last.id() == ids[2]
        ));
        assert_eq!(tinex.tracks.len(), 1);
        assert_eq!(tinex.tracks[0].id(), ids[1]);
    }

    #[test]
    fn processes_pending_requests_after_sender_disconnects() {
        let (sender, receiver) = mpsc::channel();
        let (notifications, received) = mpsc::channel();
        drop(received);
        let mut tinex = Tinex::new(receiver, notifications);
        sender
            .send(TinexRequest::NewTrack(Track::with_plugin(plugin::Silence)))
            .unwrap();
        drop(sender);
        tinex.process(ProcessArgs {
            sample_rate: 48_000,
            input: [&[], &[]],
            midi_input: &[],
            output: [&mut [], &mut []],
            _arena: &bumpalo::Bump::new(),
        });
        assert_eq!(tinex.tracks.len(), 1);
        tinex.process(ProcessArgs {
            sample_rate: 48_000,
            input: [&[], &[]],
            midi_input: &[],
            output: [&mut [], &mut []],
            _arena: &bumpalo::Bump::new(),
        });
        assert_eq!(tinex.tracks.len(), 1);
    }
}
