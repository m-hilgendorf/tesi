use std::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicBool, Ordering},
};

use crate::Padded;

pub struct SpinLock<T> {
    locked: Padded<AtomicBool>,
    value: UnsafeCell<T>,
}

unsafe impl<T> Send for SpinLock<T> {}
unsafe impl<T> Sync for SpinLock<T> {}

pub struct Guard<'a, T> {
    lock: &'a SpinLock<T>,
}

impl<T> SpinLock<T> {
    pub fn new(value: T) -> Self {
        Self {
            locked: Padded::new(AtomicBool::new(false)),
            value: UnsafeCell::new(value),
        }
    }

    pub fn lock(&self) -> Guard<'_, T> {
        let mut iters = 1;
        let max_iters = 32;
        loop {
            match self.locked.compare_exchange_weak(
                false,
                true,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(_) => {
                    (0..iters.min(iters)).for_each(|_| std::hint::spin_loop());
                    iters = (iters * 2).min(max_iters);
                }
            }
        }
        Guard { lock: self }
    }

    pub fn into_inner(self) -> T {
        self.value.into_inner()
    }
}

impl<T> Deref for Guard<'_, T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.value.get() }
    }
}

impl<T> DerefMut for Guard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.value.get() }
    }
}

impl<T> Drop for Guard<'_, T> {
    fn drop(&mut self) {
        self.lock.locked.store(false, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::SpinLock;

    #[test]
    fn spin_lock() {
        let value = Arc::new(SpinLock::new(0i32));
        let t1 = std::thread::spawn({
            let value = value.clone();
            move || {
                for _ in 0..32 {
                    let mut value = value.lock();
                    std::thread::sleep(std::time::Duration::from_millis(1));
                    *value += 1;
                }
            }
        });
        let t2 = std::thread::spawn({
            let value = value.clone();
            move || {
                for _ in 0..32 {
                    let mut value = value.lock();
                    std::thread::sleep(std::time::Duration::from_millis(1));
                    *value += 1;
                }
            }
        });
        t1.join().unwrap();
        t2.join().unwrap();
        let value = Arc::into_inner(value).unwrap().into_inner();
        assert_eq!(value, 64);
    }
}
