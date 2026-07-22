//! Async Runtime for Bare-Metal x86_64
//! 
//! Lightweight async executor based on Local APIC Timer interrupts.
//! Uses lock-free ring buffer for waker queue to avoid idle CPU consumption.

use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::ptr::NonNull;
use core::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// Lock-free single-producer single-consumer ring buffer
/// 
/// Used for waker queue in the async executor.
pub struct SpscChannel<T> {
    /// Ring buffer storage
    buffer: UnsafeCell<[MaybeUninit<T>; 256]>,
    /// Head index (consumer)
    head: AtomicUsize,
    /// Tail index (producer)
    tail: AtomicUsize,
    /// Mask for modulo operation (buffer size - 1)
    mask: usize,
}

impl<T> SpscChannel<T> {
    /// Create a new SPSC channel with 256 slots
    #[must_use]
    pub const fn new() -> Self {
        Self {
            buffer: UnsafeCell::new([MaybeUninit::uninit(); 256]),
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            mask: 255, // 256 - 1
        }
    }

    /// Try to push a value (producer only)
    /// 
    /// Returns true if successful, false if buffer is full
    pub fn try_push(&self, value: T) -> bool {
        let tail = self.tail.load(Ordering::Acquire);
        let head = self.head.load(Ordering::Acquire);

        // Check if buffer is full
        if tail.wrapping_sub(head) > self.mask {
            return false;
        }

        unsafe {
            let buffer = &mut *self.buffer.get();
            let idx = tail & self.mask;
            buffer[idx].write(value);
        }

        self.tail.store(tail.wrapping_add(1), Ordering::Release);
        true
    }

    /// Try to pop a value (consumer only)
    /// 
    /// Returns Some(value) if available, None if empty
    pub fn try_pop(&self) -> Option<T> {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);

        // Check if buffer is empty
        if head == tail {
            return None;
        }

        unsafe {
            let buffer = &*self.buffer.get();
            let idx = head & self.mask;
            let value = buffer[idx].read();
            self.head.store(head.wrapping_add(1), Ordering::Release);
            Some(value)
        }
    }

    /// Check if the channel is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        let head = self.head.load(Ordering::Acquire);
        let tail = self.tail.load(Ordering::Acquire);
        head == tail
    }

    /// Check if the channel is full
    #[must_use]
    pub fn is_full(&self) -> bool {
        let tail = self.tail.load(Ordering::Acquire);
        let head = self.head.load(Ordering::Acquire);
        tail.wrapping_sub(head) > self.mask
    }
}

impl<T> Default for SpscChannel<T> {
    fn default() -> Self {
        Self::new()
    }
}

// SAFETY: SPSC channel is safe to share between threads when used correctly
// (single producer, single consumer)
unsafe impl<T: Send> Send for SpscChannel<T> {}
unsafe impl<T: Send> Sync for SpscChannel<T> {}

/// Waker handle for waking futures
/// 
/// Simple waker that stores a pointer to the waker queue.
pub struct Waker {
    /// Index in the waker queue
    index: usize,
    /// Flag indicating if the waker has been woken
    woken: AtomicBool,
}

impl Waker {
    /// Create a new waker
    #[must_use]
    pub const fn new(index: usize) -> Self {
        Self {
            index,
            woken: AtomicBool::new(false),
        }
    }

    /// Wake the future
    pub fn wake(&self) {
        self.woken.store(true, Ordering::Release);
    }

    /// Check if the waker has been woken
    #[must_use]
    pub fn is_woken(&self) -> bool {
        self.woken.load(Ordering::Acquire)
    }

    /// Reset the woken state
    pub fn reset(&self) {
        self.woken.store(false, Ordering::Release);
    }

    /// Get the waker index
    #[must_use]
    pub const fn index(&self) -> usize {
        self.index
    }
}

/// Waker queue for tracking active futures
pub struct WakerQueue {
    /// Array of wakers
    wakers: [UnsafeCell<Option<Waker>>; 64],
    /// Count of active wakers
    count: AtomicUsize,
}

impl WakerQueue {
    /// Create a new waker queue
    #[must_use]
    pub const fn new() -> Self {
        const INIT: UnsafeCell<Option<Waker>> = UnsafeCell::new(None);
        Self {
            wakers: [INIT; 64],
            count: AtomicUsize::new(0),
        }
    }

    /// Register a new waker
    /// 
    /// Returns the index of the registered waker, or None if full
    pub fn register(&self) -> Option<usize> {
        let count = self.count.load(Ordering::Acquire);
        if count >= 64 {
            return None;
        }

        for i in 0..64 {
            unsafe {
                let waker = &*self.wakers[i].get();
                if waker.is_none() {
                    *(&mut *self.wakers[i].get()) = Some(Waker::new(i));
                    self.count.fetch_add(1, Ordering::Release);
                    return Some(i);
                }
            }
        }

        None
    }

    /// Wake a specific waker by index
    pub fn wake_by_index(&self, index: usize) {
        if index < 64 {
            unsafe {
                let waker = &*self.wakers[index].get();
                if let Some(w) = waker {
                    w.wake();
                }
            }
        }
    }

    /// Wake all registered wakers
    pub fn wake_all(&self) {
        for i in 0..64 {
            self.wake_by_index(i);
        }
    }

    /// Get a waker by index
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&Waker> {
        if index < 64 {
            unsafe {
                let waker = &*self.wakers[index].get();
                waker.as_ref()
            }
        } else {
            None
        }
    }

    /// Unregister a waker
    pub fn unregister(&self, index: usize) {
        if index < 64 {
            unsafe {
                *(&mut *self.wakers[index].get()) = None;
                self.count.fetch_sub(1, Ordering::Release);
            }
        }
    }

    /// Get the count of active wakers
    #[must_use]
    pub fn count(&self) -> usize {
        self.count.load(Ordering::Acquire)
    }
}

impl Default for WakerQueue {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple future trait for the async runtime
pub trait Future {
    /// The output type of the future
    type Output;

    /// Poll the future
    /// 
    /// Returns Poll::Pending if not ready, Poll::Ready(output) if complete
    fn poll(&self, waker: &Waker) -> Poll<Self::Output>;
}

/// Poll result for futures
pub enum Poll<T> {
    /// Future is not ready yet
    Pending,
    /// Future is complete with output
    Ready(T),
}

/// Async executor based on APIC timer interrupts
pub struct AsyncExecutor {
    /// Waker queue for tracking futures
    waker_queue: WakerQueue,
    /// Flag indicating if executor is running
    running: AtomicBool,
    /// SPSC channel for wake notifications
    wake_channel: SpscChannel<usize>,
}

impl AsyncExecutor {
    /// Create a new async executor
    #[must_use]
    pub const fn new() -> Self {
        Self {
            waker_queue: WakerQueue::new(),
            running: AtomicBool::new(false),
            wake_channel: SpscChannel::new(),
        }
    }

    /// Start the executor
    /// 
    /// This should be called after APIC timer is configured
    pub fn start(&self) {
        self.running.store(true, Ordering::Release);
    }

    /// Stop the executor
    pub fn stop(&self) {
        self.running.store(false, Ordering::Release);
    }

    /// Check if executor is running
    #[must_use]
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Acquire)
    }

    /// Register a future with the executor
    /// 
    /// Returns the waker index for the future
    pub fn register_future<F>(&self, _future: &F) -> Option<usize>
    where
        F: Future,
    {
        self.waker_queue.register()
    }

    /// Wake a future by index (called from interrupt handler)
    pub fn wake_future(&self, index: usize) {
        self.wake_queue.wake_by_index(index);
        // Also push to wake channel for processing
        let _ = self.wake_channel.try_push(index);
    }

    /// Process wake notifications (called from main loop)
    pub fn process_wakes(&self) {
        while let Some(index) = self.wake_channel.try_pop() {
            // The future at this index should be polled
            // This is a stub - actual implementation would poll the future
            let _ = index;
        }
    }

    /// Get the waker queue
    #[must_use]
    pub const fn waker_queue(&self) -> &WakerQueue {
        &self.waker_queue
    }
}

impl Default for AsyncExecutor {
    fn default() -> Self {
        Self::new()
    }
}

/// Global async executor instance
static GLOBAL_EXECUTOR: AsyncExecutor = AsyncExecutor::new();

/// Get the global async executor
#[must_use]
pub fn global_executor() -> &'static AsyncExecutor {
    &GLOBAL_EXECUTOR
}

/// APIC Timer interrupt handler
/// 
/// This should be registered in the IDT for the timer interrupt vector.
/// It wakes futures and advances the async runtime.
pub extern "x86-interrupt" fn apic_timer_handler(_stack_frame: x86_64::structures::idt::InterruptStackFrame) {
    // Process wake notifications
    global_executor().process_wakes();

    // Send EOI to LAPIC
    unsafe {
        crate::apic::end_of_interrupt();
    }
}

/// Initialize the async runtime with APIC timer
/// 
/// Configures the APIC timer to trigger interrupts at a regular interval,
/// which will wake futures and advance the async runtime.
pub fn init_async_rt() {
    let executor = global_executor();
    executor.start();

    // Configure APIC timer (this would call into the apic module)
    // For now, this is a stub
    // unsafe {
    //     crate::apic::configure_timer(32, 0x800000); // Vector 32, count
    // }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spsc_channel_push_pop() {
        let channel: SpscChannel<u32> = SpscChannel::new();

        assert!(channel.is_empty());
        assert!(!channel.is_full());

        assert!(channel.try_push(42));
        assert!(channel.try_push(100));

        assert!(!channel.is_empty());

        assert_eq!(channel.try_pop(), Some(42));
        assert_eq!(channel.try_pop(), Some(100));
        assert_eq!(channel.try_pop(), None);

        assert!(channel.is_empty());
    }

    #[test]
    fn test_spsc_channel_full() {
        let channel: SpscChannel<u8> = SpscChannel::new();

        // Fill the channel
        for i in 0..256 {
            assert!(channel.try_push(i as u8));
        }

        assert!(channel.is_full());
        assert!(!channel.try_push(255));

        // Drain the channel
        for i in 0..256 {
            assert_eq!(channel.try_pop(), Some(i as u8));
        }

        assert!(channel.is_empty());
    }

    #[test]
    fn test_waker() {
        let waker = Waker::new(0);
        assert!(!waker.is_woken());

        waker.wake();
        assert!(waker.is_woken());

        waker.reset();
        assert!(!waker.is_woken());
    }

    #[test]
    fn test_waker_queue() {
        let queue = WakerQueue::new();
        assert_eq!(queue.count(), 0);

        let idx1 = queue.register();
        assert!(idx1.is_some());
        assert_eq!(queue.count(), 1);

        let idx2 = queue.register();
        assert!(idx2.is_some());
        assert_eq!(queue.count(), 2);

        queue.wake_by_index(idx1.unwrap());
        let waker = queue.get(idx1.unwrap());
        assert!(waker.is_some());
        assert!(waker.unwrap().is_woken());

        queue.unregister(idx1.unwrap());
        assert_eq!(queue.count(), 1);
    }

    #[test]
    fn test_async_executor() {
        let executor = AsyncExecutor::new();
        assert!(!executor.is_running());

        executor.start();
        assert!(executor.is_running());

        executor.stop();
        assert!(!executor.is_running());
    }
}
