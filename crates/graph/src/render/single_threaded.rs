use processor::{context, port::Kind, processor::Processed, Direction};
use util::collections::Array;
use std::{cell::UnsafeCell, sync::Arc};

use crate::graph::RenderMessage;
type Channel = fifo::Sender<RenderMessage>;

pub struct Renderer {
    pub(crate) state: triple_buffer::Output<Option<State>>,
    pub(crate) channel: fifo::Sender<RenderMessage>,
}

pub(crate) struct State {
    // Data kept for activation/processing.
    pub sample_rate: f64,
    pub max_num_frames: usize,

    // The list of nodes to process, in topological order.
    pub nodes: Array<Node>,

    // Audio/event buffer allocators.
    pub audio_arena: buffer::audio::Arena,
    pub event_arena: buffer::event::Arena,
}

unsafe impl Send for State {}

pub(crate) struct Node {
    pub processor: Arc<UnsafeCell<dyn processor::Processor>>,

    // Nodes may be deactivated at any time.
    pub active: bool,

    // List of the ports bound to the node.
    pub ports: Box<[Port]>,

    // Scratch space for creating contexts.
    pub audio_inputs: Array<buffer::Audio>,
    pub audio_outputs: Array<buffer::Audio>,
    pub event_inputs: Array<buffer::Event>,
    pub event_outputs: Array<buffer::Event>,
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
        // Update state.
        self.state.update();
        let Some(state) = self.state.output_buffer_mut() else {
            return;
        };

        // Bind the root i/o.
        let root = &mut state.nodes[0];
        root.audio_outputs[0].assign_to(input);
        root.audio_inputs[1].assign_to(output);

        // Process nodes.
        for index in 0..state.nodes.len() {
            // Process the node.
            let result = state.process_node(index, num_frames);

            // Deactivate nodes that are finished.
            match result.state {
                processor::processor::State::Continue => (),
                processor::processor::State::Finished => {
                    // Deactivate the node.
                    state.nodes[index].active = false;

                    // Post the node deactivation.
                    post_message(&mut self.channel, RenderMessage::RemoveNode(index as _));
                }
            }
        }
    }
}

impl State {
    fn process_node(&mut self, index: usize, num_frames: u32) -> Processed {
        let Self {
            sample_rate,
            nodes,
            ..
        } = self;
        let node = unsafe { nodes.get_unchecked_mut(index) };

        // Skip processing inactive nodes.
        if !node.active {
            for outp in node.audio_outputs.as_mut_slice() {
                outp.set_constant_value(0.0);
            }
            return Processed {
                state: processor::processor::State::Continue,
                tail_frames: None,
            }
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

        result
    }

    // Assign i/o buffers.
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

fn post_message(channel: &mut Channel, msg: RenderMessage) {
    loop {
        let Some(mut txn) = channel.write(1) else {
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
