use crossbeam::queue::ArrayQueue;
use std::{
    cell::UnsafeCell,
    marker::PhantomData,
    mem::MaybeUninit,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex, RwLock, Weak,
    },
    thread::JoinHandle,
};
use tesi_util::IsSendSync;

use crate::{
    alloc::Allocator,
    bus::{AudioBus, AudioBusMut},
    graph,
    proc::{self, Processor},
};

#[derive(Clone)]
pub struct Renderer {
    pub(crate) graph: Weak<RwLock<graph::Inner>>,
    pub(crate) inner: Arc<Inner>,
    pub(crate) _p: PhantomData<*mut ()>,
}

pub(crate) struct Inner {
    pub(crate) state: IsSendSync<UnsafeCell<triple_buffer::Output<State>>>,
    pub(crate) num_frames: AtomicUsize,
    pub(crate) num_workers: usize,
    pub(crate) worker_state: AtomicUsize,
    pub(crate) workers: Mutex<Vec<JoinHandle<()>>>,
}

pub(crate) struct State {
    pub(crate) queue: ArrayQueue<usize>,
    pub(crate) alloc: Allocator,
    pub(crate) nodes: Vec<Node>,
    pub(crate) input_node: usize,
    pub(crate) output_node: usize,
    pub(crate) sources: Vec<usize>,
    pub(crate) _data: Vec<MaybeUninit<f32>>,
}

pub(crate) struct Node {
    pub(crate) audio_inputs: AudioInputs,
    pub(crate) audio_outputs: AudioOutputs,
    pub(crate) num_bound_inputs: AtomicUsize,
    pub(crate) incoming: Box<[Option<(usize, usize)>]>,
    pub(crate) outgoing: Box<[Option<(usize, usize)>]>,
    pub(crate) processor: Arc<IsSendSync<UnsafeCell<dyn Processor>>>,
}

type AudioInputs = IsSendSync<UnsafeCell<Box<[IsSendSync<UnsafeCell<AudioBus>>]>>>;
type AudioOutputs = IsSendSync<UnsafeCell<Box<[IsSendSync<UnsafeCell<AudioBusMut>>]>>>;

const WORKER_EXIT: usize = 0;
const WORKER_PARK: usize = 1;
const WORKER_SPIN: usize = 2;
const WORKER_WORK: usize = 3;

impl Renderer {
    pub fn initialize(&mut self, sample_rate: f64, max_buffer_size: usize) {
        unsafe {
            let receiver = &mut (*self.inner.state.get());
            receiver.update();

            let state = receiver.output_buffer();
            for node in &mut state.nodes {
                (*node.processor.get()).initialize(sample_rate, max_buffer_size);
            }
        }
        self.inner
            .worker_state
            .store(WORKER_SPIN, Ordering::Relaxed);
        let workers = self.inner.workers.lock().unwrap();
        for worker in workers.iter() {
            worker.thread().unpark();
        }
    }

    pub fn render(
        &mut self,
        inputs: *const *const f32,
        outputs: *mut *mut f32,
        num_inputs: usize,
        num_outputs: usize,
        num_frames: usize,
    ) {
        self.inner
            .audio_thread(inputs, outputs, num_inputs, num_outputs, num_frames)
    }

    pub fn reset(&mut self) {
        self.inner
            .worker_state
            .store(WORKER_PARK, Ordering::Relaxed);
        unsafe {
            let state = (*self.inner.state.get()).output_buffer();
            for node in &mut state.nodes {
                (*node.processor.get()).reset();
            }
        }
    }
}

impl Inner {
    pub(crate) fn new(num_workers: usize, receiver: triple_buffer::Output<State>) -> Arc<Self> {
        let num_frames = AtomicUsize::new(0);
        let state = IsSendSync::new(UnsafeCell::new(receiver));
        let worker_state = AtomicUsize::new(WORKER_PARK);
        let inner = Arc::new(Inner {
            state,
            num_frames,
            num_workers,
            worker_state,
            workers: Mutex::new(vec![]),
        });

        let threads = (0..num_workers)
            .map(|_| {
                let inner = inner.clone();
                std::thread::spawn(move || {
                    inner.worker_thread();
                })
            })
            .collect();

        *inner.workers.lock().unwrap() = threads;
        inner
    }

    pub fn audio_thread(
        &self,
        inputs: *const *const f32,
        outputs: *const *mut f32,
        num_inputs: usize,
        num_outputs: usize,
        num_frames: usize,
    ) {
        // Update the current number of frames.
        self.num_frames.store(num_frames, Ordering::Relaxed);

        let state = unsafe {
            let output = &mut *self.state.get();
            output.update();
            output.peek_output_buffer()
        };

        // Bind inputs.
        let input_node = &state.nodes[state.input_node];
        unsafe {
            let input_bus = &*(*input_node.audio_inputs.get())[0].get();
            debug_assert_eq!(num_inputs, input_bus.ptrs.len());
            for index in 0..num_inputs {
                *input_bus.ptrs[index].get() = *inputs.add(index);
            }
        }

        // Bind outputs.
        let output_node = &state.nodes[state.output_node];
        unsafe {
            let output_bus = &*(*output_node.audio_outputs.get())[0].get();
            debug_assert_eq!(num_inputs, output_bus.ptrs.len());
            for index in 0..num_outputs {
                *output_bus.ptrs[index].get() = *outputs.add(index);
            }
        }

        // Special case: single threaded rendering.
        if self.num_workers == 0 {
            for node in &state.nodes {
                unsafe {
                    node.process_single_threaded(num_frames, &state.nodes);
                }
            }
            return;
        }

        // Fill the queue.
        state.queue.push(state.input_node).ok();
        for source in &state.sources {
            state.queue.push(*source).ok();
        }

        // Signal other threads to start working.
        self.worker_state.store(WORKER_WORK, Ordering::Relaxed);

        // Work.
        while let Some(node) = state.queue.pop() {
            let node = &state.nodes[node];
            unsafe {
                node.process_multi_threaded(num_frames, &state.nodes, &state.alloc, &state.queue);
            }
        }

        // Signal other threads to spin.
        self.worker_state.store(WORKER_SPIN, Ordering::Relaxed);
    }

    fn worker_thread(&self) {
        let backoff = crossbeam::utils::Backoff::new();
        loop {
            match self.worker_state.load(Ordering::Relaxed) {
                WORKER_EXIT => break,
                WORKER_PARK => std::thread::park(),
                WORKER_SPIN => {
                    backoff.spin();
                }
                WORKER_WORK => unsafe {
                    let current_num_frames = self.num_frames.load(Ordering::Relaxed);
                    let state = (*self.state.get()).peek_output_buffer();
                    let Some(node) = state.queue.pop() else {
                        backoff.reset();
                        continue;
                    };
                    state.nodes[node].process_multi_threaded(
                        current_num_frames,
                        &state.nodes,
                        &state.alloc,
                        &state.queue,
                    );
                },
                _ => unreachable!(),
            }
        }
    }
}

impl Node {
    unsafe fn process_single_threaded(&self, current_num_frames: usize, nodes: &[Node]) {
        // Get the i/o buffers.
        let audio_inputs: &mut [_] = &mut *self.audio_inputs.get();
        let audio_outputs: &mut [_] = &mut *self.audio_outputs.get();

        // Update the current number of frames.
        for input in audio_inputs.iter_mut() {
            input.get_mut().num_frames = current_num_frames;
        }
        for output in audio_outputs.iter_mut() {
            output.get_mut().num_frames = current_num_frames;
        }

        // Create the context.
        let mut context = proc::Context {
            audio_inputs: std::mem::transmute::<&mut [IsSendSync<UnsafeCell<AudioBus>>], &[AudioBus]>(
                audio_inputs,
            ),
            audio_outputs: std::mem::transmute::<
                &mut [IsSendSync<UnsafeCell<AudioBusMut>>],
                &mut [AudioBusMut],
            >(audio_outputs),
        };

        // Process.
        (*self.processor.get()).process(&mut context);

        // Push outputs to inputs.
        for (output, outgoing) in self.outgoing.iter().copied().enumerate() {
            if let Some((node, input)) = outgoing {
                let output = &*(*self.audio_outputs.get())[output].get();
                let input = &*(*nodes[node].audio_inputs.get())[input].get();
                output.push(input);
            }
        }
    }

    unsafe fn process_multi_threaded(
        &self,
        current_num_frames: usize,
        nodes: &[Node],
        alloc: &Allocator,
        queue: &ArrayQueue<usize>,
    ) {
        // Assign unbound input buffers.
        for (input, incoming) in self.incoming.iter().copied().enumerate() {
            if incoming.is_none() {
                let bus = &*(*self.audio_inputs.get())[input].get();
                alloc.assign(bus);
            }
        }

        // Get the i/o buffers.
        let audio_inputs: &mut [_] = &mut *self.audio_inputs.get();
        let audio_outputs: &mut [_] = &mut *self.audio_outputs.get();

        // Update the current number of frames.
        for input in audio_inputs.iter_mut() {
            input.get_mut().num_frames = current_num_frames;
        }
        for output in audio_outputs.iter_mut() {
            output.get_mut().num_frames = current_num_frames;
        }

        // Create the context.
        let mut context = proc::Context {
            audio_inputs: std::mem::transmute::<&mut [IsSendSync<UnsafeCell<AudioBus>>], &[AudioBus]>(
                audio_inputs,
            ),
            audio_outputs: std::mem::transmute::<
                &mut [IsSendSync<UnsafeCell<AudioBusMut>>],
                &mut [AudioBusMut],
            >(audio_outputs),
        };

        // Process.
        (*self.processor.get()).process(&mut context);

        // Release inputs
        for (input, _) in self.incoming.iter().enumerate() {
            let bus = &*(*self.audio_inputs.get())[input].get();
            alloc.release(bus);
        }
        self.num_bound_inputs.store(0, Ordering::Relaxed);

        // Push outputs to inputs or release unbound outputs.
        for (output, outgoing) in self.outgoing.iter().copied().enumerate() {
            let output = &*(*self.audio_outputs.get())[output].get();
            if let Some((node, input)) = outgoing {
                let input = &*(*nodes[node].audio_inputs.get())[input].get();
                output.push(input);

                if nodes[node].num_bound_inputs.fetch_add(1, Ordering::Relaxed)
                    == (*nodes[node].audio_inputs.get()).len()
                {
                    queue.push(node).unwrap();
                }
            } else {
                alloc.release_mut(output);
            }
        }
    }
}

impl Clone for State {
    fn clone(&self) -> Self {
        Self::new()
    }
}

impl State {
    pub fn new() -> Self {
        Self {
            queue: ArrayQueue::new(1),
            alloc: Allocator {
                queue: ArrayQueue::new(1),
            },
            nodes: vec![],
            input_node: 0,
            output_node: 0,
            sources: vec![],
            _data: vec![],
        }
    }
}

unsafe impl Send for Renderer {}

impl Drop for Renderer {
    fn drop(&mut self) {
        let Some(graph) = self.graph.upgrade() else {
            return;
        };
        let Some(mut graph) = graph.write().ok() else {
            return;
        };
        graph.renderer.replace(Renderer {
            graph: self.graph.clone(),
            inner: self.inner.clone(),
            _p: PhantomData,
        });
    }
}

impl Drop for Inner {
    fn drop(&mut self) {
        self.worker_state.store(WORKER_EXIT, Ordering::Relaxed);
        let mut workers = self.workers.lock().unwrap();
        while let Some(worker) = workers.pop() {
            worker.thread().unpark();
            worker.join().ok();
        }
    }
}
