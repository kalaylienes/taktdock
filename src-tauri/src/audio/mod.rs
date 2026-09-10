//! Sound. The clock decides where clicks go, the voices say what they sound
//! like, and the engine gets them out of a speaker.

pub mod clock;
pub mod engine;
pub mod voice;

pub use engine::{BeatEvent, Engine, Notice, Status};
