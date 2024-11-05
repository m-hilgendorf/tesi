# Kahn's Algorithm for Parallel Audio Rendering

This note covers the basic logic for the multithreaded audio graph rendering algorithm used by `tesi`. It's a parallel variant of Kahn's algorithm that uses an MPSC queue and atomic counters to parallelize the algorithm.

## Background: Topological Scheduling with Kahn's algorithm

Here is a simplified example of an audio graph that uses [Kahn's algorithm](https://en.wikipedia.org/wiki/Topological_sorting#Kahn's_algorithm) to walk the graph in topological order. The idea is that we keep some state around for each graph node, its `indegree`, which represents the number of incoming edges that have not yet been walked. When this count hits `0`, all of the node's dependencies have been processed and it can be added to the queue.

```rs
use std::collections::VecDeque;
struct Graph {
  nodes: Vec<Node>,
  queue: VecDeque<usize>,
}

struct Node {
  incoming: Vec<usize>,
  outgoing: Vec<usize>,
  indegree: usize,
  process: Box<dyn FnMut() -> ()>,
}

impl Graph {
  // Called once per cycle to render the graph.
  fn process(&mut self) {
    // Get the list of roots, nodes with indegree 0.
    let roots = self
      .nodes
      .iter()
      .enumerate()
      .filter_map(|(index, node)| (node.indegree == 0).then_some(index));

    // Clear and push the roots to the quuee.
    self.queue.clear();
    self.queue.extend(roots);

    // Drain the queue and process the nodes along the way.
    while let Some(node) = self.queue.pop_front() {
      // Process the node.
      (self.nodes[node].process)();

      // Reset the node's indegree.
      self.nodes[node].indegree = self.nodes[node].incoming.len();

      // Decrement the indegree of any neighbors.
      for node in self.nodes[node].outgoing.clone() {
        self.nodes[node].indegree -= 1;

        // If the neighbor's indegree hits zero, add it to the back of the queue.
        if self.nodes[node].indegree == 0 {
          self.queue.push_back(node);
        }
      }
    }
  }
}
```

The reason that Kahn's algorithm is preferable to a depth-first walk of the graph is that it is easy to convert it to be used by multiple threads. All we need to do is:

- replace the `VecDequeue` with an MPSC fifo/queue implementation. `crossbeam::ArrayQueue` is a good choice.
- use atomics for the `indegree` field
- wrap the node's internals in `UnsafeCell` to handle processing on different threads.
- write two variants of `process`, one for the audio thread and one for worker threads.

```rs
use crossbeam::queue::ArrayQueue;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::cell::UnsafeCell;

struct Graph {
  counter: AtomicUsize,
  nodes: Vec<UnsafeCell<Node>>,
  queue: ArrayQueue<usize>,
}

struct Node {
  indegree: AtomicUsize,
  incoming: Vec<usize>,
  outgoing: Vec<usize>,
  process: Box<dyn FnMut() -> ()>,
}

unsafe impl Send for Graph {}
unsafe impl Sync for Graph {}

impl Graph {
  fn audio_thread(&self) {
    // Set the counter to 0.
    self.counter.store(self.nodes.len(), Ordering::Relaxed);

    // Get the roots.
    unsafe {
      for (index, node) in self.nodes.iter().enumerate() {
        if (*node.get()).incoming.len() == 0 {
          self.queue.push(index).ok();
        }
      }
    }

    /* ... signal worker threads to start processing ... */

    // Process the nodes.
    self.process_nodes();

    // Spin until all the nodes are processed. Note: this is not technically RT-safe, but it should be for a very short amount of time.
    while self.counter.load(Ordering::Relaxed) != 0 {
      continue;
    }

    /* ... signal the worker threads to spin ... */

  }

  fn worker_thread(&self) {
    loop {
      /* ... wait for the audio thread to signal that there is work to do ... */

      self.process_nodes();
   }
  }

  fn process_nodes(&self) {
    while let Some(node) = self.queue.pop() {
      // Safety: the algorithm guarantees that whichever thread pops the node index from the queue has exclusive access to that node.
      let node = unsafe { &mut *self.nodes[node].get() };

      // Process the node.
      (node.process)();

      // Reset the node's indegree.
      node.indegree.store(node.incoming.len(), Ordering::Relaxed);

      // Decrement the indegree of each neighbor.
      for outgoing in &node.outgoing {
        // Safety: we are only acquiring an immutable reference.
        let node = unsafe { &*self.nodes[*outgoing].get() };
        if node.indegree.fetch_sub(1, Ordering::Relaxed) == 0 {
          // If the indegree hits zero, push the index into the queue.
          self.queue.push(*outgoing).ok();
        }
      }

      // Increment the counter.
      self.counter.fetch_add(1, Ordering::Relaxed);
    }
  }
}
```

The one quirk is that at the bottom of the `audio_thread()` implementation, we need to spin until all the nodes have been processed by worker threads. This avoids a condition where the worker threads are outpaced by the audio thread, which finishes draining the queue before worker threads have finished processing the last node they each drained from the queue. The amount of time spent spinning is not large, and in the worst case it is still a finite amount of time, so this is not *really* unsafe for realtime.

There are some "gotchas" to be aware of:

- The queue implementation must have "exactly-once" delivery semantics.
- Worker threads must be panic safe, otherwise the audio thread blocks indefinitely.
- The nodes and all state the process functions wraps are owned by the `Graph`

Not covered here, that will be touched on by a future note:

- Allocating memory for the input/output of nodes.
- Updating the graph's nodes and edges at runtime.
- Waking and sleeping the worker threads.
