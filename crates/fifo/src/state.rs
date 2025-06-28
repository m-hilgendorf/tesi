use std::{
    alloc::{Layout, alloc_zeroed, dealloc},
    sync::atomic::AtomicUsize,
};

pub(crate) struct State<T> {
    pub(crate) head: AtomicUsize,
    pub(crate) tail: AtomicUsize,
    pub(crate) capacity: usize,
    pub(crate) align: usize,
    pub(crate) data: *mut T,
}

impl<T> State<T> {
    pub(crate) fn new(capacity: usize, align: usize, init: impl Fn() -> T) -> Self {
        unsafe {
            let layout = Layout::from_size_align(capacity * std::mem::size_of::<T>(), align)
                .expect("invalid alignment");
            let data = alloc_zeroed(layout).cast::<T>();
            (0..capacity).for_each(|k| std::ptr::write(data.add(k), init()));
            Self {
                head: AtomicUsize::new(0),
                tail: AtomicUsize::new(0),
                capacity,
                align,
                data,
            }
        }
    }
}

impl<T> Drop for State<T> {
    fn drop(&mut self) {
        unsafe {
            let layout = Layout::from_size_align_unchecked(
                self.capacity * std::mem::size_of::<T>(),
                self.align,
            );
            dealloc(self.data.cast(), layout);
        }
    }
}
