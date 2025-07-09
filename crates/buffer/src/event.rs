use std::ptr::null_mut;
use util::collections::Stack;

pub struct Arena {
    events: *mut u8,                     /* event buffer  */
    entries: *mut Entry,                 /* entry buffer */
    stack: Stack<(*mut u8, *mut Entry)>, /* stack of free items */
    size: u32,                           /* size of each event */
    align: u32,                          /* alignment of each event */
    max_num_ports: u32, /* the max number of event ports that need to be supported concurrently */
    max_num_events: u32, /* the max number of events in one buffer */
}

pub struct Event {
    pub size: u32,           /* the size of each event */
    pub align: u32,          /* the alignment of each event */
    pub length: u32,         /* the number of events in this buffer */
    pub capacity: u32,       /* the total capacity of this buffer */
    pub entries: *mut Entry, /* the event offsets */
    pub events: *mut u8,     /* the raw event data */
}

#[derive(Copy, Clone, Debug, Default)]
pub struct Entry {
    pub offset: u32, /* offset from the start of the buffer, in bytes */
    pub length: u32, /* length of the event, in bytes */
    pub time: u32,   /* offset from the start of the buffer, in frames */
}

impl Arena {
    pub fn new(max_num_ports: u32, max_num_events: u32, event_align: u32, event_size: u32) -> Self {
        let aligned_event_size = if event_size % event_align == 0 {
            event_size
        } else {
            event_size + (event_align - event_size % event_align)
        };
        let capacity = max_num_events * max_num_ports;
        let events = unsafe {
            let size = capacity * aligned_event_size;
            let layout = std::alloc::Layout::from_size_align(size as _, event_align as _)
                .expect("invalid event size and alignment");
            std::alloc::alloc_zeroed(layout)
        };
        let entries: *mut Entry = unsafe {
            let size = (capacity as usize) * std::mem::size_of::<Entry>();
            let align = std::mem::align_of::<Entry>();
            let layout = std::alloc::Layout::from_size_align_unchecked(size, align);
            std::alloc::alloc_zeroed(layout).cast()
        };
        let mut stack = Stack::new(max_num_ports.try_into().unwrap());
        for idx in 0..max_num_ports {
            unsafe {
                let event = events.add((idx * max_num_events * aligned_event_size) as _);
                let entry = entries.add((idx * max_num_events) as _);
                stack.push((event, entry));
            }
        }
        Self {
            events,
            entries,
            size: event_size,
            align: event_align,
            stack,
            max_num_events,
            max_num_ports,
        }
    }

    pub fn acquire(&mut self, buffer: &mut Event) -> bool {
        let Some((event, entry)) = self.alloc() else {
            return false;
        };
        buffer.events = event;
        buffer.entries = entry;
        buffer.length = 0;
        buffer.capacity = self.max_num_events;
        buffer.align = self.align;
        buffer.size = self.size;
        buffer.size = self.size;
        true
    }

    pub fn release(&mut self, buffer: &mut Event) {
        self.dealloc((buffer.events, buffer.entries));
        buffer.events = null_mut();
        buffer.entries = null_mut();
        buffer.capacity = 0;
        buffer.length = 0;
    }

    fn alloc(&mut self) -> Option<(*mut u8, *mut Entry)> {
        self.stack.pop()
    }

    fn dealloc(&mut self, ptr: (*mut u8, *mut Entry)) {
        self.stack.push(ptr);
    }

    pub unsafe fn reset(&mut self) {
        self.stack.clear();
        let aligned_event_size = if self.size % self.align == 0 {
            self.size
        } else {
            self.size + (self.align - self.size % self.align)
        };
        for idx in 0..self.max_num_ports {
            unsafe {
                let event = self.events.add(
                    (idx * self.max_num_events * aligned_event_size)
                        .try_into()
                        .unwrap(),
                );
                let entry = self
                    .entries
                    .add((idx * self.max_num_events).try_into().unwrap());
                self.stack.push((event, entry));
            }
        }
    }
}

impl Drop for Arena {
    fn drop(&mut self) {
        unsafe {
            let aligned_event_size = if self.size % self.align == 0 {
                self.size
            } else {
                self.size + (self.align - self.size % self.align)
            };
            let capacity = self.max_num_ports * self.max_num_events;
            let size = capacity * aligned_event_size;
            let layout = std::alloc::Layout::from_size_align(size as _, self.align as _)
                .expect("invalid event size and alignment");
            std::alloc::dealloc(self.events.cast(), layout);

            let size = (capacity as usize) * std::mem::size_of::<Entry>();
            let align = std::mem::align_of::<Entry>();
            let layout = std::alloc::Layout::from_size_align_unchecked(size, align);
            std::alloc::dealloc(self.entries.cast(), layout);
        }
    }
}

impl Event {
    pub fn empty() -> Self {
        Self {
            size: 0,
            align: 0,
            length: 0,
            capacity: 0,
            entries: null_mut(),
            events: null_mut(),
        }
    }

    pub fn assign_to(&mut self, other: &Self) {
        self.size = other.size;
        self.align = other.align;
        self.capacity = other.capacity;
        self.entries = other.entries;
        self.events = other.events;
        self.length = other.length;
    }

    fn entries(&self) -> &[Entry] {
        unsafe { std::slice::from_raw_parts(self.entries, self.length as _) }
    }

    pub fn len(&self) -> usize {
        self.entries()
            .last()
            .map_or(0, |entry| (entry.offset + entry.length) as _)
    }

    pub fn iter_chunks<E: Copy + 'static>(&self) -> impl Iterator<Item = (usize, &[E])> + '_ {
        self.entries().iter().map(|entry| {
            let start = entry.offset * self.size;
            let slice = unsafe {
                let ptr = self.events.add(start as _);
                let length = entry.length;
                std::slice::from_raw_parts(ptr.cast(), length as _)
            };
            (entry.time as _, slice)
        })
    }

    pub fn iter<E: Copy + 'static>(&self) -> impl Iterator<Item = (usize, E)> + '_ {
        self.iter_chunks()
            .flat_map(|(time, block)| block.iter().copied().map(move |e| (time, e)))
    }

    pub fn insert<E: Copy + 'static>(&mut self, time: u32, events: &[E]) -> u32 {
        debug_assert_eq!(std::mem::align_of::<E>(), self.align as _);
        debug_assert_eq!(std::mem::size_of::<E>(), self.size as _);
        debug_assert!((self.length as usize) + events.len() <= self.capacity as _);

        // Compute offset and length.
        let (index, offset) = self
            .entries()
            .iter()
            .enumerate()
            .rev()
            .find(|(_, entry)| entry.time < time as _)
            .map_or((0, 0), |(index, entry)| {
                (index, entry.offset + entry.length)
            });
        let length = (events.len() as u32).min(self.capacity - offset);
        unsafe {
            // Move existing events.
            let src = self.events.add((offset * self.size) as _);
            let dst = src.add((length * self.size) as _);
            std::ptr::copy(src, dst, ((length - self.length) * self.size) as _);

            // Insert new events.
            let src = events.as_ptr();
            let dst = self.events.add((offset * self.size) as _);
            std::ptr::copy_nonoverlapping(src.cast(), dst, (length * self.size) as _);

            // Insert the entry.
            let src = self.entries.add(index);
            let dst = src.add(1);
            std::ptr::copy(src, dst, (self.length - offset) as _);

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
