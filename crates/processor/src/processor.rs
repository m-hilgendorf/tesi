use crate::Editor;
use context as cx;

/// An abstract interface into a real-time audio or event processing node.
pub trait Processor
where
    Self: Send + 'static,
{
    /// Create the editor handle.
    fn editor(&self) -> Box<dyn Editor>;

    /// Try to activate the process. If this returns None, the node may be destroyed.
    fn activate(&mut self, context: cx::Activate) -> Option<Activated>;

    /// Called before beginning audio processing.
    fn start(&mut self) -> bool {
        true
    }

    /// Called after processing has stopped.
    fn stop(&mut self) -> bool {
        true
    }

    /// Real time processing.
    fn process(&mut self, context: cx::Process<'_>) -> Processed;

    /// Reset or release resources here.
    fn reset(&mut self);
}

/// Data returned from [Processor::activate].
#[derive(Copy, Clone, Debug, Default)]
pub struct Activated {
    /// The processor's latency in samples.
    pub latency: Option<f64>,
}

/// Data returned from [Processor::process].
#[derive(Copy, Clone, Debug, Default)]
pub struct Processed {
    /// The number of frames that the processor was able to consume. A value of -1 means the node may be removed from the graph.
    pub num_frames: isize,

    /// A hint about the number of future samples that this processor will need to render.
    pub tail_samples: Option<usize>,

    /// The amount of gain reduction applied by the plugin.
    pub gain: Option<f64>,
}

pub mod context {
    use crate::port;
    use std::sync::Arc;

    pub struct Process<'a> {
        pub sample_rate: f64,
        pub num_frames: usize,
        pub audio_inputs: &'a [buffer::Audio],
        pub audio_outputs: &'a mut [buffer::AudioMut],
        pub event_inputs: &'a [buffer::Event],
        pub event_outputs: &'a mut [buffer::Event],
    }

    pub struct Activate<'a> {
        /// A handle to the engine that can be cheaply cloned and called on the audio thread.
        pub engine: Engine,

        /// The sample rate for processing.
        pub sample_rate: f64,

        /// The maximum number of frames that will be passed to the audio thread.
        pub max_num_frames: usize,

        /// The audio input port configurations.
        pub ports: &'a [port::Port],
    }

    pub type Engine = Arc<dyn EngineCx>;

    pub trait EngineCx
    where
        Self: 'static + Send + Sync,
    {
        /// Call to request a deactivate/reactivate cycle for this node, temporarily removing it from the processing graph.
        fn request_restart(&self);

        /// Request a parameter flush.
        fn request_flush(&self);
    }
}
