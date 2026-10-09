use std::sync::mpsc::{Receiver, Sender};

use wmidi::MidiMessage;

use crate::tinex::{id::Id, track::Track};

pub mod id;
pub mod plugin;
pub mod track;

const TRACK_CAPACITY: usize = 64;

pub struct Tinex {
    tracks: Vec<Track>,
    requests: Receiver<TinexRequest>,
    notifications: Sender<TinexNotification>,
}

#[allow(unused)]
pub enum TinexRequest {
    NewTrack(Track),
    DeleteTrack(Id<Track>),
}

#[derive(Debug)]
#[allow(unused)]
pub enum TinexNotification {
    TrackCreated(Id<Track>),
    TrackCreationFailed(Track),
    TrackDeleted(Track),
    OutputLevel(f32),
}

pub struct ProcessArgs<'a, 'out> {
    pub input: [&'a [f32]; 2],
    pub midi_input: &'a [(usize, MidiMessage<'static>)],
    pub output: [&'out mut [f32]; 2],
    pub _arena: &'a bumpalo::Bump,
}

impl Tinex {
    pub fn new(requests: Receiver<TinexRequest>, notifications: Sender<TinexNotification>) -> Self {
        Self {
            tracks: Vec::with_capacity(TRACK_CAPACITY),
            requests,
            notifications,
        }
    }

    /// Handles pending requests, processes audio, and sends the peak absolute output level.
    pub fn process(&mut self, mut args: ProcessArgs<'_, '_>) {
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
        for channel in args.output.iter_mut() {
            channel.fill(0.0);
        }
        for track in self.tracks.iter_mut() {
            let [out_l, out_r] = args.output.each_mut();
            track.process(args.midi_input, args.input, [out_l, out_r]);
        }
        let output_level = args
            .output
            .iter()
            .flat_map(|channel| channel.iter())
            .fold(0.0_f32, |peak, sample| peak.max(sample.abs()));
        let _ = self
            .notifications
            .send(TinexNotification::OutputLevel(output_level));
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
            input: [&[], &[]],
            midi_input: &[],
            output: [&mut left, &mut right],
            _arena: &arena,
        });
        assert_eq!(left, [0.0; 8]);
        assert_eq!(right, [0.0; 8]);
        let mut notifications: Vec<_> = received.try_iter().collect();
        assert!(matches!(
            notifications.pop(),
            Some(TinexNotification::OutputLevel(0.0))
        ));
        assert!(
            notifications
                .iter()
                .all(|notification| !matches!(notification, TinexNotification::OutputLevel(_)))
        );
        notifications
    }

    #[test]
    fn sends_one_level_per_call_without_tracks() {
        let (_sender, receiver) = mpsc::channel();
        let (notifications, received) = mpsc::channel();
        let mut tinex = Tinex::new(receiver, notifications);
        assert!(process(&mut tinex, &received).is_empty());
        assert!(process(&mut tinex, &received).is_empty());

        tinex.process(ProcessArgs {
            input: [&[], &[]],
            midi_input: &[],
            output: [&mut [], &mut []],
            _arena: &bumpalo::Bump::new(),
        });
        assert!(matches!(
            received.try_recv().unwrap(),
            TinexNotification::OutputLevel(0.0)
        ));
        assert!(received.try_recv().is_err());
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
                .send(TinexRequest::NewTrack(Track::new(plugin::Silence)))
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
            .send(TinexRequest::NewTrack(Track::new(plugin::Silence)))
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
        let replacement = Track::new(plugin::Silence);
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
        let first = Track::new(plugin::Silence);
        let first_id = first.id();
        let second = Track::new(plugin::Silence);
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
            .send(TinexRequest::NewTrack(Track::new(plugin::Silence)))
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
            Track::new(plugin::Silence),
            Track::new(plugin::Silence),
            Track::new(plugin::Silence),
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
            .send(TinexRequest::NewTrack(Track::new(plugin::Silence)))
            .unwrap();
        drop(sender);
        tinex.process(ProcessArgs {
            input: [&[], &[]],
            midi_input: &[],
            output: [&mut [], &mut []],
            _arena: &bumpalo::Bump::new(),
        });
        assert_eq!(tinex.tracks.len(), 1);
        tinex.process(ProcessArgs {
            input: [&[], &[]],
            midi_input: &[],
            output: [&mut [], &mut []],
            _arena: &bumpalo::Bump::new(),
        });
        assert_eq!(tinex.tracks.len(), 1);
    }
}
