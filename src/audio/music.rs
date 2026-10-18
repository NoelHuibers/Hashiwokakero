//! The generative background music, scheduled a little ahead of the audio clock.

use std::cell::RefCell;

use leptos::prelude::{IntervalHandle, set_interval_with_handle};
use web_sys::{AudioContextState, AudioNode, OscillatorType};

use super::imp::{AUDIO, MUSIC, note, scale_freq};

/// Seconds per step of the melody.
const STEP: f64 = 0.42;
/// How far ahead notes are scheduled.
const LOOKAHEAD: f64 = 0.6;

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
    static COMPOSER: RefCell<Composer> = RefCell::new(Composer::default());
}

fn random() -> f64 {
    js_sys::Math::random()
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

pub(super) fn start_music() {
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
