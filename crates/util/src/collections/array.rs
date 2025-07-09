use std::{ops::{Deref, DerefMut, Index, IndexMut}};

#[repr(transparent)]
pub struct Array<T> {
    inner: Box<[T]>,
}

impl<T> Array<T> {
    pub fn as_slice(&self) -> &[T] {
        self
    }

    pub fn as_mut_slice(&mut self) -> &mut [T] {
        self
    }
}

impl<T> From<Vec<T>> for Array<T> {
    fn from(value: Vec<T>) -> Self {
        Self { inner: value.into_boxed_slice() }
    }
}

impl<T> Deref for Array<T> {
    type Target = [T];
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl<T> DerefMut for Array<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl<Idx, T> Index<Idx> for Array<T>
where
    Idx: TryInto<usize>
{
    type Output = T;
    fn index(&self, index: Idx) -> &Self::Output {
        let Ok(index) = index.try_into() else {
            unreachable!()
        };
        debug_assert!(index < self.len());
        unsafe { self.get_unchecked(index) }
    }
}

impl<Idx, T> IndexMut<Idx> for Array<T>
where
    Idx: TryInto<usize>
{
    fn index_mut(&mut self, index: Idx) -> &mut Self::Output {
        let Ok(index) = index.try_into() else {
            unreachable!()
        };
        debug_assert!(index < self.len());
        unsafe { self.get_unchecked_mut(index) }
    }
}

#[cfg(test)]
mod tests {
    use super::Array;

    #[test]
    fn compiles() {
        let array = Array::from(vec![1i32, 2, 3]);
        let _0 = array[0u32];
        let _1 = array[1u32];
        let _2 = array[2u32];
    }
}