use std::{
    alloc::{Layout, alloc_zeroed, dealloc},
    ops::{Deref, DerefMut},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

/// The write end of a ring buffer.
pub struct Sender<T> {
    cap: usize,
    state: Arc<State<T>>,
}

/// A write transcation. See [Sender::send].
pub struct SendTxn<'a, T> {
    writer: &'a mut Sender<T>,
    start: usize,
    length: usize,
}

/// The read-end of a ring buffer.
pub struct Receiver<T> {
    cap: usize,
    state: Arc<State<T>>,
}

/// A read transcation. See [Reader::read].
pub struct RecvTxn<'a, T> {
    reader: &'a mut Receiver<T>,
    start: usize,
    length: usize,
}

struct State<T> {
    head: AtomicUsize,
    tail: AtomicUsize,
    cap: usize,
    align: usize,
    data: *mut T,
}

impl<T> State<T> {
    fn new(cap: usize, align: usize, init: impl Fn() -> T) -> Self {
        unsafe {
            debug_assert!(
                cap.is_power_of_two(),
                "fifo capacity must be a power of two"
            );
            let layout = Layout::from_size_align(cap * std::mem::size_of::<T>(), align)
                .expect("invalid alignment");
            let data = alloc_zeroed(layout).cast::<T>();
            (0..cap).for_each(|k| std::ptr::write(data.add(k), init()));
            Self {
                head: AtomicUsize::new(0),
                tail: AtomicUsize::new(0),
                cap,
                align,
                data,
            }
        }
    }

    fn split(self) -> (Sender<T>, Receiver<T>) {
        let state = Arc::new(self);
        let writer = Sender {
            cap: state.cap,
            state: state.clone(),
        };
        let reader = Receiver {
            cap: state.cap,
            state,
        };
        (writer, reader)
    }
}

impl<T> Drop for State<T> {
    fn drop(&mut self) {
        unsafe {
            let layout =
                Layout::from_size_align_unchecked(self.cap * std::mem::size_of::<T>(), self.align);
            dealloc(self.data.cast(), layout);
        }
    }
}

unsafe impl<T> Send for State<T> {}
unsafe impl<T> Sync for State<T> {}

/// Create a new ring buffer with a fixed capacity and initial value within the buffer.
pub fn channel<T>(
    cap: usize,
    align: Option<usize>,
    init: impl Fn() -> T,
) -> (Sender<T>, Receiver<T>) {
    let align = align.unwrap_or(align_of::<T>());
    State::new(cap, align, init).split()
}

impl<T> Receiver<T> {
    pub fn available(&self) -> usize {
        // Load the data.
        let cap = self.cap;
        let head = self.state.head.load(Ordering::Acquire);
        let tail = self.state.tail.load(Ordering::Acquire);

        // Compute the read region.
        let used = head.wrapping_sub(tail);
        let start = tail & (cap - 1);
        let length = if used < cap - start {
            used
        } else {
            cap - start
        };
        length
    }

    fn sender_dropped(&self) -> bool {
        Arc::strong_count(&self.state) == 1
    }

    /// Acquire a read transaction that contains every message yet to be dequeued.
    /// Returns `None` when the queue is empty _and_ the corresponding [Writer] has been dropped.
    pub fn read(&mut self) -> Option<RecvTxn<'_, T>> {
        // Load the data.
        let cap = self.cap;
        let head = self.state.head.load(Ordering::Acquire);
        let tail = self.state.tail.load(Ordering::Acquire);

        // Compute the read region.
        let used = head.wrapping_sub(tail);
        let start = tail & (cap - 1);
        let length = if used < cap - start {
            used
        } else {
            cap - start
        };

        // If the length is zero, check if the writer dropped.
        if length == 0 && self.sender_dropped() {
            return None;
        }

        // Return the read guard.
        Some(RecvTxn {
            reader: self,
            start,
            length,
        })
    }

    /// Create a new sender if the previous sender was dropped.
    pub fn sender(&mut self) -> Option<Sender<T>> {
        if !self.sender_dropped() {
            return None;
        }
        Some(Sender {
            cap: self.cap,
            state: self.state.clone(),
        })
    }
}

impl<T> Sender<T>
where
    T: Clone,
{
    pub fn send_all(&mut self, mut message: &[T]) -> usize {
        let mut count = 0;
        while !message.is_empty() {
            let Some(mut txn) = self.write(count) else {
                break;
            };
            let len = message.len().min(txn.len());
            txn[0..len].clone_from_slice(&message[0..len]);
            message = &message[len..];
            count += len;
        }
        count
    }
}

impl<T> Sender<T> {
    /// Acquire a write transaction. Returns None if the corresponding [Reader] was dropped.
    /// Note: there is no guarantee that this returns `None` if the reader is dropped concurrently
    /// with acquiring the transaction or before it is committed.
    pub fn write(&mut self, count: usize) -> Option<SendTxn<'_, T>> {
        if self.receiver_dropped() {
            return None;
        }

        // Load the data.
        let cap = self.cap;
        let head = self.state.head.load(Ordering::Acquire);
        let tail = self.state.tail.load(Ordering::Acquire);

        // Compute the write region.
        let used = head.wrapping_sub(tail);
        let free = cap - used;
        let start = head & (cap - 1);
        let length = if free < cap - start {
            free
        } else {
            cap - start
        };

        // Return the guard.
        Some(SendTxn {
            writer: self,
            start,
            length: length.min(count),
        })
    }

    /// Create a new reader, if the old reader was dropped.
    pub fn receiver(&mut self) -> Option<Receiver<T>> {
        if !self.receiver_dropped() {
            return None;
        }
        Some(Receiver {
            cap: self.cap,
            state: self.state.clone(),
        })
    }

    fn receiver_dropped(&self) -> bool {
        Arc::strong_count(&self.state) == 1
    }
}

impl<T> RecvTxn<'_, T> {
    pub fn commit(self) {
        self.reader
            .state
            .tail
            .fetch_add(self.length, Ordering::AcqRel);
    }
    pub fn commit_n(self, size: usize) {
        debug_assert!(size <= self.length);
        self.reader.state.tail.fetch_add(size, Ordering::AcqRel);
    }
}

impl<T> SendTxn<'_, T> {
    /// Commit the write transaction. This _must_ be called or messages will not appear in the
    /// queue.
    pub fn commit(self) {
        self.writer
            .state
            .head
            .fetch_add(self.length, Ordering::AcqRel);
    }
}

impl<T> Deref for RecvTxn<'_, T> {
    type Target = [T];
    fn deref(&self) -> &Self::Target {
        unsafe {
            let data = self.reader.state.data.add(self.start);
            std::slice::from_raw_parts(data, self.length)
        }
    }
}

impl<T> Deref for SendTxn<'_, T> {
    type Target = [T];
    fn deref(&self) -> &Self::Target {
        unsafe {
            let data = self.writer.state.data.add(self.start);
            std::slice::from_raw_parts(data, self.length)
        }
    }
}

impl<T> DerefMut for SendTxn<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe {
            let data = self.writer.state.data.add(self.start);
            std::slice::from_raw_parts_mut(data, self.length)
        }
    }
}

impl std::io::Write for Sender<u8> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        loop {
            let Some(mut txn) = self.write(buf.len()) else {
                return Ok(0);
            };
            if txn.is_empty() {
                std::hint::spin_loop();
                continue;
            }
            let len = txn.len().min(buf.len());
            txn[0..len].copy_from_slice(&buf[0..len]);
            return Ok(len);
        }
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl std::io::Read for Receiver<u8> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let Some(txn) = self.read() else {
            return Ok(0);
        };
        let len = buf.len().min(txn.len());
        buf[0..len].copy_from_slice(&txn[0..len]);
        txn.commit_n(len);
        Ok(len)
    }
}

#[cfg(test)]
mod tests {
    use crate::channel;

    #[test]
    fn blocked_reader() {
        let cap = 128;
        let (mut writer, _reader) = channel(cap, None, || 0u64);

        let guard = writer.write(100).unwrap();
        assert_eq!(guard.len(), 100);
        guard.commit();

        let guard = writer.write(100).unwrap();
        assert_eq!(guard.len(), 28);
        guard.commit();

        let guard = writer.write(100).unwrap();
        assert_eq!(guard.len(), 0);
        guard.commit();
    }

    #[test]
    fn read_write() {
        let cap = 128;
        let (mut writer, mut reader) = channel(cap, None, || 0u64);

        let guard = reader.read().unwrap();
        assert_eq!(guard.len(), 0);

        let guard = writer.write(100).unwrap();
        assert_eq!(guard.len(), 100);
        guard.commit();

        let guard = reader.read().unwrap();
        assert_eq!(guard.len(), 100);
        guard.commit();

        let guard = writer.write(100).unwrap();
        assert_eq!(guard.len(), 28);
        guard.commit();
    }

    #[test]
    fn slow_reader() {
        let total = 500;
        let cap = 128;
        let (mut writer, mut reader) = channel(cap, None, || 0u64);
        let thread = std::thread::spawn(move || {
            let mut received = vec![];
            while let Some(buf) = reader.read() {
                received.extend_from_slice(&buf);
                buf.commit();
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            assert_eq!(received.len(), total);
        });
        for mut chunk in vec![0; total].chunks(100) {
            while chunk.len() > 0 {
                let mut buf = writer.write(chunk.len()).unwrap();
                let len = buf.len().min(chunk.len());
                buf[0..len].copy_from_slice(&chunk[0..len]);
                buf.commit();
                chunk = &chunk[len..];
            }
        }
        drop(writer);
        thread.join().unwrap();
    }
}
