use super::*;
use wmidi::{Channel, ControlFunction, U7};

fn on(note: u8, velocity: u8) -> MidiMessage<'static> {
    MidiMessage::NoteOn(
        Channel::Ch1,
        Note::try_from(note).unwrap(),
        U7::try_from(velocity).unwrap(),
    )
}

fn cc(controller: u8, value: u8) -> MidiMessage<'static> {
    MidiMessage::ControlChange(
        Channel::Ch1,
        ControlFunction::from(U7::try_from(controller).unwrap()),
        U7::try_from(value).unwrap(),
    )
}

fn render(bass: &mut MotownBass, events: &[(usize, MidiMessage)], frames: usize) -> Vec<f32> {
    let mut left = vec![f32::NAN; frames];
    let mut right = vec![f32::NAN; frames];
    bass.process(events, [&[], &[]], [&mut left, &mut right]);
    assert_eq!(left, right);
    assert!(
        left.iter()
            .all(|sample| sample.is_finite() && sample.abs() < 1.0)
    );
    left
}

fn energy(audio: &[f32]) -> f64 {
    audio.iter().map(|&sample| f64::from(sample).powi(2)).sum()
}

#[test]
fn plays_a_rounded_pluck_with_velocity_and_release() {
    let mut soft = MotownBass::new(48_000.0);
    let mut hard = MotownBass::new(48_000.0);
    assert_eq!(render(&mut soft, &[], 64), [0.0; 64]);
    let quiet = render(&mut soft, &[(0, on(40, 32))], 12_000);
    let loud = render(&mut hard, &[(0, on(40, 127))], 12_000);
    assert!(energy(&loud) > energy(&quiet) * 10.0);
    // The short attack rises smoothly, then the muted upper partials lose energy.
    assert!(loud[0].abs() < loud[100..500].iter().map(|s| s.abs()).fold(0.0, f32::max));
    assert!(energy(&loud[100..2100]) > energy(&loud[9000..11000]));

    let release = render(&mut hard, &[(0, on(40, 0))], 48_000);
    assert!(energy(&release[..2400]) > energy(&release[12_000..14_400]) * 100.0);
    assert!(hard.voices.iter().all(Option::is_none));
}

#[test]
fn midi_timing_is_independent_of_block_size_and_writes_unequal_outputs() {
    let mut whole = MotownBass::new(48_000.0);
    let mut split = MotownBass::new(48_000.0);
    let events = [(127, on(40, 100)), (480, on(43, 90)), (800, on(40, 0))];
    let expected = render(&mut whole, &events, 1200);
    let mut actual = render(&mut split, &[], 127);
    actual.extend(render(&mut split, &[(0, events[0].1.clone())], 353));
    actual.extend(render(&mut split, &[(0, events[1].1.clone())], 320));
    actual.extend(render(&mut split, &[(0, events[2].1.clone())], 400));
    assert_eq!(expected, actual);
    assert_eq!(expected[..127], [0.0; 127]);

    let expected = render(&mut whole, &[], 256);
    let mut left = [f32::NAN; 64];
    let mut right = [f32::NAN; 256];
    split.process(&[], [&[1.0; 64], &[1.0; 256]], [&mut left, &mut right]);
    assert_eq!(left.as_slice(), &expected[..64]);
    assert_eq!(right.as_slice(), expected);
    // An event at the block end updates state before the next call.
    render(&mut split, &[(0, on(48, 100))], 0);
    assert!(
        split
            .voices
            .iter()
            .flatten()
            .any(|voice| voice.note == Note::C3)
    );
}

#[test]
fn sustain_and_all_sound_off_work_across_channels() {
    let mut bass = MotownBass::new(48_000.0);
    render(
        &mut bass,
        &[(0, on(40, 110)), (0, cc(64, 127)), (200, on(40, 0))],
        4800,
    );
    assert!(
        bass.voices
            .iter()
            .flatten()
            .any(|voice| voice.note == Note::E2)
    );
    let held = render(&mut bass, &[], 4800);
    let released = render(
        &mut bass,
        &[(
            0,
            MidiMessage::ControlChange(
                Channel::Ch2,
                ControlFunction::from(U7::try_from(64).unwrap()),
                U7::try_from(0).unwrap(),
            ),
        )],
        4800,
    );
    assert!(energy(&held) > energy(&released));
    render(&mut bass, &[(0, cc(120, 0))], 0);
    assert_eq!(render(&mut bass, &[], 256), [0.0; 256]);
}

#[test]
fn voices_are_bounded_and_registration_constructs_the_instrument() {
    let mut bass = MotownBass::new(44_100.0);
    let notes: Vec<_> = (36..68).map(|note| (0, on(note, 127))).collect();
    let audio = render(&mut bass, &notes, 8192);
    assert!(energy(&audio) > 0.0);
    assert_eq!(bass.voices.iter().flatten().count(), VOICES);
    let builder = crate::FACTORY
        .iter()
        .find(|builder| builder.name() == "Bass")
        .unwrap();
    let mut plugin = builder.build(48_000.0);
    let mut outputs = [[0.0; 64]; 2];
    let [left, right] = &mut outputs;
    plugin.process(&[(0, on(40, 100))], [&[], &[]], [left, right]);
    assert_eq!(outputs[0], outputs[1]);
    assert!(outputs[0].iter().any(|&sample| sample != 0.0));
}

#[test]
fn rejects_invalid_sample_rates() {
    for rate in [0.0, -1.0, f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
        assert!(std::panic::catch_unwind(|| MotownBass::new(rate)).is_err());
    }
}
