//! Audio buffer types.
//!
//! - [Audio]: immutable audio buffer.
//! - [AudioMut]: mutable audio buffer.
//!
//! The internal structure of these types is designed to allow trivial conversion between [Audio]
//! and [AudioMut], as well as allow [AudioMut] to `impl AsRef<Audio>`.
//!
use core::f32;
use std::{
    marker::PhantomData,
    ops::{Index, IndexMut},
    ptr::{NonNull, null_mut},
};

use util::collections::{Array, Stack};

use crate::NO_CONSTANT_VALUE;

pub struct Arena {
    slab: *mut f32,
    max_num_channels: usize,
    max_num_frames: usize,
    stack: Stack<*mut f32>,
}

pub struct Audio {
    pub num_channels: u32,
    pub num_frames: u32,
    pub value: f32,
    pub channels: Array<*mut f32>,
}

pub struct AudioIter<'a> {
    channels: *const *const f32,
    num_frames: usize,
    num_channels: usize,
    _p: PhantomData<&'a ()>,
}

pub struct AudioIterMut<'a> {
    channels: *const *mut f32,
    num_frames: usize,
    num_channels: usize,
    _p: PhantomData<&'a ()>,
}

impl Arena {
    /// Create a new audio buffer allocator.
    pub fn new(max_num_channels: usize, max_num_frames: usize) -> Self {
        // Maximum number of frames must be divisible by 4.
        debug_assert!(
            max_num_frames % 4 == 0,
            "max_num_frames must be a multiple of 4 for proper alignment"
        );

        // Allocate the slab.
        let slab: *mut f32 = unsafe {
            let layout = std::alloc::Layout::from_size_align_unchecked(
                max_num_channels * max_num_frames * std::mem::size_of::<f32>(),
                16,
            );
            std::alloc::alloc_zeroed(layout).cast()
        };
        debug_assert!(!slab.is_null());

        // Allocate the stack.
        let mut stack = Stack::new(max_num_channels);

        // Fill the stack.
        for idx in 0..max_num_channels {
            let channel = unsafe { slab.add(idx * max_num_frames) };
            stack.push(channel);
        }

        Self {
            slab,
            stack,
            max_num_channels,
            max_num_frames,
        }
    }

    pub unsafe fn realloc(&mut self, max_num_channels: usize, max_num_frames: usize) {
        let slab: *mut f32 = unsafe {
            let layout = std::alloc::Layout::from_size_align_unchecked(
                self.max_num_channels * self.max_num_frames * std::mem::size_of::<f32>(),
                16,
            );
            std::alloc::realloc(
                self.slab.cast(),
                layout,
                max_num_channels * max_num_frames * std::mem::size_of::<f32>(),
            )
            .cast()
        };
        debug_assert!(!slab.is_null());
        self.slab = slab;
        self.max_num_channels = self.max_num_channels;
        self.max_num_frames = self.max_num_frames;
        unsafe { self.reset() };
    }

    fn alloc(&mut self) -> Option<NonNull<f32>> {
        self.stack
            .pop()
            .map(|ptr| unsafe { NonNull::new_unchecked(ptr) })
    }

    fn dealloc(&mut self, ptr: *mut f32) {
        self.stack.push(ptr);
    }

    pub fn acquire(&mut self, audio: &mut Audio) -> bool {
        for idx in 0..audio.num_channels {
            let Some(channel) = self.alloc() else {
                return false;
            };
            audio.channels[idx] = channel.as_ptr();
        }
        true
    }

    pub fn release(&mut self, audio: &mut Audio) {
        for idx in 0..audio.num_channels {
            self.dealloc(audio.channels[idx]);
        }
    }

    pub unsafe fn reset(&mut self) {
        self.stack.clear();
        for idx in 0..self.max_num_channels {
            let channel = unsafe { self.slab.add(idx * self.max_num_frames) };
            self.stack.push(channel);
        }
    }
}

impl Audio {
    /// Create a new non-owned buffer of immutable audio data.
    pub fn new(num_channels: u32) -> Self {
        let channels = Array::from(vec![null_mut(); num_channels.try_into().unwrap()]);
        Self {
            num_channels,
            num_frames: 0,
            channels,
            value: NO_CONSTANT_VALUE,
        }
    }

    pub unsafe fn from_raw(channels: *const *mut f32, num_channels: u32, num_frames: u32) -> Self {
        let mut this = Self::new(num_channels);
        this.num_frames = num_frames;
        for i in 0..num_channels {
            this.channels[i] = unsafe { *channels.add(i.try_into().unwrap()) };
        }
        this
    }

    /// Get the number of channels in the buffer.
    pub fn num_channels(&self) -> u32 {
        self.num_channels
    }

    /// Return the number of frames per channel.
    pub fn num_frames(&self) -> u32 {
        self.num_frames
    }

    /// Set this buffer to a constant value.
    pub fn set_constant_value(&mut self, value: f32) {
        debug_assert!(!value.is_nan(), "cannot set a constant to be NaN");
        self.value = value;
    }

    /// Unset the constant value.
    pub fn clear_constant_value(&mut self) {
        self.value = NO_CONSTANT_VALUE;
    }

    /// Update the number of frames in the buffer.
    pub fn set_num_frames(&mut self, num_frames: u32) {
        self.num_frames = num_frames;
    }

    pub fn set_num_channels(&mut self, num_channels: u32) {
        self.num_channels = num_channels;
    }

    /// Return the raw channel pointers.
    pub fn raw(&self) -> *const *const f32 {
        self.channels.as_ptr().cast()
    }

    pub fn raw_mut(&mut self) -> *mut *mut f32 {
        self.channels.as_mut_ptr()
    }

    /// Get the constant value.
    pub fn constant_value(&self) -> Option<f32> {
        (!self.value.is_nan()).then_some(self.value)
    }

    /// Iterate channels.
    pub fn iter(&self) -> AudioIter<'_> {
        AudioIter {
            channels: self.raw(),
            num_channels: self.num_channels.try_into().unwrap(),
            num_frames: self.num_frames.try_into().unwrap(),
            _p: PhantomData::default(),
        }
    }

    pub fn iter_mut(&mut self) -> AudioIterMut<'_> {
        AudioIterMut {
            channels: self.raw_mut(),
            num_channels: self.num_channels.try_into().unwrap(),
            num_frames: self.num_frames.try_into().unwrap(),
            _p: PhantomData::default(),
        }
    }

    pub fn assign_to(&mut self, other: &Self) {
        debug_assert!(self.channels.len() <= other.channels.len());
        let len = self.channels.len().min(other.channels.len());
        self.num_channels = other.num_channels;
        self.num_frames = other.num_frames;
        self.channels.as_mut_slice()[0..len].copy_from_slice(&other.channels.as_slice()[0..len]);
    }
}

impl Drop for Arena {
    fn drop(&mut self) {
        unsafe {
            let layout = std::alloc::Layout::from_size_align_unchecked(
                self.max_num_channels * self.max_num_frames * std::mem::size_of::<f32>(),
                16,
            );
            std::alloc::dealloc(self.slab.cast(), layout);
        }
    }
}

impl<Idx> Index<Idx> for Audio
where
    Idx: TryInto<u32>,
{
    type Output = [f32];
    fn index(&self, index: Idx) -> &Self::Output {
        let Ok(index) = index.try_into() else {
            unreachable!()
        };
        debug_assert!(index < self.num_channels);
        unsafe {
            let ptr = *self.raw().add(index.try_into().unwrap());
            let len = self.num_frames.try_into().unwrap();
            std::slice::from_raw_parts(ptr, len)
        }
    }
}

impl<Idx> IndexMut<Idx> for Audio
where
    Idx: TryInto<u32>,
{
    fn index_mut(&mut self, index: Idx) -> &mut Self::Output {
        let Ok(index) = index.try_into() else {
            unreachable!()
        };
        debug_assert!(index < self.num_channels);
        unsafe {
            let ptr = *self.raw_mut().add(index.try_into().unwrap());
            let len = self.num_frames.try_into().unwrap();
            std::slice::from_raw_parts_mut(ptr, len)
        }
    }
}

impl<'a> IntoIterator for &'a Audio {
    type IntoIter = AudioIter<'a>;
    type Item = &'a [f32];
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &'a mut Audio {
    type IntoIter = AudioIterMut<'a>;
    type Item = &'a mut [f32];
    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl<'a> Iterator for AudioIter<'a> {
    type Item = &'a [f32];
    fn next(&mut self) -> Option<Self::Item> {
        (self.num_channels > 0).then(|| unsafe {
            let slice = std::slice::from_raw_parts(*self.channels, self.num_frames);
            self.channels = self.channels.add(1);
            self.num_channels -= 1;
            slice
        })
    }
}

impl<'a> Iterator for AudioIterMut<'a> {
    type Item = &'a mut [f32];
    fn next(&mut self) -> Option<Self::Item> {
        (self.num_channels > 0).then(|| unsafe {
            let slice = std::slice::from_raw_parts_mut(*self.channels, self.num_frames);
            self.channels = self.channels.add(1);
            self.num_channels -= 1;
            slice
        })
    }
}

unsafe impl Send for Audio {}
