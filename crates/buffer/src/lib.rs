use core::f32;
pub mod audio;
pub mod event;
pub use audio::{Audio, AudioMut};
pub use event::Event;

/// https://github.com/rust-lang/rust/issues/72447
pub const NO_CONSTANT_VALUE: f32 =
    unsafe { std::mem::transmute(0b0_11111111_10000000000000000000000) };
