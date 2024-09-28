use crate::bus::{AudioBus, AudioBusMut};

pub trait Processor {
    fn initialize(&mut self, sample_rate: f64, max_num_frames: usize);
    fn process(&mut self, context: &mut Context<'_>);
    fn reset(&mut self);
}

pub struct Context<'a> {
    pub audio_inputs: &'a [AudioBus],
    pub audio_outputs: &'a mut [AudioBusMut],
}
