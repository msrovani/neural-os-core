use core::sync::atomic::{AtomicUsize, Ordering};
use spin::Mutex;

pub const BUCKET_SIZES: [usize; 8] = [32, 64, 128, 256, 512, 1024, 2048, 4096];
const ZONE_SIZE: usize = 65536;
const NUM_BUCKETS: usize = 8;

static SLAB_METRICS: Metrics = Metrics::new();

struct Metrics {
    alloc_count: AtomicUsize,
    dealloc_count: AtomicUsize,
}

impl Metrics {
    const fn new() -> Self {
        Metrics {
            alloc_count: AtomicUsize::new(0),
            dealloc_count: AtomicUsize::new(0),
        }
    }
}

struct SlabBucket {
    block_size: usize,
    zone_start: usize,
    zone_end: usize,
    free_head: *mut u8,
}

impl SlabBucket {
    const fn empty() -> Self {
        SlabBucket {
            block_size: 0,
            zone_start: 0,
            zone_end: 0,
            free_head: core::ptr::null_mut(),
        }
    }

    unsafe fn init(&mut self, block_size: usize, zone_start: usize, zone_end: usize) {
        self.block_size = block_size;
        self.zone_start = zone_start;
        self.zone_end = zone_end;
        self.free_head = core::ptr::null_mut();

        let mut addr = zone_start;
        let mut prev: *mut u8 = core::ptr::null_mut();
        while addr + block_size < zone_end {
            let curr = addr as *mut u8;
            if prev.is_null() {
                self.free_head = curr;
            } else {
                *(prev as *mut *mut u8) = curr;
            }
            prev = curr;
            addr += block_size;
        }
        if !prev.is_null() {
            *(prev as *mut *mut u8) = core::ptr::null_mut();
        }
    }

    unsafe fn alloc(&mut self) -> *mut u8 {
        if self.free_head.is_null() {
            return core::ptr::null_mut();
        }
        let block = self.free_head;
        self.free_head = *(block as *mut *mut u8);
        block
    }

    unsafe fn dealloc(&mut self, ptr: *mut u8) {
        *(ptr as *mut *mut u8) = self.free_head;
        self.free_head = ptr;
    }

    fn contains(&self, ptr: *mut u8) -> bool {
        let addr = ptr as usize;
        addr >= self.zone_start && addr < self.zone_end
    }
}

pub struct SlabAllocator {
    buckets: [SlabBucket; NUM_BUCKETS],
}

impl SlabAllocator {
    pub const fn empty() -> Self {
        const EMPTY_BUCKET: SlabBucket = SlabBucket::empty();
        SlabAllocator {
            buckets: [
                EMPTY_BUCKET, EMPTY_BUCKET, EMPTY_BUCKET, EMPTY_BUCKET,
                EMPTY_BUCKET, EMPTY_BUCKET, EMPTY_BUCKET, EMPTY_BUCKET,
            ],
        }
    }

    pub unsafe fn init(&mut self, heap_start: usize) {
        let mut zone_offset = 0usize;
        for i in 0..NUM_BUCKETS {
            let block_size = BUCKET_SIZES[i];
            let zone_start = heap_start + zone_offset;
            let zone_end = zone_start + ZONE_SIZE;
            self.buckets[i].init(block_size, zone_start, zone_end);
            zone_offset += ZONE_SIZE;
        }
    }

    fn bucket_for_size(&self, size: usize) -> Option<usize> {
        for (i, &bs) in BUCKET_SIZES.iter().enumerate() {
            if size <= bs {
                return Some(i);
            }
        }
        None
    }

    pub unsafe fn slab_alloc(&mut self, size: usize) -> *mut u8 {
        let idx = match self.bucket_for_size(size) {
            Some(i) => i,
            None => return core::ptr::null_mut(),
        };
        let ptr = self.buckets[idx].alloc();
        if !ptr.is_null() {
            SLAB_METRICS.alloc_count.fetch_add(1, Ordering::Relaxed);
        }
        ptr
    }

    pub unsafe fn slab_dealloc(&mut self, ptr: *mut u8) -> bool {
        for i in 0..NUM_BUCKETS {
            if self.buckets[i].contains(ptr) {
                self.buckets[i].dealloc(ptr);
                SLAB_METRICS.dealloc_count.fetch_add(1, Ordering::Relaxed);
                return true;
            }
        }
        false
    }

    pub fn metrics(&self) -> (usize, usize) {
        (
            SLAB_METRICS.alloc_count.load(Ordering::Relaxed),
            SLAB_METRICS.dealloc_count.load(Ordering::Relaxed),
        )
    }
}

unsafe impl Send for SlabAllocator {}

pub static SLAB_ALLOCATOR: Mutex<SlabAllocator> = Mutex::new(SlabAllocator::empty());
