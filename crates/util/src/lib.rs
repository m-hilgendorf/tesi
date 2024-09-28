use std::ops::{Deref, DerefMut};

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
