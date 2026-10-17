//! Sound effects and background music, synthesized with the Web Audio API: no audio files, a
//! chiptune feel that matches the pixel art.
//!
//! The music is generative: koto-like plucks wandering through the Japanese *In* scale
//! (D E♭ G A B♭), with an echo and now and then a low drone. Browsers only allow audio after a
//! user gesture, so the audio context is created on the first click or key press.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sfx {
    /// A first bridge between two islands.
    Place,
    /// The second, vermilion bridge.
    Double,
    Remove,
    /// The bridge would cross another one.
    Blocked,
    /// An island got all its bridges.
    Island,
    Hint,
    /// Check found no mistakes.
    Correct,
    /// Check found bridges that don't belong to the solution.
    Wrong,
    Win,
}

#[cfg(feature = "hydrate")]
pub use imp::{play, set_music, set_sound, unlock};

/// On the server there is nothing to play.
#[cfg(not(feature = "hydrate"))]
pub fn play(_sfx: Sfx) {}
#[cfg(not(feature = "hydrate"))]
pub fn set_sound(_on: bool) {}
#[cfg(not(feature = "hydrate"))]
pub fn set_music(_on: bool) {}
#[cfg(not(feature = "hydrate"))]
pub fn unlock() {}

#[cfg(feature = "hydrate")]
mod imp {
    use std::cell::{Cell, RefCell};

    use leptos::prelude::{IntervalHandle, set_interval_with_handle};
    use web_sys::{
        AudioContext, AudioContextState, AudioNode, BiquadFilterType, GainNode, OscillatorType,
    };

    use super::Sfx;

    /// Semitones of the In scale (Miyako-bushi) above D.
    const IN_SCALE: [i32; 5] = [0, 1, 5, 7, 8];
    const D3: f64 = 146.83;
    /// Seconds per step of the melody.
    const STEP: f64 = 0.42;
    /// How far ahead notes are scheduled.
    const LOOKAHEAD: f64 = 0.6;

    struct Audio {
        ctx: AudioContext,
        sfx: GainNode,
        music: GainNode,
    }

    #[derive(Default)]
    struct Composer {
        /// Audio time of the next step.
        next: f64,
        step: u32,
        /// Position in the scale (index into three octaves).
        degree: i32,
        /// Steps left to rest.
        rest: u32,
        timer: Option<IntervalHandle>,
    }

    thread_local! {
        static AUDIO: RefCell<Option<Audio>> = const { RefCell::new(None) };
        static COMPOSER: RefCell<Composer> = RefCell::new(Composer::default());
        static SOUND: Cell<bool> = const { Cell::new(true) };
        static MUSIC: Cell<bool> = const { Cell::new(false) };
    }

    fn random() -> f64 {
        js_sys::Math::random()
    }

    /// Frequency of a degree of the In scale, counted from D3.
    fn scale_freq(degree: i32) -> f64 {
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
    fn note(
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
        if osc.connect_with_audio_node(&gain).is_err()
            || gain.connect_with_audio_node(dest).is_err()
        {
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

    pub fn set_music(on: bool) {
        MUSIC.set(on);
        if on {
            // Only possible after a gesture; `unlock` starts it then
            let ready = AUDIO.with(|a| {
                a.borrow()
                    .as_ref()
                    .is_some_and(|a| a.ctx.state() == AudioContextState::Running)
            });
            if ready {
                start_music();
            }
        } else {
            stop_music();
        }
    }

    fn start_music() {
        let running = COMPOSER.with(|c| c.borrow().timer.is_some());
        if running {
            return;
        }
        AUDIO.with(|a| {
            if let Some(audio) = a.borrow().as_ref() {
                let t = audio.ctx.current_time();
                let g = audio.music.gain();
                let _ = g.cancel_scheduled_values(t);
                let _ = g.set_value_at_time(g.value(), t);
                let _ = g.linear_ramp_to_value_at_time(0.35, t + 2.0);
                COMPOSER.with(|c| {
                    let mut c = c.borrow_mut();
                    c.next = t + 0.1;
                    c.degree = 8;
                });
            }
        });
        let timer = set_interval_with_handle(compose, std::time::Duration::from_millis(150)).ok();
        COMPOSER.with(|c| c.borrow_mut().timer = timer);
    }

    fn stop_music() {
        if let Some(timer) = COMPOSER.with(|c| c.borrow_mut().timer.take()) {
            timer.clear();
        }
        AUDIO.with(|a| {
            if let Some(audio) = a.borrow().as_ref() {
                let t = audio.ctx.current_time();
                let g = audio.music.gain();
                let _ = g.cancel_scheduled_values(t);
                let _ = g.set_value_at_time(g.value(), t);
                let _ = g.linear_ramp_to_value_at_time(0.0, t + 0.8);
            }
        });
    }

    /// Schedules the melody a little ahead of the audio clock.
    fn compose() {
        AUDIO.with(|cell| {
            let state = cell.borrow();
            let Some(audio) = state.as_ref() else { return };
            let now = audio.ctx.current_time();
            let dest: &AudioNode = &audio.music;
            COMPOSER.with(|composer| {
                let mut c = composer.borrow_mut();
                // After a pause in the tab, start fresh instead of catching up
                if c.next < now {
                    c.next = now + 0.05;
                }
                #[expect(
                    clippy::while_float,
                    reason = "schedules steps until the lookahead on the audio clock, the float is \
                              a time and not a loop counter"
                )]
                while c.next < now + LOOKAHEAD {
                    let at = c.next;
                    if c.step % 32 == 0 {
                        // A low drone on D or A
                        let drone = scale_freq(if random() < 0.6 { -5 } else { -2 });
                        note(
                            audio,
                            dest,
                            OscillatorType::Sine,
                            drone,
                            drone,
                            at,
                            6.0,
                            0.22,
                        );
                    }
                    if c.rest > 0 {
                        c.rest -= 1;
                    } else if random() < 0.62 {
                        let roll = random();
                        let step = if roll < 0.4 {
                            1
                        } else if roll < 0.8 {
                            -1
                        } else if roll < 0.9 {
                            2
                        } else {
                            -2
                        };
                        c.degree = (c.degree + step).clamp(4, 14);
                        let melody = scale_freq(c.degree);
                        note(
                            audio,
                            dest,
                            OscillatorType::Triangle,
                            melody,
                            melody,
                            at,
                            1.8,
                            0.2,
                        );
                        if random() < 0.12 {
                            let below = scale_freq(c.degree - 2);
                            note(
                                audio,
                                dest,
                                OscillatorType::Sine,
                                below,
                                below,
                                at + 0.02,
                                1.4,
                                0.12,
                            );
                        }
                    }
                    c.step += 1;
                    // Phrases of eight steps, with a breath in between
                    if c.step % 8 == 0 && random() < 0.5 {
                        #[expect(
                            clippy::cast_possible_truncation,
                            clippy::cast_sign_loss,
                            reason = "`random()` is in [0, 1), so this floors to 0, 1 or 2"
                        )]
                        let breath = (random() * 3.0) as u32;
                        c.rest = 2 + breath;
                    }
                    c.next += STEP;
                }
            });
        });
    }
}
