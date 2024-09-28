use std::{
    cell::UnsafeCell,
    ops::{Index, IndexMut},
    ptr::{null, null_mut},
};

use tesi_util::IsSendSync;

pub struct AudioBus {
    pub(crate) num_frames: usize,
    pub(crate) ptrs: Box<[IsSendSync<UnsafeCell<*const f32>>]>,
}

pub struct AudioBusMut {
    pub(crate) num_frames: usize,
    pub(crate) ptrs: Box<[IsSendSync<UnsafeCell<*mut f32>>]>,
}

pub struct Iter<'a> {
    bus: &'a AudioBus,
    idx: usize,
}

pub struct IterMut<'a> {
    bus: &'a mut AudioBusMut,
    idx: usize,
}

impl AudioBus {
    pub fn new(num_channels: usize) -> Self {
        let num_frames = 0;
        let mut ptrs = Vec::with_capacity(num_channels);
        for _ in 0..num_channels {
            ptrs.push(IsSendSync::new(UnsafeCell::new(null())));
        }
        Self {
            num_frames,
            ptrs: ptrs.into_boxed_slice(),
        }
    }

    pub fn iter(&self) -> Iter<'_> {
        Iter { bus: self, idx: 0 }
    }
}

impl AudioBusMut {
    pub fn new(num_channels: usize) -> Self {
        let num_frames = 0;
        let mut ptrs = Vec::with_capacity(num_channels);
        for _ in 0..num_channels {
            ptrs.push(IsSendSync::new(UnsafeCell::new(null_mut())));
        }
        Self {
            num_frames,
            ptrs: ptrs.into_boxed_slice(),
        }
    }

    pub(crate) unsafe fn push(&self, other: &AudioBus) {
        debug_assert_eq!(self.ptrs.len(), other.ptrs.len());
        let src = self.ptrs.as_ptr().cast();
        let dst = other.ptrs.as_ptr().cast_mut();
        let count = self.ptrs.len() * std::mem::size_of::<*const f32>();
        std::ptr::copy_nonoverlapping(src, dst, count);
    }

    pub fn iter(&mut self) -> IterMut<'_> {
        IterMut { bus: self, idx: 0 }
    }
}

impl Index<usize> for AudioBus {
    type Output = [f32];
    fn index(&self, index: usize) -> &Self::Output {
        debug_assert!(index < self.ptrs.len());
        unsafe {
            let data = *self.ptrs.get_unchecked(index).get();
            debug_assert!(!data.is_null());
            std::slice::from_raw_parts(data, self.num_frames)
        }
    }
}

impl Index<usize> for AudioBusMut {
    type Output = [f32];
    fn index(&self, index: usize) -> &Self::Output {
        debug_assert!(index < self.ptrs.len());
        unsafe {
            let data = *self.ptrs.get_unchecked(index).get();
            debug_assert!(!data.is_null());
            std::slice::from_raw_parts(data, self.num_frames)
        }
    }
}

impl IndexMut<usize> for AudioBusMut {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        debug_assert!(index < self.ptrs.len());
        unsafe {
            let data = *self.ptrs.get_unchecked(index).get();
            debug_assert!(!data.is_null());
            std::slice::from_raw_parts_mut(data, self.num_frames)
        }
    }
}

impl<'a> Iterator for Iter<'a> {
    type Item = &'a [f32];
    fn next(&mut self) -> Option<Self::Item> {
        if self.idx == self.bus.ptrs.len() {
            return None;
        }
        let buffer = &self.bus[self.idx];
        self.idx += 1;
        Some(buffer)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.bus.ptrs.len() - self.idx;
        (len, Some(len))
    }
}

impl<'a> Iterator for IterMut<'a> {
    type Item = &'a mut [f32];
    fn next(&mut self) -> Option<Self::Item> {
        if self.idx == self.bus.ptrs.len() {
            return None;
        }
        let buffer = unsafe {
            let data = *self.bus.ptrs[self.idx].get();
            let len = self.bus.num_frames;
            std::slice::from_raw_parts_mut(data, len)
        };
        self.idx += 1;
        Some(buffer)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.bus.ptrs.len() - self.idx;
        (len, Some(len))
    }
}

impl<'a> IntoIterator for &'a AudioBus {
    type IntoIter = Iter<'a>;
    type Item = &'a [f32];
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &'a mut AudioBusMut {
    type IntoIter = IterMut<'a>;
    type Item = &'a mut [f32];
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}