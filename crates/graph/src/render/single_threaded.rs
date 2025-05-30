use processor::{context, port::Kind, Direction, Processor as _};
use std::{cell::UnsafeCell, ops::DerefMut, sync::Arc};

use crate::graph::{self, RenderMessage};

pub struct Renderer {
    state: util::swappable::Reader<State>,
}

pub(crate) struct State {
    // Data kept for activation/processing.
    pub sample_rate: f64,
    pub max_num_frames: usize,

    // The list of nodes to process, in topological order.
    pub nodes: util::array::Array<Node>,

    // Audio/event buffer allocators.
    pub audio_arena: buffer::audio::Arena,
    pub event_arena: buffer::event::Arena,

    pub writer: batchbuffer::Writer<RenderMessage>,
}

pub(crate) struct Node {
    pub processor: Arc<UnsafeCell<dyn processor::Processor>>,

    // Nodes may be deactivated at any time.
    pub active: bool,

    // List of the ports bound to the node.
    pub ports: Box<[Port]>,

    // Scratch space for creating contexts.
    pub audio_inputs: Box<[buffer::Audio]>,
    pub audio_outputs: Box<[buffer::Audio]>,
    pub event_inputs: Box<[buffer::Event]>,
    pub event_outputs: Box<[buffer::Event]>,
}

pub(crate) struct Port {
    pub kind: Kind,
    pub direction: Direction,
    pub index: usize,
}

impl Renderer {
    pub fn process(
        &mut self,
        input: &buffer::Audio,
        output: &mut buffer::Audio,
     ) {
        debug_assert!(input.num_frames == output.num_frames, "i/o size mismatch");
        let num_frames = input.num_frames;
        let mut state = self.state.read();

        // Bind the root i/o.
        let root = &mut state.nodes[0];
        root.audio_outputs[0].assign_to(input);
        root.audio_inputs[1].assign_to(output);

        // Process nodes.
        for index in 0..state.nodes.len() {
            state.process_node(index, num_frames);
        }
    }
}

impl State {
    fn post_message(&mut self, msg: RenderMessage) {
        loop {
            let Some(mut txn) = self.writer.write(1) else {
                return;
            };
            if txn.len() == 0 {
                std::hint::spin_loop();
                continue;
            }
            txn[0] = msg;
            txn.commit();
            break;
        }
    }

    fn process_node(&mut self, index: usize, num_frames: u32) {
        let Self {
            sample_rate,
            nodes,
            ..
        } = self;
        let node = unsafe { nodes.get_unchecked_mut(index) };

        // Skip processing inactive nodes.
        if !node.active {
            for outp in &mut node.audio_outputs {
                outp.set_constant_value(0.0);
            }
            return;
        }

        // Create the context.
        let context = context::Process {
            sample_rate: *sample_rate,
            num_frames,
            audio_inputs: &node.audio_inputs,
            audio_outputs: &mut node.audio_outputs,
            event_inputs: &node.event_inputs,
            event_outputs: &mut node.event_outputs
        };

        // Process samples.
        let result = unsafe {
            (*node.processor.get()).process(context)
        };

        // TODO: tail frames.

        // Deactivate nodes that are finished.
        match result.state {
            processor::processor::State::Continue => (),
            processor::processor::State::Finished => {
                node.active = false;
                self.post_message(RenderMessage::RemoveNode(index as _));
            }
        }
    }

    pub fn assign_buffers(&mut self) {
        for index in 1..self.nodes.len() {
            self.acquire_outputs(index);
            self.release_inputs(index);
        }
    }

    fn acquire_outputs(&mut self, node: usize) {
        let Self {
            nodes,
            audio_arena,
            event_arena,
            ..
        } = self;
        let node = unsafe { nodes.get_unchecked_mut(node) };
        node.ports
            .iter()
            .filter(|port| matches!((&port.kind, port.direction), (Kind::Audio(_), Direction::Output)))
            .enumerate()
            .for_each(|(idx, _)| {
                if !audio_arena.acquire(&mut node.audio_outputs[idx]) {
                    util::rt_error("failed to acquire audio input buffer");
                }
            });
        node.ports
            .iter()
            .filter(|port| matches!((&port.kind, port.direction), (Kind::Event(_), Direction::Output)))
            .enumerate()
            .for_each(|(idx, _)| {
                if !event_arena.acquire(&mut node.event_outputs[idx]) {
                    util::rt_error("failed to acquire audio input buffer")
                }
            });
    }

    fn release_inputs(&mut self, node: usize) {
        let Self {
            nodes,
            audio_arena,
            event_arena,
            ..
        } = self;
        let node = unsafe { nodes.get_unchecked_mut(node) };
        node.ports
            .iter()
            .filter(|port| matches!((&port.kind, port.direction), (Kind::Audio(_), Direction::Input)))
            .enumerate()
            .for_each(|(idx, _)| {
                audio_arena.release(&mut node.audio_inputs[idx]);
            });
        node.ports
            .iter()
            .filter(|port| matches!((&port.kind, port.direction), (Kind::Event(_), Direction::Input)))
            .enumerate()
            .for_each(|(idx, _)| {
                event_arena.release(&mut node.event_inputs[idx]);
            });
    }

}
