//! Async Runtime for Bare-Metal x86_64
//! 
//! Lightweight async executor based on Local APIC Timer interrupts.
//! Uses lock-free ring buffer for waker queue to avoid idle CPU consumption.

use alloc::boxed::Box;
use core::cell::UnsafeCell;
use core::mem::MaybeUninit;
use core::pin::Pin;
use core::sync::atomic::{AtomicBool, AtomicUsize, AtomicU64, Ordering};
use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

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
            buffer: UnsafeCell::new([const { MaybeUninit::uninit() }; 256]),
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
            let value = core::ptr::read(buffer[idx].as_ptr());
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

/// Task state for the async executor
struct TaskState {
    future: Option<Pin<Box<dyn core::future::Future<Output = ()> + Send>>>,
    waker: Option<Waker>,
    woken: AtomicBool,
}

impl TaskState {
    const fn new() -> Self {
        Self {
            future: None,
            waker: None,
            woken: AtomicBool::new(false),
        }
    }
}

/// Waker queue for tracking active futures
pub struct WakerQueue {
    /// Array of task states
    tasks: [UnsafeCell<TaskState>; 64],
    /// Count of active tasks
    count: AtomicUsize,
}

// ponytail: UnsafeCell is not Sync by default, but WakerQueue is only accessed
// from the BSP with interrupts disabled — safe for static.
unsafe impl Sync for WakerQueue {}

impl WakerQueue {
    /// Create a new waker queue
    #[must_use]
    pub const fn new() -> Self {
        const INIT: UnsafeCell<TaskState> = UnsafeCell::new(TaskState::new());
        Self {
            tasks: [INIT; 64],
            count: AtomicUsize::new(0),
        }
    }

    /// Register a new future
    /// 
    /// Returns the index of the registered task, or None if full
    pub fn register(&self, future: Pin<Box<dyn core::future::Future<Output = ()> + Send>>) -> Option<usize> {
        let count = self.count.load(Ordering::Acquire);
        if count >= 64 {
            return None;
        }

        for i in 0..64 {
            unsafe {
                let task = &mut *self.tasks[i].get();
                if task.future.is_none() {
                    task.future = Some(future);
                    self.count.fetch_add(1, Ordering::Release);
                    return Some(i);
                }
            }
        }

        None
    }

    /// Wake a specific task by index
    pub fn wake_by_index(&self, index: usize) {
        if index < 64 {
            unsafe {
                let task = &mut *self.tasks[index].get();
                task.woken.store(true, Ordering::Release);
                if let Some(waker) = &task.waker {
                    waker.wake_by_ref();
                }
            }
        }
    }

    /// Wake all registered tasks
    pub fn wake_all(&self) {
        for i in 0..64 {
            self.wake_by_index(i);
        }
    }

    /// Get a task by index
    #[must_use]
    pub fn get(&self, index: usize) -> Option<&TaskState> {
        if index < 64 {
            unsafe {
                let task = &*self.tasks[index].get();
                if task.future.is_some() {
                    Some(task)
                } else {
                    None
                }
            }
        } else {
            None
        }
    }

    /// Unregister a task
    pub fn unregister(&self, index: usize) {
        if index < 64 {
            unsafe {
                let task = &mut *self.tasks[index].get();
                task.future = None;
                task.waker = None;
                task.woken.store(false, Ordering::Release);
                self.count.fetch_sub(1, Ordering::Release);
            }
        }
    }

    /// Get the count of active tasks
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
    /// Returns the task index for the future
    pub fn register_future(&self, future: Pin<Box<dyn core::future::Future<Output = ()> + Send>>) -> Option<usize> {
        self.waker_queue.register(future)
    }

    /// Wake a future by index (called from interrupt handler)
    pub fn wake_future(&self, index: usize) {
        self.waker_queue.wake_by_index(index);
        // Also push to wake channel for processing
        let _ = self.wake_channel.try_push(index);
    }

    /// Process wake notifications (called from main loop)
    pub fn process_wakes(&self) {
        while let Some(index) = self.wake_channel.try_pop() {
            // Poll the future at this index
            self.poll_task(index);
        }
    }

    /// Poll a specific task
    fn poll_task(&self, index: usize) {
        if index >= 64 {
            return;
        }

        unsafe {
            let task = &mut *self.waker_queue.tasks[index].get();
            if let Some(future) = task.future.as_mut() {
                // Create a waker for this task
                let waker = self.create_waker(index);
                task.waker = Some(waker.clone());
                
                let mut cx = Context::from_waker(&waker);
                match future.as_mut().poll(&mut cx) {
                    Poll::Ready(()) => {
                        // Task completed
                        task.future = None;
                        task.waker = None;
                        self.waker_queue.count.fetch_sub(1, Ordering::Release);
                    }
                    Poll::Pending => {
                        // Task still pending, waker is stored
                    }
                }
            }
        }
    }

    /// Create a waker for a specific task index
    fn create_waker(&self, index: usize) -> Waker {
        // Create a raw waker that will wake the task by index
        let data = index as *const ();
        let vtable = &RawWakerVTable::new(
            |data| RawWaker::new(data, &VTABLE), // clone
            |data| {
                // wake
                let idx = data as usize;
                crate::async_rt::global_executor().wake_future(idx);
            },
            |data| {
                // wake_by_ref
                let idx = data as usize;
                crate::async_rt::global_executor().wake_future(idx);
            },
            |_data| {
                // drop - nothing to do
            },
        );
        unsafe { Waker::from_raw(RawWaker::new(data, vtable)) }
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

/// VTABLE for raw waker
static VTABLE: RawWakerVTable = RawWakerVTable::new(
    |data| RawWaker::new(data, &VTABLE), // clone
    |data| {
        // wake
        let idx = data as usize;
        crate::async_rt::global_executor().wake_future(idx);
    },
    |data| {
        // wake_by_ref
        let idx = data as usize;
        crate::async_rt::global_executor().wake_future(idx);
    },
    |_data| {
        // drop - nothing to do
    },
);

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

    // Register a demo TimerFuture that fires every 100 ticks
    let _ = executor.register_future(Box::pin(TimerFuture::new(100)));

    // Configure APIC timer (this would call into the apic module)
    // For now, this is a stub - the timer handler is already registered in IDT
    // unsafe {
    //     crate::apic::configure_timer(32, 0x800000); // Vector 32, count
    // }
}

/// TimerFuture — wakes after N scheduler ticks
/// 
/// Simple timer future for testing the async executor.
/// Uses the global executor's wake mechanism.
pub struct TimerFuture {
    ticks_remaining: AtomicU64,
    total_ticks: u64,
    index: AtomicUsize,
}

impl TimerFuture {
    /// Create a new TimerFuture that completes after `ticks` scheduler ticks
    pub fn new(ticks: u64) -> Self {
        Self {
            ticks_remaining: AtomicU64::new(ticks),
            total_ticks: ticks,
            index: AtomicUsize::new(0),
        }
    }
}

impl core::future::Future for TimerFuture {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();
        
        // Register waker on first poll
        if this.index.load(Ordering::Acquire) == 0 {
            let idx = global_executor().register_future(Box::pin(async {})).unwrap_or(0);
            this.index.store(idx, Ordering::Release);
        }
        
        let remaining = this.ticks_remaining.load(Ordering::Acquire);
        if remaining == 0 {
            Poll::Ready(())
        } else {
            // Store waker for when timer fires
            this.ticks_remaining.fetch_sub(1, Ordering::Release);
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
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
    fn test_waker_queue() {
        let queue = WakerQueue::new();
        assert_eq!(queue.count(), 0);

        // Can't easily test without a real future, but we can test registration
        // This is a placeholder for the test structure
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