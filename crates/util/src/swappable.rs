use std::{
    ops::{Deref, DerefMut}, ptr::null_mut, sync::{atomic::{AtomicPtr, Ordering}, Arc}
};

pub fn swappable<T>(initial_value: T) -> (Reader<T>, Writer<T>) {
    let inner = Arc::new(AtomicPtr::new(Box::into_raw(Box::new(initial_value))));
    let reader = Reader { inner: inner.clone() };
    let writer = Writer { inner };
    (reader, writer)
}

pub struct Reader<T> {
    inner: Arc<AtomicPtr<T>>,
}

pub struct Writer<T> {
    inner: Arc<AtomicPtr<T>>,
}

pub struct Guard<'a, T> {
    reader: &'a Reader<T>,
    pointer: *mut T,
}

const fn sentinal<T>() -> *mut T {
    -1isize as *mut T
}

impl<T> Reader<T> {
    pub fn read(&mut self) -> Guard<'_, T> {
        let mut pointer = self.inner.load(Ordering::Relaxed);
        loop {
            match self.inner.compare_exchange_weak(
                pointer,
                sentinal(),
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_old) => break,
                Err(old) => {
                    pointer = old;
                    std::hint::spin_loop();
                }
            }
        }
        Guard {
            reader: self,
            pointer,
        }
    }
}

impl<T> Writer<T> {
    #[allow(unused_mut)]
    pub fn write(&mut self, value: T) {
        let mut old = self.inner.load(Ordering::Relaxed);
        let new = Box::into_raw(Box::new(value));
        loop {
            match self
                .inner
                .compare_exchange_weak(old, new, Ordering::AcqRel, Ordering::Relaxed)
            {
                Ok(old) => unsafe {
                    std::ptr::drop_in_place(old);
                },
                Err(old) if old == sentinal() => {
                    std::thread::yield_now();
                    continue;
                }
                Err(old) if old.is_null() => unsafe {
                    std::ptr::drop_in_place(new);
                    break;
                },
                Err(current) => {
                    old = current;
                    std::thread::yield_now();
                }
            }
        }
    }
}

impl<T> Drop for Reader<T> {
    fn drop(&mut self) {
        let mut pointer = self.inner.load(Ordering::Relaxed);
        loop {
            match self.inner.compare_exchange_weak(
                pointer,
                null_mut(),
                Ordering::AcqRel,
                Ordering::Relaxed,
            ) {
                Ok(_old) => break,
                Err(old) => {
                    pointer = old;
                    std::thread::yield_now();
                }
            }
        }
        unsafe {
            std::ptr::drop_in_place(pointer);
        }
    }
}

impl<T> Drop for Guard<'_, T> {
    fn drop(&mut self) {
        self.reader.inner.store(self.pointer, Ordering::Release);
    }
}

impl<T> Deref for Guard<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.pointer }
    }
}

impl<T> DerefMut for Guard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.pointer }
    }
}
