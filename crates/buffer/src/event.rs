pub struct Event {
    size: usize,
    align: usize,
    length: usize,
    capacity: usize,
    entries: *mut Entry,
    events: *mut u8,
}

#[derive(Copy, Clone, Debug, Default)]
struct Entry {
    offset: usize,
    length: usize,
    time: usize,
}

impl Event {
    pub fn new(size: usize, align: usize, capacity: usize) -> Self {
        let (entries, events) = unsafe {
            let layout = std::alloc::Layout::from_size_align(
                capacity * std::mem::size_of::<Entry>(),
                std::mem::align_of::<Entry>(),
            )
            .unwrap();
            let entries = std::alloc::alloc_zeroed(layout).cast();

            let layout = std::alloc::Layout::from_size_align(size * capacity, align).unwrap();
            let events = std::alloc::alloc_zeroed(layout).cast();
            (entries, events)
        };
        Self {
            size,
            align,
            length: 0,
            capacity,
            entries,
            events,
        }
    }

    fn entries(&self) -> &[Entry] {
        unsafe { std::slice::from_raw_parts(self.entries, self.length) }
    }

    pub fn len(&self) -> usize {
        self.entries()
            .last()
            .map_or(0, |entry| entry.offset + entry.length)
    }

    pub fn iter_chunks<E: Copy + 'static>(&self) -> impl Iterator<Item = (usize, &[E])> + '_ {
        self.entries().iter().map(|entry| {
            let start = entry.offset * self.size;
            let slice = unsafe {
                let ptr = self.events.add(start);
                let length = entry.length;
                std::slice::from_raw_parts(ptr.cast(), length)
            };
            (entry.time, slice)
        })
    }

    pub fn iter<E: Copy + 'static>(&self) -> impl Iterator<Item = (usize, E)> + '_ {
        self.iter_chunks()
            .flat_map(|(time, block)| block.iter().copied().map(move |e| (time, e)))
    }

    pub fn insert<E: Copy + 'static>(&mut self, time: usize, events: &[E]) -> usize {
        debug_assert_eq!(std::mem::align_of::<E>(), self.align);
        debug_assert_eq!(std::mem::size_of::<E>(), self.size);
        debug_assert!(self.length + events.len() <= self.capacity);

        // Compute offset and length.
        let (index, offset) = self
            .entries()
            .iter()
            .enumerate()
            .rev()
            .find(|(_, entry)| entry.time < time)
            .map_or((0, 0), |(index, entry)| {
                (index, entry.offset + entry.length)
            });
        let length = events.len().min(self.capacity - offset);
        unsafe {
            // Move existing events.
            let src = self.events.add(offset * self.size);
            let dst = src.add(length * self.size);
            std::ptr::copy(src, dst, (length - self.length) * self.size);

            // Insert new events.
            let src = events.as_ptr();
            let dst = self.events.add(offset * self.size);
            std::ptr::copy_nonoverlapping(src.cast(), dst, length * self.size);

            // Insert the entry.
            let src = self.entries.add(index);
            let dst = src.add(1);
            std::ptr::copy(src, dst, self.length - offset);

            *src = Entry {
                offset,
                length,
                time,
            };
        }

        length
    }
}

unsafe impl Send for Event {}
