//! Sound effects and background music, synthesized with the Web Audio API: no audio files, a
//! chiptune feel that matches the pixel art.
//!
//! The music is generative: koto-like plucks wandering through the Japanese *In* scale
//! (D E♭ G A B♭), with an echo and now and then a low drone. Browsers only allow audio after a
//! user gesture, so the audio context is created on the first click or key press.

#[cfg(feature = "hydrate")]
mod imp;
#[cfg(feature = "hydrate")]
mod music;

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
pub use imp::{play, set_sound, unlock};
#[cfg(feature = "hydrate")]
pub use music::set_music;

/// On the server there is nothing to play.
#[cfg(not(feature = "hydrate"))]
pub fn play(_sfx: Sfx) {}
#[cfg(not(feature = "hydrate"))]
pub fn set_sound(_on: bool) {}
#[cfg(not(feature = "hydrate"))]
pub fn set_music(_on: bool) {}
#[cfg(not(feature = "hydrate"))]
pub fn unlock() {}
