use crate::graph;
use processor::{Editor, Processor};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

pub struct Node {
    pub(crate) inner: Rc<RefCell<Inner>>,
}

pub(crate) struct Inner {
    pub(crate) editor: Box<dyn Editor>,
    pub(crate) index: usize,
    pub(crate) graph: Weak<RefCell<graph::Inner>>,
}

impl Node {
    /// Create a new node.
    pub fn new(graph: &graph::Graph, processor: impl Processor) -> Self {
        let mut editor = processor.editor();
        let index = graph
            .inner
            .borrow_mut()
            .add_node(processor, editor.get_ports());
        let graph = Rc::downgrade(&graph.inner);
        Self {
            inner: Rc::new(RefCell::new(Inner {
                editor,
                index,
                graph,
            })),
        }
    }

    /// Notify the engine that this node's internal processing latency has changed.
    pub fn latency_changed(&self, _latency: f32) {
        todo!()
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        let Some(graph) = self.graph.upgrade() else {
            return;
        };
        graph.borrow_mut().remove_node(self.index);
    }
}

impl Clone for Node {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl Editor for Node {
    fn attach_gui(&mut self, gui: processor::editor::GuiHandle) -> bool {
        self.inner.borrow_mut().editor.attach_gui(gui)
    }

    fn capabilites(&self) -> processor::editor::Capabilities {
        self.inner.borrow().editor.capabilites()
    }

    fn fmt_param(&mut self, id: u64, value: f64) -> Option<String> {
        self.inner.borrow_mut().editor.fmt_param(id, value)
    }

    fn get_param(&mut self, id: u64) -> Option<f64> {
        self.inner.borrow_mut().editor.get_param(id)
    }

    fn get_ports(&mut self) -> Vec<processor::Port> {
        self.inner.borrow_mut().editor.get_ports()
    }

    fn load(&mut self, bytes: &[u8]) {
        self.inner.borrow_mut().editor.load(bytes);
    }

    fn params(&mut self) -> processor::editor::ParameterTree {
        self.inner.borrow_mut().editor.params()
    }

    fn parse_param(&mut self, id: u64, text: &str) -> Option<f64> {
        self.inner.borrow_mut().editor.parse_param(id, text)
    }

    fn save(&mut self) -> Vec<u8> {
        self.inner.borrow_mut().editor.save()
    }

    fn set_param(&mut self, id: u64, value: f64) {
        self.inner.borrow_mut().editor.set_param(id, value);
    }

    fn set_ports(&mut self, ports: &[processor::Port]) -> bool {
        self.inner.borrow_mut().editor.set_ports(ports)
    }
}
