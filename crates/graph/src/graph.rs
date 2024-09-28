use std::{
    cell::UnsafeCell,
    collections::{BTreeMap, BTreeSet, VecDeque},
    marker::PhantomData,
    sync::{atomic::AtomicUsize, Arc, RwLock},
};

use crossbeam::queue::ArrayQueue;
use tesi_util::IsSendSync;

use crate::{
    bus::{AudioBus, AudioBusMut},
    graph::node::Node,
    proc::Processor,
    renderer::{self, Renderer},
};

#[derive(Debug)]
pub enum Error {
    AlreadyConnected,
    BusChannelsMismatched,
    CycleDetected,
    InvalidPort,
}

#[derive(Clone)]
pub struct Graph {
    pub(crate) inner: Arc<RwLock<Inner>>,
}

pub struct Options {
    pub num_input_channels: usize,
    pub num_output_channels: usize,
    pub num_workers: usize,
}

pub(crate) struct Inner {
    pub(crate) nodes: Vec<Option<NodeData>>,
    pub(crate) stack: Vec<usize>,
    pub(crate) sender: triple_buffer::Input<renderer::State>,
    pub(crate) input_node: Option<Node>,
    pub(crate) output_node: Option<Node>,
    pub(crate) num_frames: usize,
    pub(crate) renderer: Option<renderer::Renderer>,
}

pub(crate) struct NodeData {
    pub(crate) options: node::Options,
    pub(crate) incoming: Vec<Option<(usize, usize)>>,
    pub(crate) outgoing: Vec<Option<(usize, usize)>>,
    pub(crate) processor: Arc<IsSendSync<UnsafeCell<dyn Processor>>>,
}

struct InputNode;

struct OutputNode;

pub mod node {
    use crate::{graph, proc::Processor};
    use std::sync::{Arc, RwLock, Weak};

    #[derive(Clone)]
    pub struct Node {
        pub(super) inner: Arc<Inner>,
    }

    pub(super) struct Inner {
        pub(super) index: usize,
        pub(super) graph: Weak<RwLock<graph::Inner>>,
    }

    #[derive(Clone, Debug)]
    pub struct Options {
        pub audio_inputs: Vec<usize>,
        pub audio_outputs: Vec<usize>,
    }

    impl Node {
        pub fn new(graph: &graph::Graph, options: Options, p: impl Processor + 'static) -> Self {
            let index = graph.inner.write().unwrap().add_node(options, p);
            let graph = Arc::downgrade(&graph.inner);
            let inner = Arc::new(Inner { index, graph });
            Self { inner }
        }

        pub fn options(&self) -> Options {
            self.inner.graph.upgrade().unwrap().read().unwrap().nodes[self.inner.index]
                .as_ref()
                .unwrap()
                .options
                .clone()
        }
    }

    impl Drop for Inner {
        fn drop(&mut self) {
            let Some(graph) = self.graph.upgrade() else {
                return;
            };
            graph.write().unwrap().remove_node(self.index);
        }
    }
}

pub mod edge {
    use crate::graph::{self, node};
    use std::sync::{Arc, RwLock, Weak};

    pub struct Edge {
        inner: Arc<Inner>,
    }
    struct Inner {
        source: Arc<node::Inner>,
        output: usize,
        sink: Arc<node::Inner>,
        input: usize,
        graph: Weak<RwLock<graph::Inner>>,
    }

    impl Edge {
        pub fn new(
            graph: &graph::Graph,
            source: &node::Node,
            output: usize,
            sink: &node::Node,
            input: usize,
        ) -> Result<Self, graph::Error> {
            graph.inner.write().unwrap().add_edge(
                source.inner.index,
                output,
                sink.inner.index,
                input,
            )?;
            let inner = Arc::new(Inner {
                source: source.inner.clone(),
                output,
                sink: sink.inner.clone(),
                input,
                graph: Arc::downgrade(&graph.inner),
            });
            Ok(Self { inner })
        }

        pub fn source(&self) -> (node::Node, usize) {
            (
                node::Node {
                    inner: self.inner.source.clone(),
                },
                self.inner.output,
            )
        }

        pub fn sink(&self) -> (node::Node, usize) {
            (
                node::Node {
                    inner: self.inner.sink.clone(),
                },
                self.inner.input,
            )
        }
    }

    impl Drop for Inner {
        fn drop(&mut self) {
            let Some(graph) = self.graph.upgrade() else {
                return;
            };
            graph.write().unwrap().remove_edge(
                self.source.index,
                self.output,
                self.sink.index,
                self.input,
            );
        }
    }
}

impl Graph {
    pub fn new(options: Options) -> Self {
        // Create the sender/receiver
        let (sender, receiver) = triple_buffer::triple_buffer(&renderer::State::new());

        // Create the graph.
        let nodes = vec![];
        let stack = vec![];
        let input_node = None;
        let output_node = None;
        let inner = Arc::new(RwLock::new(Inner {
            nodes,
            stack,
            sender,
            input_node,
            output_node,
            num_frames: 2048,
            renderer: None,
        }));

        // Create the renderer
        {
            let mut inner_ = inner.write().unwrap();
            let renderer = Renderer {
                graph: Arc::downgrade(&inner),
                inner: renderer::Inner::new(options.num_workers, receiver),
                _p: PhantomData,
            };
            inner_.renderer.replace(renderer);
        }

        // Create the graph.
        let graph = Graph { inner };

        // Create the input and output nodes.
        let input_options = node::Options {
            audio_inputs: vec![options.num_input_channels],
            audio_outputs: vec![],
        };
        let input_node = Node::new(&graph, input_options, InputNode);
        let output_options = node::Options {
            audio_inputs: vec![],
            audio_outputs: vec![options.num_output_channels],
        };
        let output_node = Node::new(&graph, output_options, OutputNode);
        {
            let mut graph_ = graph.inner.write().unwrap();
            graph_.input_node.replace(input_node);
            graph_.output_node.replace(output_node);
        }

        graph
    }

    pub fn renderer(&self) -> Option<renderer::Renderer> {
        self.inner.write().unwrap().renderer.clone().take()
    }

    pub fn commit_changes(&self) {
        // Acquire an exclusive lock over the graph.
        let mut graph = self.inner.write().unwrap();

        // Sort topologically with BFS to remap nodes to indices.
        let mut indices = BTreeMap::new();
        let sources = std::iter::once(0)
            .chain(graph.nodes.iter().enumerate().filter_map(|(index, node)| {
                let node = node.as_ref()?;
                node.incoming.is_empty().then_some(index)
            }))
            .collect::<Vec<_>>();
        let mut queue: VecDeque<_> = sources.clone().into();
        while let Some(node) = queue.pop_front() {
            if indices.contains_key(&node) {
                continue;
            }
            let index = indices.len();
            indices.insert(node, index);
            let adjacent = graph.nodes[node]
                .as_ref()
                .unwrap()
                .outgoing
                .iter()
                .flatten()
                .map(|(node, _)| *node);
            queue.extend(adjacent);
        }

        // Get the input and output nodes.
        let input_node = *indices.get(&0).unwrap();
        let output_node = *indices.get(&1).unwrap();

        // Create the renderer state for each node.
        let mut indices = indices.into_iter().collect::<Vec<_>>();
        indices.sort_unstable_by_key(|(_, new)| *new);
        let nodes = indices
            .into_iter()
            .map(|(old, _)| {
                let old = graph.nodes[old].as_ref().unwrap();
                let incoming = old.incoming.clone().into_boxed_slice();
                let outgoing = old.outgoing.clone().into_boxed_slice();
                let audio_inputs = old
                    .options
                    .audio_inputs
                    .iter()
                    .copied()
                    .map(|num_channels| {
                        let bus = AudioBus::new(num_channels);
                        IsSendSync::new(UnsafeCell::new(bus))
                    })
                    .collect::<Vec<_>>()
                    .into_boxed_slice();
                let audio_inputs = IsSendSync::new(UnsafeCell::new(audio_inputs));

                let audio_outputs = old
                    .options
                    .audio_outputs
                    .iter()
                    .copied()
                    .map(|num_channels| {
                        let bus = AudioBusMut::new(num_channels);
                        IsSendSync::new(UnsafeCell::new(bus))
                    })
                    .collect::<Vec<_>>()
                    .into_boxed_slice();
                let audio_outputs = IsSendSync::new(UnsafeCell::new(audio_outputs));
                renderer::Node {
                    audio_inputs,
                    audio_outputs,
                    num_bound_inputs: AtomicUsize::new(0),
                    incoming,
                    outgoing,
                    processor: old.processor.clone(),
                }
            })
            .collect::<Vec<_>>();

        // Allocate audio buffers.
        let (alloc, data) =
            crate::alloc::compile(input_node, output_node, graph.num_frames, 0, &nodes);

        // Create the work queue.
        let queue = ArrayQueue::new(nodes.len());

        // Create the state
        let state = renderer::State {
            queue,
            alloc,
            nodes,
            input_node,
            output_node,
            sources,
            _data: data,
        };

        // Update the renderer.
        graph.sender.write(state);
    }

    pub fn input_node(&self) -> Node {
        self.inner.read().unwrap().input_node.clone().unwrap()
    }

    pub fn output_node(&self) -> Node {
        self.inner.read().unwrap().output_node.clone().unwrap()
    }
}

impl Inner {
    fn add_node(&mut self, options: node::Options, p: impl Processor + 'static) -> usize {
        let incoming = vec![None; options.audio_inputs.len()];
        let outgoing = vec![None; options.audio_outputs.len()];
        let node = NodeData {
            options,
            incoming,
            outgoing,
            processor: Arc::new(IsSendSync::new(UnsafeCell::new(p))),
        };

        if let Some(index) = self.stack.pop() {
            self.nodes[index].replace(node);
            index
        } else {
            let index = self.nodes.len();
            self.nodes.push(Some(node));
            index
        }
    }

    fn remove_node(&mut self, index: usize) {
        self.nodes.get_mut(index).and_then(|node| node.take());
    }

    fn add_edge(
        &mut self,
        source: usize,
        output: usize,
        sink: usize,
        input: usize,
    ) -> Result<(), Error> {
        let source_ = self.nodes[source].as_ref().unwrap();
        let sink_ = self.nodes[sink].as_ref().unwrap();

        // Check if source:output and sink:input are disconnected
        if source_
            .outgoing
            .get(output)
            .ok_or(Error::InvalidPort)?
            .is_some()
            || sink_
                .outgoing
                .get(input)
                .ok_or(Error::InvalidPort)?
                .is_some()
        {
            return Err(Error::AlreadyConnected);
        }

        // Check that the connection is valid.
        if source_.options.audio_outputs[output] != sink_.options.audio_inputs[input] {
            return Err(Error::BusChannelsMismatched);
        }

        // Check if the edge would create a cycle.
        let mut visited = BTreeSet::new();
        let mut stack = vec![sink];
        while let Some(node) = stack.pop() {
            if node == source {
                return Err(Error::CycleDetected);
            }
            if visited.contains(&node) {
                continue;
            }
            visited.insert(node);
            stack.extend(
                self.nodes[node]
                    .as_ref()
                    .unwrap()
                    .outgoing
                    .iter()
                    .flatten()
                    .map(|(node, _)| *node),
            );
        }

        Ok(())
    }

    fn remove_edge(&mut self, source: usize, output: usize, sink: usize, input: usize) {
        self.nodes[source].as_mut().unwrap().outgoing[output].take();
        self.nodes[sink].as_mut().unwrap().incoming[input].take();
    }
}

impl Processor for InputNode {
    fn initialize(&mut self, _sample_rate: f64, _max_num_frames: usize) {}

    fn process(&mut self, context: &mut crate::proc::Context<'_>) {
        let input = &context.audio_inputs[0];
        for output in context.audio_outputs.iter_mut() {
            for (output, input) in output.iter().zip(input.iter()) {
                output.copy_from_slice(input);
            }
        }
    }

    fn reset(&mut self) {}
}

impl Processor for OutputNode {
    fn initialize(&mut self, _sample_rate: f64, _max_num_frames: usize) {}

    fn process(&mut self, context: &mut crate::proc::Context<'_>) {
        let output = &mut context.audio_outputs[0];
        for output in output {
            for sample in output {
                *sample = 0.0;
            }
        }

        for input in context.audio_inputs.iter() {
            for (input, output) in input.iter().zip(context.audio_outputs[0].iter()) {
                for idx in 0..input.len() {
                    output[idx] += input[idx];
                }
            }
        }
    }

    fn reset(&mut self) {}
}

unsafe impl Send for Inner {}
unsafe impl Sync for Inner {}
