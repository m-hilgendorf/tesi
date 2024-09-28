//! ## Usage
//!
//! ```rs
//! use tesi_graph as graph;
//!
//! let options = graph::Options {
//!     num_inputs: 2,
//!     num_outputs: 2,
//!     num_workers: 4,
//! };
//!
//! let graph = graph::Graph::new(options);
//!
//! // Create the renderer for the graph. Note there is only one renderer allowed at a time. It is
//! // returned to the graph once it has been dropped.
//!
//! let renderer = graph.renderer().unwrap();
//!
//! // Prepare.
//! renderer.prepare(48e3, 2048);
//! let audio_stream = audio_backend::start(|/* ... */| renderer.render(/* ... */));
//!
//! ```
pub mod bus;
pub mod graph;
pub mod proc;

mod alloc;
mod renderer;

pub use graph::*;
pub use renderer::*;
