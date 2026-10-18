//! The browser side: the Web Audio graph, the synth voice and the sound effects.

use std::cell::{Cell, RefCell};

use web_sys::{
    AudioContext, AudioContextState, AudioNode, BiquadFilterType, GainNode, OscillatorType,
};

use super::Sfx;
use super::music::start_music;

/// Semitones of the In scale (Miyako-bushi) above D.
const IN_SCALE: [i32; 5] = [0, 1, 5, 7, 8];
const D3: f64 = 146.83;

pub(super) struct Audio {
    pub(super) ctx: AudioContext,
    sfx: GainNode,
    pub(super) music: GainNode,
}

thread_local! {
    pub(super) static AUDIO: RefCell<Option<Audio>> = const { RefCell::new(None) };
    static SOUND: Cell<bool> = const { Cell::new(true) };
    pub(super) static MUSIC: Cell<bool> = const { Cell::new(false) };
}

/// Frequency of a degree of the In scale, counted from D3.
pub(super) fn scale_freq(degree: i32) -> f64 {
    let octave = degree.div_euclid(5);
    let semitones = 12 * octave + IN_SCALE[degree.rem_euclid(5) as usize];
    D3 * 2f64.powf(f64::from(semitones) / 12.0)
}

fn build() -> Option<Audio> {
    let ctx = AudioContext::new().ok()?;
    let out = ctx.destination();
    let sfx = ctx.create_gain().ok()?;
    sfx.gain().set_value(0.5);
    sfx.connect_with_audio_node(&out).ok()?;

    // Music: a soft lowpass, and an echo for the plucks to ring in
    let music = ctx.create_gain().ok()?;
    music.gain().set_value(0.0);
    let filter = ctx.create_biquad_filter().ok()?;
    filter.set_type(BiquadFilterType::Lowpass);
    filter.frequency().set_value(2600.0);
    let delay = ctx.create_delay().ok()?;
    delay.delay_time().set_value(0.36);
    let feedback = ctx.create_gain().ok()?;
    feedback.gain().set_value(0.32);
    music.connect_with_audio_node(&filter).ok()?;
    filter.connect_with_audio_node(&out).ok()?;
    filter.connect_with_audio_node(&delay).ok()?;
    delay.connect_with_audio_node(&feedback).ok()?;
    feedback.connect_with_audio_node(&delay).ok()?;
    feedback.connect_with_audio_node(&out).ok()?;
    Some(Audio { ctx, sfx, music })
}

/// Creates or resumes the audio context; call from a user gesture.
pub fn unlock() {
    AUDIO.with(|a| {
        let mut a = a.borrow_mut();
        if a.is_none() {
            *a = build();
        }
        if let Some(audio) = a.as_ref()
            && audio.ctx.state() == AudioContextState::Suspended
        {
            let _ = audio.ctx.resume();
        }
    });
    if MUSIC.get() {
        start_music();
    }
}

/// One note: `f0` gliding to `f1`, with a quick attack and an exponential decay.
#[expect(
    clippy::too_many_arguments,
    reason = "one argument per property of the note, a struct would only add noise"
)]
#[expect(
    clippy::cast_possible_truncation,
    reason = "audible frequencies lose nothing as the f32 that Web Audio parameters take"
)]
pub(super) fn note(
    audio: &Audio,
    dest: &AudioNode,
    wave: OscillatorType,
    f0: f64,
    f1: f64,
    at: f64,
    length: f64,
    volume: f32,
) {
    let ctx = &audio.ctx;
    let (Ok(osc), Ok(gain)) = (ctx.create_oscillator(), ctx.create_gain()) else {
        return;
    };
    osc.set_type(wave);
    let _ = osc.frequency().set_value_at_time(f0 as f32, at);
    if (f1 - f0).abs() > 0.5 {
        let _ = osc
            .frequency()
            .exponential_ramp_to_value_at_time(f1 as f32, at + length * 0.6);
    }
    let g = gain.gain();
    let _ = g.set_value_at_time(0.0, at);
    let _ = g.linear_ramp_to_value_at_time(volume, at + 0.006);
    let _ = g.exponential_ramp_to_value_at_time(0.0001, at + length);
    if osc.connect_with_audio_node(&gain).is_err() || gain.connect_with_audio_node(dest).is_err() {
        return;
    }
    let _ = osc.start_with_when(at);
    let _ = osc.stop_with_when(at + length + 0.05);
}

pub fn play(sfx: Sfx) {
    if !SOUND.get() {
        return;
    }
    unlock();
    AUDIO.with(|a| {
        let a = a.borrow();
        let Some(audio) = a.as_ref() else { return };
        let t = audio.ctx.current_time() + 0.01;
        let dest: &AudioNode = &audio.sfx;
        let tri = OscillatorType::Triangle;
        let sq = OscillatorType::Square;
        let sine = OscillatorType::Sine;
        match sfx {
            Sfx::Place => note(audio, dest, tri, 587.0, 880.0, t, 0.12, 0.5),
            Sfx::Double => {
                note(audio, dest, tri, 698.0, 1047.0, t, 0.1, 0.45);
                note(audio, dest, tri, 880.0, 1319.0, t + 0.06, 0.12, 0.35);
            }
            Sfx::Remove => note(audio, dest, tri, 523.0, 330.0, t, 0.14, 0.4),
            Sfx::Blocked => note(audio, dest, sq, 140.0, 110.0, t, 0.16, 0.12),
            Sfx::Island => {
                note(
                    audio,
                    dest,
                    sine,
                    scale_freq(13),
                    scale_freq(13),
                    t,
                    0.25,
                    0.3,
                );
                note(
                    audio,
                    dest,
                    sine,
                    scale_freq(15),
                    scale_freq(15),
                    t + 0.08,
                    0.35,
                    0.3,
                );
            }
            Sfx::Hint => {
                for (i, d) in (0u8..).zip([12, 14, 17]) {
                    let f = scale_freq(d);
                    note(audio, dest, tri, f, f, t + 0.07 * f64::from(i), 0.2, 0.25);
                }
            }
            Sfx::Correct => {
                note(
                    audio,
                    dest,
                    sine,
                    scale_freq(10),
                    scale_freq(10),
                    t,
                    0.2,
                    0.3,
                );
                note(
                    audio,
                    dest,
                    sine,
                    scale_freq(13),
                    scale_freq(13),
                    t + 0.1,
                    0.3,
                    0.3,
                );
            }
            Sfx::Wrong => {
                note(audio, dest, sq, 220.0, 220.0, t, 0.14, 0.1);
                note(audio, dest, sq, 185.0, 165.0, t + 0.15, 0.22, 0.1);
            }
            Sfx::Win => {
                // Up the In scale, then a ringing chord
                for (i, d) in (0u8..).zip([5, 7, 8, 10, 12, 13, 15]) {
                    let f = scale_freq(d);
                    note(audio, dest, tri, f, f, t + 0.09 * f64::from(i), 0.35, 0.3);
                }
                for d in [10, 13, 15] {
                    let f = scale_freq(d);
                    note(audio, dest, sine, f, f, t + 0.7, 1.6, 0.18);
                }
            }
        }
    });
}

pub fn set_sound(on: bool) {
    SOUND.set(on);
}
