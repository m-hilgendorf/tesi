#![allow(dead_code)]
use crate::{
    bus::{AudioBus, AudioBusMut},
    renderer,
};
use crossbeam::queue::ArrayQueue;
use std::{
    mem::MaybeUninit,
    ptr::{null, null_mut},
};

pub struct Allocator {
    pub(crate) queue: ArrayQueue<*mut f32>,
}

pub(crate) struct SlabAllocator<T> {
    pub(crate) slab_size: usize,
    pub(crate) pointers: Vec<*mut T>,
    pub(crate) data: Vec<MaybeUninit<T>>,
}

impl Allocator {
    pub(crate) unsafe fn assign(&self, bus: &AudioBus) {
        for index in 0..bus.ptrs.len() {
            let channel = self.queue.pop().unwrap();
            *bus.ptrs[index].get() = channel;
        }
    }

    pub(crate) unsafe fn assign_mut(&self, bus: &AudioBusMut) {
        for index in 0..bus.ptrs.len() {
            let channel = self.queue.pop().unwrap();
            *bus.ptrs[index].get() = channel;
        }
    }

    pub(crate) unsafe fn release(&self, bus: &AudioBus) {
        for index in 0..bus.ptrs.len() {
            let channel = (*bus.ptrs[index].get()).cast_mut();
            *bus.ptrs[index].get() = null();
            self.queue.push(channel).unwrap();
        }
    }

    pub(crate) unsafe fn release_mut(&self, bus: &AudioBusMut) {
        for index in 0..bus.ptrs.len() {
            let channel = *bus.ptrs[index].get();
            *bus.ptrs[index].get() = null_mut();
            self.queue.push(channel).unwrap();
        }
    }
}

unsafe impl Send for Allocator {}
unsafe impl Sync for Allocator {}

impl<T> SlabAllocator<T> {
    pub fn new(slab_size: usize) -> Self {
        Self {
            slab_size,
            pointers: vec![],
            data: vec![],
        }
    }

    pub fn alloc(&mut self) -> *mut T {
        if let Some(ptr) = self.pointers.pop() {
            return ptr;
        }
        self.data.reserve_exact(self.slab_size);
        unsafe {
            let ptr = self.data.as_mut_ptr().add(self.data.len());
            self.data.set_len(self.data.len() + self.slab_size);
            ptr.cast()
        }
    }

    pub fn dealloc(&mut self, ptr: *mut T) {
        self.pointers.push(ptr);
    }
}

pub(crate) fn compile(
    input_node: usize,
    output_node: usize,
    max_num_frames: usize,
    num_workers: usize,
    nodes: &[renderer::Node],
) -> (Allocator, Vec<MaybeUninit<f32>>) {
    let mut alloc: SlabAllocator<f32> = SlabAllocator::new(max_num_frames);
    let mut max_breadth = 0;
    unsafe {
        let mut breadth = 0;
        for (index, node) in nodes.iter().enumerate() {
            if index != input_node {
                for (index, incoming) in node.incoming.iter().enumerate() {
                    let bus = &mut *(*node.audio_inputs.get())[index].get();
                    breadth += bus.ptrs.len();

                    for channel in 0..bus.ptrs.len() {
                        if incoming.is_none() {
                            let ptr = alloc.alloc();
                            for n in 0..max_num_frames {
                                std::ptr::write(ptr.add(n), 0.0);
                            }
                            *bus.ptrs[channel].get() = ptr.cast();
                        }
                        let ptr = *bus.ptrs[channel].get();
                        alloc.dealloc(ptr.cast_mut());
                    }
                }
            }

            if index != output_node {
                for (index, outgoing) in node.outgoing.iter().enumerate() {
                    let bus = &mut *(*node.audio_outputs.get())[index].get();
                    breadth += bus.ptrs.len();

                    for channel in 0..bus.ptrs.len() {
                        let ptr = alloc.alloc();
                        *bus.ptrs[channel].get() = ptr;

                        if outgoing.is_none() {
                            for n in 0..max_num_frames {
                                std::ptr::write(ptr.add(n), 0.0);
                            }
                            alloc.dealloc(ptr);
                        }
                    }
                }
            }

            max_breadth = max_breadth.max(breadth);
        }

        for _ in 0..(max_breadth * num_workers) {
            let ptr = alloc.alloc();
            alloc.dealloc(ptr);
        }
    }

    let SlabAllocator { pointers, data, .. } = alloc;

    let queue = ArrayQueue::new(pointers.len());
    for ptr in pointers {
        queue.push(ptr).ok();
    }
    let alloc = Allocator { queue };
    (alloc, data)
}