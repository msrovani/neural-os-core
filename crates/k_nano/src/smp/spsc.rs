//! SPSC (Single Producer Single Consumer) Queue — lock-free circular buffer.
//! Usado para comunicação entre cores no SMP.
//! Baseado em MPMC de Dmitry Vyukov, simplificado para SPSC.

use core::sync::atomic::{AtomicUsize, Ordering};

pub struct SpscQueue<T> {
    buffer: *mut T,
    capacity: usize,
    mask: usize,
    head: AtomicUsize,
    tail: AtomicUsize,
}

unsafe impl<T: Send> Send for SpscQueue<T> {}
unsafe impl<T: Send> Sync for SpscQueue<T> {}

impl<T> SpscQueue<T> {
    /// Cria uma nova SPSC queue com capacidade potência de 2
    pub fn new(capacity: usize) -> Option<Self> {
        if !capacity.is_power_of_two() || capacity < 2 {
            return None;
        }
        
        let buffer = unsafe {
            let layout = alloc::alloc::Layout::array::<T>(capacity).ok()?;
            let ptr = alloc::alloc::alloc(layout) as *mut T;
            if ptr.is_null() {
                alloc::alloc::dealloc(ptr as *mut u8, layout);
                return None;
            }
            ptr
        };
        
        Some(SpscQueue {
            buffer,
            capacity,
            mask: capacity - 1,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        })
    }
    
    /// Envia um item (producer)
    pub fn try_send(&self, item: T) -> Result<(), T> {
        let pos = self.tail.load(Ordering::Relaxed);
        let next_pos = pos + 1;
        
        // Verifica se a queue está cheia
        if next_pos - self.head.load(Ordering::Acquire) >= self.capacity {
            return Err(item);
        }
        
        unsafe {
            core::ptr::write(self.buffer.add(pos & self.mask), item);
        }
        
        self.tail.store(next_pos, Ordering::Release);
        Ok(())
    }
    
    /// Recebe um item (consumer)
    pub fn try_recv(&self) -> Option<T> {
        let pos = self.head.load(Ordering::Relaxed);
        
        // Verifica se a queue está vazia
        if pos == self.tail.load(Ordering::Acquire) {
            return None;
        }
        
        let item = unsafe {
            core::ptr::read(self.buffer.add(pos & self.mask))
        };
        
        self.head.store(pos + 1, Ordering::Release);
        Some(item)
    }
    
    /// Retorna o número de itens na queue
    pub fn len(&self) -> usize {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.head.load(Ordering::Relaxed);
        tail - head
    }
    
    /// Retorna true se a queue está vazia
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl<T> Drop for SpscQueue<T> {
    fn drop(&mut self) {
        unsafe {
            // Drop todos os itens restantes
            let head = self.head.load(Ordering::Relaxed);
            let tail = self.tail.load(Ordering::Relaxed);
            for i in head..tail {
                core::ptr::drop_in_place(self.buffer.add(i & self.mask));
            }
            
            // Dealloc buffer
            let layout = alloc::alloc::Layout::array::<T>(self.capacity).ok();
            if let Some(layout) = layout {
                alloc::alloc::dealloc(self.buffer as *mut u8, layout);
            }
        }
    }
}
