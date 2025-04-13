//! Audio buffer types.
//!
//! - [Audio]: immutable audio buffer, owned or not-owned.
//! - [AudioMut]: mutable audio buffer, owend or not-owned.
//!
//! The internal structure of these types is designed to allow trivial conversion between [Audio]
//! and [AudioMut], as well as allow [AudioMut] to `impl AsRef<Audio>`.
//!
//! Safety:
//!
//! - Typical users will want to use [AudioMut::new] to construct an owned, mutable chunk of multi-
//!   channel audio.
//! - Advanced users may want to create [Audio] and [AudioMut] instances that don't own their
//!   underlying data but do not wish to allocate when re-binding it to other instances, or changing
//!   the buffer size at run time.
//!
use core::f32;
use std::{
    marker::PhantomData,
    ops::{Index, IndexMut},
    ptr::NonNull,
};

#[repr(C)]
pub struct Audio {
    pub(crate) num_channels: usize,
    pub(crate) num_frames: usize,
    pub(crate) channels: *mut *const f32,
    pub(crate) owned: Option<NonNull<f32>>,
    pub(crate) value: f32,
}

#[repr(C)]
pub struct AudioMut {
    pub(crate) num_channels: usize,
    pub(crate) num_frames: usize,
    pub(crate) channels: *mut *mut f32,
    pub(crate) owned: Option<NonNull<f32>>,
    pub(crate) value: f32,
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

impl Audio {
    /// Create a new (owned) buffer of immutable audio data. You almost certainly want to use [AudioMut] instead.
    pub fn new(num_channels: usize, num_frames: usize) -> Self {
        let (channels, owned) = unsafe {
            let layout = std::alloc::Layout::from_size_align_unchecked(
                num_channels * num_frames * std::mem::size_of::<f32>(),
                std::mem::align_of::<f32>(),
            );
            let ptr = std::alloc::alloc_zeroed(layout).cast();
            let owned = NonNull::new(ptr);

            let layout = std::alloc::Layout::from_size_align_unchecked(
                num_channels * std::mem::size_of::<*const f32>(),
                std::mem::align_of::<*const f32>(),
            );
            let channels = std::alloc::alloc(layout).cast();
            (channels, owned)
        };
        Self {
            num_channels,
            num_frames,
            channels,
            owned,
            value: f32::NAN,
        }
    }

    /// Create a new non-owned buffer of immutable audio data.
    ///
    /// Safety: the caller is responsible for calling [Audio::bind] and [Audio::set_num_frames] in
    /// order to fully initialize the data. This interface exists to create an audio buffer whose
    /// underlying pointers are mapped lazily or updated without allocation.
    pub unsafe fn non_owned(num_channels: usize) -> Self {
        let channels = unsafe {
            let layout = std::alloc::Layout::from_size_align_unchecked(
                num_channels * std::mem::size_of::<*const f32>(),
                std::mem::align_of::<*const f32>(),
            );
            std::alloc::alloc(layout).cast()
        };
        Self {
            num_channels,
            num_frames: 0,
            channels,
            owned: None,
            value: f32::NAN,
        }
    }

    /// Get the number of channels in the buffer.
    pub fn num_channels(&self) -> usize {
        self.num_channels
    }

    /// Return the number of frames per channel.
    pub fn num_frames(&self) -> usize {
        self.num_frames
    }

    /// Set this buffer to a constant value.
    pub fn set_constant_value(&mut self, value: f32) {
        debug_assert!(!value.is_nan(), "cannot set a constant to be NaN");
        self.value = value;
    }

    /// Unset the constant value.
    pub fn clear_constant_value(&mut self) {
        self.value = f32::NAN;
    }

    /// Update the number of frames in the buffer.
    ///
    /// Safety: this is only valid with a non-owned buffer.
    pub unsafe fn set_num_frames(&mut self, num_frames: usize) {
        debug_assert!(self.owned.is_none());
        self.num_frames = num_frames;
    }

    /// Return the raw channel pointers.
    pub unsafe fn raw(&self) -> *const *const f32 {
        self.channels
    }

    /// Get the constant value.
    pub fn constant_value(&self) -> Option<f32> {
        (!self.value.is_nan()).then_some(self.value)
    }

    /// Update the underlying channel pointers.
    /// Safety: This method is only valid on non-owned buffers. The number of values in the iterator
    /// must match the number of channels in the buffer.
    pub unsafe fn bind(&mut self, ptrs: impl Iterator<Item = *const f32>) {
        debug_assert!(self.owned.is_none());
        for (offset, ptr) in ptrs.into_iter().enumerate() {
            debug_assert!(offset < self.num_channels);
            unsafe {
                *self.channels.add(offset) = ptr;
            }
        }
    }

    /// Iterate channels.
    pub fn iter(&self) -> AudioIter<'_> {
        AudioIter {
            channels: self.channels,
            num_channels: self.num_channels,
            num_frames: self.num_frames,
            _p: PhantomData::default(),
        }
    }
}

impl AudioMut {
    pub fn new(num_channels: usize, num_frames: usize) -> Self {
        let (channels, owned) = unsafe {
            let layout = std::alloc::Layout::from_size_align_unchecked(
                num_channels * num_frames * std::mem::size_of::<f32>(),
                std::mem::align_of::<f32>(),
            );
            let ptr = std::alloc::alloc_zeroed(layout).cast();
            let owned = NonNull::new(ptr);

            let layout = std::alloc::Layout::from_size_align_unchecked(
                num_channels * num_frames * std::mem::size_of::<*mut f32>(),
                std::mem::align_of::<*mut f32>(),
            );
            let channels = std::alloc::alloc(layout).cast();
            (channels, owned)
        };
        Self {
            num_channels,
            num_frames,
            channels,
            owned,
            value: f32::NAN,
        }
    }

    pub unsafe fn non_owned(num_channels: usize) -> Self {
        let channels = unsafe {
            let layout = std::alloc::Layout::from_size_align_unchecked(
                num_channels * std::mem::size_of::<*mut f32>(),
                std::mem::align_of::<*mut f32>(),
            );
            std::alloc::alloc(layout).cast()
        };
        Self {
            num_channels,
            num_frames: 0,
            channels,
            owned: None,
            value: f32::NAN,
        }
    }

    pub fn num_frames(&self) -> usize {
        self.num_frames
    }

    pub fn num_channels(&self) -> usize {
        self.num_channels
    }

    pub fn set_constant_value(&mut self, value: f32) {
        debug_assert!(!value.is_nan(), "cannot set a constant to be NaN");
        self.value = value;
    }

    pub fn set_num_frames(&mut self, num_frames: usize) {
        self.num_frames = num_frames;
    }

    /// Return the raw pointers.
    pub fn raw(&self) -> *const *mut f32 {
        self.channels
    }

    /// Return a constant value for the whole buffer, if it exists.
    pub fn constant_value(&self) -> Option<f32> {
        (!self.value.is_nan()).then_some(self.value)
    }

    /// Bind channels to pointers.
    pub unsafe fn bind(&mut self, ptrs: impl Iterator<Item = *mut f32>) {
        debug_assert!(self.owned.is_none());
        for (offset, ptr) in ptrs.into_iter().enumerate() {
            debug_assert!(offset < self.num_channels);
            unsafe {
                *self.channels.add(offset) = ptr;
            }
        }
    }

    pub fn iter(&self) -> AudioIter<'_> {
        AudioIter {
            channels: self.channels.cast(),
            num_frames: self.num_frames,
            num_channels: self.num_channels,
            _p: PhantomData::default(),
        }
    }

    pub fn iter_mut(&self) -> AudioIterMut<'_> {
        AudioIterMut {
            channels: self.channels,
            num_frames: self.num_frames,
            num_channels: self.num_channels,
            _p: PhantomData::default(),
        }
    }
}

impl Drop for Audio {
    fn drop(&mut self) {
        unsafe {
            let layout = std::alloc::Layout::from_size_align_unchecked(
                self.num_channels * std::mem::size_of::<*const f32>(),
                std::mem::align_of::<*const f32>(),
            );
            std::alloc::dealloc(self.channels.cast(), layout);
        }
    }
}

impl Drop for AudioMut {
    fn drop(&mut self) {
        unsafe {
            let layout = std::alloc::Layout::from_size_align_unchecked(
                self.num_channels * std::mem::size_of::<*const f32>(),
                std::mem::align_of::<*const f32>(),
            );
            std::alloc::dealloc(self.channels.cast(), layout);
        }
    }
}

impl Index<usize> for Audio {
    type Output = [f32];
    fn index(&self, index: usize) -> &Self::Output {
        debug_assert!(index < self.num_channels);
        unsafe {
            let ptr = *self.channels.add(index);
            let len = self.num_frames;
            std::slice::from_raw_parts(ptr, len)
        }
    }
}

impl Index<usize> for AudioMut {
    type Output = [f32];
    fn index(&self, index: usize) -> &Self::Output {
        debug_assert!(index < self.num_channels);
        unsafe {
            let ptr = *self.channels.add(index);
            let len = self.num_frames;
            std::slice::from_raw_parts(ptr, len)
        }
    }
}

impl IndexMut<usize> for AudioMut {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        debug_assert!(index < self.num_channels);
        unsafe {
            let ptr = *self.channels.add(index);
            let len = self.num_frames;
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

impl<'a> IntoIterator for &'a AudioMut {
    type IntoIter = AudioIter<'a>;
    type Item = &'a [f32];
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &'a mut AudioMut {
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

impl AsRef<Audio> for AudioMut {
    fn as_ref(&self) -> &Audio {
        unsafe { std::mem::transmute(self) }
    }
}

unsafe impl Send for Audio {}
unsafe impl Send for AudioMut {}

impl Into<Audio> for AudioMut {
    fn into(self) -> Audio {
        unsafe { std::mem::transmute(self) }
    }
}

impl Into<AudioMut> for Audio {
    fn into(self) -> AudioMut {
        unsafe { std::mem::transmute(self) }
    }
}
