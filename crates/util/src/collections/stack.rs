/// Fixed capacity stack.
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
