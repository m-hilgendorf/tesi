use processor::{Direction, Processor};
use triple_buffer::{TripleBuffer};
use util::collections::BitSet;
use std::{
    cell::{RefCell, UnsafeCell},
    collections::BTreeSet,
    rc::Rc,
    sync::Arc,
};

pub use crate::edge::Edge;
use crate::{error::Error, render::single_threaded::{self, Renderer}};
pub use crate::node::{self, Node};

type Channel = fifo::Receiver<RenderMessage>;

pub fn graph(
    _global_ports: Vec<processor::Port>,
) -> (Graph, Renderer) {
    let (sender, receiver) = fifo::channel(16_384, None, || RenderMessage::Nop);
    let (input, output) = TripleBuffer::default().split();
    let renderer = Renderer {
        state: output,
        channel: sender,
    };
    let inner = Inner {
        nodes: Vec::new(),
        free_list: Vec::new(),
        channel: receiver,
        state: input,
    };
    let graph = Graph {
        inner: Rc::new(RefCell::new(inner)),
    };
    (graph, renderer)
}

pub struct Graph {
    pub(crate) inner: Rc<RefCell<Inner>>,
}

pub(crate)struct Inner {
    pub(crate) sample_rate: f64,
    pub(crate) max_buffer_size: usize,
    pub(crate) nodes: Vec<Option<NodeData>>,
    pub(crate) free_list: Vec<usize>,
    pub(crate) channel: Channel,
    pub(crate) state: triple_buffer::Input<Option<single_threaded::State>>,
}

pub(crate) struct NodeData {
    pub(crate) ports: Vec<PortData>,
    pub(crate) processor: Arc<UnsafeCell<dyn Processor>>,
}

pub(crate) struct PortData {
    pub port: processor::Port,
    pub connection: Option<(usize, usize)>,
}

#[derive(Copy, Clone, Debug)]
pub(crate) enum RenderMessage {
    Nop,
    RemoveNode(u32),
    ReactivateNode(u32),
    LatencyChanged(u32, u32)
}

impl Graph {
    pub fn latency_changed(&self) {
        todo!("latency changes")
    }

    /// Propagate changes to the graph (new or removed [Node]s and [Edge]s)
    pub fn commit_changes(&self) {
        let this = self.inner.borrow();
        let mut visited = BitSet::with_capacity(this.nodes.len());
        let mut stack = vec![0];

        // Sort.
        let mut nodes = Vec::with_capacity(this.nodes.len());
        while let Some(index) = stack.pop() {
            if visited.get(index) {
                continue;
            }
            visited.set(index);
            let data = this.nodes[index].as_ref().unwrap();
            nodes.push(single_threaded::Node {
                processor: data.clone(),
                active: todo!(),
                ports: todo!(),
                audio_inputs: todo!(),
                audio_outputs: todo!(),
                event_inputs: todo!(),
                event_outputs: todo!(),
            })
        }

        let mut state = single_threaded::State {
            sample_rate: self.sample_rate,
            max_num_frames: self.max_num_frames,
            nodes,
            audio_arena,
            event_arena,
        };
        todo!()
    }
}

impl Inner {
    pub(crate) fn add_node(
        &mut self,
        processor: impl Processor,
        ports: Vec<processor::Port>,
    ) -> usize {
        let ports = ports
            .into_iter()
            .map(|port| PortData {
                port,
                connection: None,
            })
            .collect();
        let data = NodeData {
            ports,
            processor: Arc::new(UnsafeCell::new(processor)),
        };
        if let Some(index) = self.free_list.pop() {
            self.nodes[index].replace(data);
            index
        } else {
            let index = self.nodes.len();
            self.nodes.push(Some(data));
            index
        }
    }

    pub(crate) fn remove_node(&mut self, node: usize) {
        let Some(data) = self.nodes[node].take() else {
            return;
        };
        self.free_list.push(node);
        for port in 0..data.ports.len() {
            let Some((node_, port_)) = data.ports[port].connection else {
                continue;
            };
            let (source, output, sink, input) =
                if matches!(data.ports[port].port.direction, Direction::Input) {
                    (node_, port_, node, port)
                } else {
                    (node, port, node_, port_)
                };
            self.remove_edge(source, output, sink, input);
        }
    }

    pub(crate) fn add_edge(
        &mut self,
        source: usize,
        output: usize,
        sink: usize,
        input: usize,
    ) -> Result<(), Error> {
        // Validate the connection.
        self.check_edge(source, output, sink, input)?;

        // Add the edges.
        self.nodes[source].as_mut().unwrap().ports[output]
            .connection
            .replace((sink, input));
        self.nodes[sink].as_mut().unwrap().ports[input]
            .connection
            .replace((source, output));

        Ok(())
    }

    fn check_edge(
        &self,
        source: usize,
        output: usize,
        sink: usize,
        input: usize,
    ) -> Result<(), Error> {
        let source_ = self.nodes[source].as_ref().ok_or(Error::InvalidId)?;
        let sink_ = self.nodes[sink].as_ref().ok_or(Error::InvalidId)?;

        // Get the output/input ports.
        let output_ = source_.ports.get(output).ok_or(Error::InvalidId)?;
        let input_ = sink_.ports.get(input).ok_or(Error::InvalidId)?;

        // Check if either source/sink port is connected.
        if output_.connection.is_some() || input_.connection.is_some() {
            return Err(Error::AlreadyConnected);
        }

        // Check if the ports are compatible.
        if output_.port.kind != input_.port.kind
            || matches!(output_.port.direction, Direction::Input)
            || matches!(input_.port.direction, Direction::Output)
        {
            return Err(Error::InvalidPortType);
        }

        // Check for cycles.
        let mut stack = vec![sink];
        let mut visited = BTreeSet::new();
        while let Some(node) = stack.pop() {
            if node == source {
                return Err(Error::CycleDetected);
            }
            if visited.contains(&node) {
                continue;
            }
            visited.insert(node);
            let sinks = self.nodes[node]
                .as_ref()
                .unwrap()
                .ports
                .iter()
                .filter_map(|data| {
                    let (node, _) = data.connection?;
                    matches!(data.port.direction, Direction::Output).then_some(node)
                });
            stack.extend(sinks);
        }
        Ok(())
    }

    pub(crate) fn remove_edge(&mut self, source: usize, output: usize, sink: usize, input: usize) {
        if let Some(output_) = &mut self.nodes[source] {
            if output_.ports[output].connection == Some((sink, input)) {
                output_.ports[output].connection.take();
            }
        }
        if let Some(input_) = &mut self.nodes[sink] {
            if input_.ports[input].connection == Some((source, output)) {
                input_.ports[input].connection.take();
            }
        }
    }
}
