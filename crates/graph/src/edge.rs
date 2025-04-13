use crate::{
    error::Error,
    node::{self, Node},
};
use std::{
    cell::RefCell,
    rc::{Rc, Weak},
};

#[derive(Debug)]
pub struct Edge {
    pub(crate) inner: Rc<Inner>,
}

#[derive(Debug)]
pub(crate) struct Inner {
    pub(crate) source: Weak<RefCell<node::Inner>>,
    pub(crate) output: usize,
    pub(crate) sink: Weak<RefCell<node::Inner>>,
    pub(crate) input: usize,
}

impl Edge {
    /// Create a new edge from (source, output) -> (sink, input). Errors if:
    ///
    /// - The edge would create a cycle.
    /// - Either port is already connected.
    /// - The ports do not exist.
    /// - The port [super::Kind] are incompatible.
    pub fn new(
        source: &Node,
        output: usize,
        sink: &node::Node,
        input: usize,
    ) -> Result<Self, Error> {
        // Sanity check that these nodes are in the same graph.
        if !source
            .inner
            .borrow()
            .graph
            .ptr_eq(&sink.inner.borrow().graph)
        {
            return Err(Error::Graph);
        }
        let Some(source_graph) = source.inner.borrow().graph.upgrade() else {
            return Err(Error::Lifetime);
        };
        source_graph.borrow_mut().add_edge(
            source.inner.borrow().index,
            output,
            sink.inner.borrow().index,
            input,
        )?;
        Ok(Self {
            inner: Rc::new(Inner {
                source: Rc::downgrade(&source.inner),
                output,
                sink: Rc::downgrade(&sink.inner),
                input,
            }),
        })
    }

    /// Returns if both nodes that the edge references are alive. This is just a hint, since it is
    /// possible for either or both of the source/sink nodes to be dropped concurrently with this
    /// method.
    pub fn check_nodes_alive(&self) -> bool {
        self.inner.source.strong_count() > 0 && self.inner.sink.strong_count() > 0
    }

    /// Get the source node/port of this edge.
    pub fn source(&self) -> Option<(Node, usize)> {
        let node = Node {
            inner: self.inner.source.upgrade()?,
        };
        Some((node, self.inner.output))
    }

    /// Get the sink node/port of this edge.
    pub fn sink(&self) -> Option<(Node, usize)> {
        let node = Node {
            inner: self.inner.sink.upgrade()?,
        };
        Some((node, self.inner.input))
    }
}

impl Clone for Edge {
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
        }
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        let Some(source) = self.source.upgrade() else {
            return;
        };
        let Some(sink) = self.sink.upgrade() else {
            return;
        };
        let Some(graph) = source.borrow().graph.upgrade() else {
            return;
        };
        graph.borrow_mut().remove_edge(
            source.borrow().index,
            self.output,
            sink.borrow().index,
            self.input,
        );
    }
}
