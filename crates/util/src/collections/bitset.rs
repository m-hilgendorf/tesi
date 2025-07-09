#[derive(Clone, Debug)]
pub struct BitSet {
    inner: Vec<u64>,
}

impl BitSet {
    pub fn new() -> Self {
        Self {
            inner: Vec::with_capacity(1),
        }
    }

    pub fn with_capacity(capacity: impl TryInto<usize>) -> Self {
        let capacity = crate::cast_usize!(capacity);
        let inner = vec![0; capacity / 64];
        Self { inner }
    }

    #[inline]
    pub fn set(&mut self, n: impl TryInto<usize>) {
        let n = crate::cast_usize!(n);
        let word = n / 64;
        let bit = n % 64;

        if word > self.inner.len() {
            self.inner.resize_with(word, || 0);
        }

        // Safety: this can never be out of bounds given the resize above.
        unsafe {
            *self.inner.get_unchecked_mut(word) |= 1 << bit;
        }
    }

    #[inline]
    pub fn clear(&mut self, n: impl TryInto<usize>) {
        let n = crate::cast_usize!(n);
        let word = n / 64;
        let bit = n % 64;

        if word > self.inner.len() {
            self.inner.resize_with(word, || 0);
        }

        // Safety: this can never be out of bounds given the resize above.
        unsafe {
            *self.inner.get_unchecked_mut(word) &= !(1 << bit);
        }
    }

    #[inline]
    pub fn get(&self, n: impl TryInto<usize>) -> bool {
        let n = crate::cast_usize!(n);
        let word = n / 64;
        let bit = n % 64;
        self.inner
            .get(word)
            .is_some_and(|word| *word & (1 << bit) != 0)
    }
}
