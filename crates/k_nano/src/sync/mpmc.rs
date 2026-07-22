use core::mem::MaybeUninit;
use core::sync::atomic::{AtomicUsize, Ordering};

const EMPTY: usize = 0;
const STORING: usize = 1;
const READY: usize = 2;
const LOADING: usize = 3;

struct Slot<T> {
    state: AtomicUsize,
    data: MaybeUninit<T>,
}

impl<T> Slot<T> {
    const fn new() -> Self {
        Slot { state: AtomicUsize::new(EMPTY), data: MaybeUninit::uninit() }
    }
}

pub struct MpmcQueue<T> {
    buffer: *mut Slot<T>,
    capacity: usize,
    mask: usize,
    enqueue_pos: AtomicUsize,
    dequeue_pos: AtomicUsize,
}

unsafe impl<T: Send> Send for MpmcQueue<T> {}
unsafe impl<T: Send> Sync for MpmcQueue<T> {}

impl<T> MpmcQueue<T> {
    pub fn new(capacity: usize) -> Option<Self> {
        if !capacity.is_power_of_two() || capacity < 2 {
            return None;
        }
        let layout = alloc::alloc::Layout::array::<Slot<T>>(capacity).ok()?;
        let ptr = unsafe { alloc::alloc::alloc(layout) as *mut Slot<T> };
        if ptr.is_null() {
            return None;
        }
        for i in 0..capacity {
            unsafe { core::ptr::write(ptr.add(i), Slot::new()); }
        }
        Some(MpmcQueue {
            buffer: ptr,
            capacity,
            mask: capacity - 1,
            enqueue_pos: AtomicUsize::new(0),
            dequeue_pos: AtomicUsize::new(0),
        })
    }

    pub fn try_send(&self, item: T) -> Result<(), T> {
        loop {
            let pos = self.enqueue_pos.load(Ordering::Relaxed);
            if pos - self.dequeue_pos.load(Ordering::Acquire) >= self.capacity {
                return Err(item);
            }
            if self.enqueue_pos
                .compare_exchange_weak(pos, pos + 1, Ordering::Relaxed, Ordering::Relaxed)
                .is_err() { continue; }
            let slot = self.buffer.wrapping_add(pos & self.mask);
            unsafe {
                while (*slot).state.compare_exchange(EMPTY, STORING, Ordering::Acquire, Ordering::Relaxed).is_err() {}
                (*slot).data.as_mut_ptr().write(item);
                (*slot).state.store(READY, Ordering::Release);
            }
            return Ok(());
        }
    }

    pub fn try_recv(&self) -> Option<T> {
        loop {
            let pos = self.dequeue_pos.load(Ordering::Relaxed);
            if pos == self.enqueue_pos.load(Ordering::Acquire) {
                return None;
            }
            if self.dequeue_pos
                .compare_exchange_weak(pos, pos + 1, Ordering::Relaxed, Ordering::Relaxed)
                .is_err() { continue; }
            let slot = self.buffer.wrapping_add(pos & self.mask);
            unsafe {
                while (*slot).state.compare_exchange(READY, LOADING, Ordering::Acquire, Ordering::Relaxed).is_err() {}
                let item = (*slot).data.as_ptr().read();
                (*slot).state.store(EMPTY, Ordering::Release);
                return Some(item);
            }
        }
    }

    pub fn len(&self) -> usize {
        self.enqueue_pos.load(Ordering::Relaxed) - self.dequeue_pos.load(Ordering::Relaxed)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

impl<T> Drop for MpmcQueue<T> {
    fn drop(&mut self) {
        unsafe {
            let enq = self.enqueue_pos.load(Ordering::Relaxed);
            let deq = self.dequeue_pos.load(Ordering::Relaxed);
            for i in deq..enq {
                let slot = self.buffer.wrapping_add(i & self.mask);
                if (*slot).state.load(Ordering::Relaxed) != EMPTY {
                    core::ptr::drop_in_place((*slot).data.as_mut_ptr());
                }
            }
            if let Some(l) = alloc::alloc::Layout::array::<Slot<T>>(self.capacity).ok() {
                alloc::alloc::dealloc(self.buffer as *mut u8, l);
            }
        }
    }
}
