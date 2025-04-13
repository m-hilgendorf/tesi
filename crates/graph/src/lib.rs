//! # `tesi-graph`: node-based audio-graph
//! ## Features
//! - Thread-safe node-based API for managing [graph::Node]s and [graph::Edge]s.
//!     - Reference-counted, RAII guards
//! - Real-time safe [renderer::Renderer] handle.
//!     - Configurable as single-threaded or multi-threaded with a dedicated thread pool.
//! - As-simple-as-possible [processor::Processor] abstraction for defining audio and event
//!   processing steps.
//! - Generic event handling.
//!
//! ## Usage
//!
//! ```rs
//! use tesi::prelude::*;
//! struct MyProcessor;
//!
//! type Event = ();
//! impl tesi::processor::Processor<Event> for MyProcessor {
//!     fn initialize(&mut self, _sample_rate: f64, _max_num_frames: usize) {}
//!     fn process(&mut self, _context: tesi::processor::ProcessContext<'_, Event>) {}
//!     fn reset(&mut self);
//! }
//!
//! fn main() {
//!     let (graph, renderer) = tesi::graph(tesi::renderer::Options::default());
//!
//!     // Create some ports, for example one input and one output.
//!     let ports = [
//!         Port::new(Direction::Input, Kind::Audio { num_channels: 2 }),
//!         Port::new(Direction::Output, Kind::Audio { num_channels: 2 }),
//!     ];
//!
//!     // Create some nodes.
//!     let n1 = tesi::graph::Node::new(&graph, ports, MyProcessor);
//!     let n2 = tesi::graph::Node::new(&graph, ports, MyProcessor);
//!
//!     // Connect the nodes by creating an edge.
//!     let e = Edge::new(&n1, 0, &n2, 0).unwrap();
//!
//!     // Propagate the nodes we created to the `renderer`.
//!     graph.commit_changes();
//!
//!     // ... pass renderer() to your stream and call renderer.tick() to process ...
//!
//!     // Nodes are like RAII guards, so when they are dropped/go out of scope, their internal
//!     // reference count is decremented. when the reference count hits 0 the node is removed
//!     // from the graph.
//!     drop(n2);
//!
//!     // Note that the edge contains a reference to its nodes, so even though we've dropped the
//!     // handle to n1, we must also remove all of the edges that point to it.
//!     drop(e);
//! }
//! ```
//! ## Architecture
//!
//! ```
//! ┌───────────┐              ┌───────────┐   │
//! │   Node    │              │   Edge    │   │
//! └───────────┘              └───────────┘   │
//!       │                          │         │
//!       │ add/remove node/edge     │         │
//! ┌───────────────────────────────────────┐  │    ┌─────────────────┐
//! │                Graph                  │  │    │  Audio Thread   │
//! └───────────────────────────────────────┘  │    └─────────────────┘
//!                    │                       │             │
//!                    │ update()              │             │ tick()
//! ┌───────────────────────────────────────┐       ┌──────────────────┐
//! │                Controller             │───────│      Renderer    │
//! └───────────────────────────────────────┘       └──────────────────┘
//! ```

pub mod edge;
pub mod error;
pub mod graph;
pub mod node;
