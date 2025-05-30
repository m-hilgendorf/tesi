use std::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
};

/// The read-end of a ring buffer.
pub struct Reader<T> {
    cap: usize,
    fifo: Arc<BatchBuffer<T>>,
}

/// The write end of a ring buffer.
pub struct Writer<T> {
    cap: usize,
    fifo: Arc<BatchBuffer<T>>,
}

/// A single-producer, single-consumer (SPSC) queue with batch read/write operations.
struct BatchBuffer<T> {
    head: Padded<AtomicUsize>,
    tail: Padded<AtomicUsize>,
    reader_dropped: Padded<AtomicBool>,
    writer_dropped: Padded<AtomicBool>,
    data: UnsafeCell<Box<[T]>>,
    cap: usize,
}

/// A read transcation. See [Reader::read].
pub struct ReadTxn<'a, T> {
    reader: &'a mut Reader<T>,
    start: usize,
    length: usize,
}

/// A write transcation. See [Writer::write].
pub struct WriteTxn<'a, T> {
    writer: &'a mut Writer<T>,
    start: usize,
    length: usize,
}

unsafe impl<T> Send for BatchBuffer<T> {}
unsafe impl<T> Sync for BatchBuffer<T> {}

/// Create a new ring buffer with a fixed capacity and initial value within the buffer.
pub fn batchbuffer<T>(cap: usize, init: impl Fn() -> T) -> (Writer<T>, Reader<T>) {
    BatchBuffer::new(cap, init).split()
}

impl<T> BatchBuffer<T> {
    pub fn new(cap: usize, init: impl Fn() -> T) -> Self {
        debug_assert!(
            cap.is_power_of_two(),
            "fifo capacity must be a power of two"
        );
        let mut data = Vec::with_capacity(cap);
        data.resize_with(cap, init);
        let data = UnsafeCell::new(data.into_boxed_slice());
        let head = Padded(AtomicUsize::new(0));
        let tail = Padded(AtomicUsize::new(0));
        let reader_dropped = Padded(AtomicBool::new(false));
        let writer_dropped = Padded(AtomicBool::new(false));
        Self {
            head,
            tail,
            reader_dropped,
            writer_dropped,
            data,
            cap,
        }
    }

    pub fn split(self) -> (Writer<T>, Reader<T>) {
        let fifo = Arc::new(self);
        let cap = fifo.cap;
        let writer = Writer {
            cap,
            fifo: fifo.clone(),
        };
        let reader = Reader {
            cap,
            fifo: fifo.clone(),
        };
        (writer, reader)
    }
}

impl<T> Reader<T> {
    /// Acquire a read transaction that contains every message yet to be dequeued.
    /// Returns `None` when the queue is empty _and_ the corresponding [Writer] has been dropped.
    pub fn read(&mut self) -> Option<ReadTxn<'_, T>> {
        // Load the data.
        let cap = self.cap;
        let head = self.fifo.head.load(Ordering::Acquire);
        let tail = self.fifo.tail.load(Ordering::Acquire);

        // Compute the read region.
        let used = head.wrapping_sub(tail);
        let start = tail & (cap - 1);
        let length = if used < cap - start {
            used
        } else {
            cap - start
        };

        // If the length is zero, check if the writer dropped.
        if length == 0 && self.fifo.writer_dropped.load(Ordering::Relaxed) {
            return None;
        }

        // Return the read guard.
        Some(ReadTxn {
            reader: self,
            start,
            length,
        })
    }

    /// Create a new writer if the previous writer was dropped.
    pub fn writer(&mut self) -> Option<Writer<T>> {
        if !self.fifo.writer_dropped.load(Ordering::Acquire) {
            return None;
        }
        self.fifo.writer_dropped.store(false, Ordering::Release);
        Some(Writer {
            cap: self.cap,
            fifo: self.fifo.clone(),
        })
    }
}

impl<T> Writer<T> {
    /// Acquire a write transaction. Returns None if the corresponding [Reader] was dropped.
    /// Note: there is no guarantee that this returns `None` if the reader is dropped concurrently
    /// with acquiring the transaction or before it is committed.
    pub fn write(&mut self, count: usize) -> Option<WriteTxn<'_, T>> {
        if self.fifo.reader_dropped.load(Ordering::Relaxed) {
            return None;
        }

        // Load the data.
        let cap = self.cap;
        let head = self.fifo.head.load(Ordering::Acquire);
        let tail = self.fifo.tail.load(Ordering::Acquire);

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
        Some(WriteTxn {
            writer: self,
            start,
            length: length.min(count),
        })
    }

    /// Create a new reader, if the old reader was dropped.
    pub fn reader(&mut self) -> Option<Reader<T>> {
        if !self.fifo.reader_dropped.load(Ordering::Relaxed) {
            return None;
        }
        self.fifo.reader_dropped.store(false, Ordering::Relaxed);
        Some(Reader {
            cap: self.cap,
            fifo: self.fifo.clone(),
        })
    }
}

impl<T> Drop for Reader<T> {
    fn drop(&mut self) {
        self.fifo.reader_dropped.store(true, Ordering::Release);
    }
}

impl<T> Drop for Writer<T> {
    fn drop(&mut self) {
        self.fifo.writer_dropped.store(true, Ordering::Release);
    }
}

impl<T> ReadTxn<'_, T> {
    pub fn commit(self) {
        self.reader
            .fifo
            .tail
            .fetch_add(self.length, Ordering::AcqRel);
    }
}

impl<T> WriteTxn<'_, T> {
    /// Commit the write transaction. This _must_ be called or messages will not appear in the
    /// queue.
    pub fn commit(self) {
        self.writer
            .fifo
            .head
            .fetch_add(self.length, Ordering::AcqRel);
    }
}

impl<T> Deref for ReadTxn<'_, T> {
    type Target = [T];
    fn deref(&self) -> &Self::Target {
        unsafe {
            let data = (*self.reader.fifo.data.get()).as_ptr().add(self.start);
            std::slice::from_raw_parts(data, self.length)
        }
    }
}

impl<T> Deref for WriteTxn<'_, T> {
    type Target = [T];
    fn deref(&self) -> &Self::Target {
        unsafe {
            let data = (*self.writer.fifo.data.get()).as_ptr().add(self.start);
            std::slice::from_raw_parts(data, self.length)
        }
    }
}

impl<T> DerefMut for WriteTxn<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe {
            let data = (*self.writer.fifo.data.get()).as_mut_ptr().add(self.start);
            std::slice::from_raw_parts_mut(data, self.length)
        }
    }
}

#[cfg_attr(
    any(target_arch = "x86_64", target_arch = "aarch64",),
    repr(align(128))
)]
#[cfg_attr(
    not(any(target_arch = "x86_64", target_arch = "aarch64",)),
    repr(align(64))
)]
struct Padded<T>(T);

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

#[cfg(test)]
mod tests {
    use std::{
        cell::UnsafeCell,
        sync::atomic::{AtomicBool, AtomicUsize},
    };

    use crate::{BatchBuffer, Padded, batchbuffer};

    #[test]
    fn wraparound() {
        let fifo = BatchBuffer {
            reader_dropped: Padded(AtomicBool::new(false)),
            writer_dropped: Padded(AtomicBool::new(false)),
            head: Padded(AtomicUsize::new(15)),
            tail: Padded(AtomicUsize::new(usize::MAX - 16)),
            data: UnsafeCell::new(vec![0u32; 64].into_boxed_slice()),
            cap: 64,
        };
        let (mut writer, mut reader) = fifo.split();
        let readbuf = reader.read().unwrap();
        let writebuf = writer.write(64).unwrap();
        assert_eq!(readbuf.len(), 17);
        assert_eq!(writebuf.len(), 32);
        assert_eq!(writebuf.start + writebuf.length, readbuf.start);
    }

    #[test]
    fn blocked_reader() {
        let cap = 128;
        let (mut writer, _reader) = batchbuffer(cap, || 0u64);

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
        let (mut writer, mut reader) = batchbuffer(cap, || 0u64);

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
        let (mut writer, mut reader) = batchbuffer(cap, || 0u64);
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
