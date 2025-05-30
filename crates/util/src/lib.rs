use std::ops::{Deref, DerefMut};
pub mod swappable;
pub mod array;
pub mod deref;

#[cfg_attr(
    any(target_arch = "x86_64", target_arch = "aarch64",),
    repr(align(128))
)]
#[cfg_attr(
    not(any(target_arch = "x86_64", target_arch = "aarch64",)),
    repr(align(64))
)]
pub struct Padded<T>(T);

impl<T> Deref for Padded<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for Padded<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[repr(transparent)]
pub struct IsSend<T: ?Sized>(T);

#[repr(transparent)]
pub struct IsSendSync<T: ?Sized>(T);

impl<T> IsSend<T> {
    pub fn new(value: T) -> Self {
        Self(value)
    }
    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> IsSendSync<T> {
    pub fn new(value: T) -> Self {
        Self(value)
    }
    pub fn into_inner(self) -> T {
        self.0
    }
}

unsafe impl<T: ?Sized> Send for IsSend<T> {}
unsafe impl<T: ?Sized> Send for IsSendSync<T> {}
unsafe impl<T: ?Sized> Sync for IsSendSync<T> {}

impl<T: ?Sized> Deref for IsSend<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: ?Sized> DerefMut for IsSend<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T: ?Sized> Deref for IsSendSync<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: ?Sized> DerefMut for IsSendSync<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T: ?Sized> AsRef<T> for IsSend<T> {
    fn as_ref(&self) -> &T {
        &self.0
    }
}

impl<T: ?Sized> AsRef<T> for IsSendSync<T> {
    fn as_ref(&self) -> &T {
        &self.0
    }
}

pub fn start_trace(_id: &str) {}
pub fn end_trace(_id: &str) {}
pub fn rt_error(_msg: &str) {}

pub struct Stack<T> {
    inner: Vec<T>
}

impl<T> Stack<T> {
    pub fn new(capacity: usize) -> Self {
        Self { inner: Vec::with_capacity(capacity) }
    }

    pub fn push(&mut self, value: T) {
        debug_assert!(self.inner.len() < self.inner.capacity());
        self.inner.push(value);
    }

    pub fn pop(&mut self) -> Option<T> {
        self.inner.pop()
    }

    pub fn clear(&mut self) {
        self.inner.clear();
    }
}

impl<T> Padded<T> {
    pub fn new(value: T) -> Self { Self(value) }
}
