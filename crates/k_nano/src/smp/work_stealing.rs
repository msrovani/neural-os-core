//! Work-Stealing Queue — distribuição dinâmica de trabalho entre cores.
//! Baseado em Chase-Lev deque: cada core tem sua própria deque,
//! workers roubam trabalho de outras deques quando a própria está vazia.
//!
//! Usado para parallel matmul e outras tarefas paralelas.

use core::sync::atomic::{AtomicUsize, Ordering};
use core::cell::UnsafeCell;

/// Work-Stealing Task — closure ou função a ser executada
pub type Task = unsafe fn(*mut ());

/// Work-Stealing Deque (Chase-Lev) simplificada — array circular
pub struct WorkStealingDeque {
    buffer: UnsafeCell<[Option<Task>; 64]>, // Capacidade fixa 64
    capacity: usize,
    mask: usize,
    top: AtomicUsize,
    bottom: AtomicUsize,
}

unsafe impl Send for WorkStealingDeque {}
unsafe impl Sync for WorkStealingDeque {}

impl WorkStealingDeque {
    /// Cria uma nova work-stealing deque com capacidade fixa 64
    pub fn new() -> Self {
        WorkStealingDeque {
            buffer: UnsafeCell::new([None; 64]),
            capacity: 64,
            mask: 63,
            top: AtomicUsize::new(0),
            bottom: AtomicUsize::new(0),
        }
    }
    
    /// Push local (owner) — adiciona task no bottom
    pub fn push(&self, task: Task) {
        let b = self.bottom.load(Ordering::Relaxed);
        let t = self.top.load(Ordering::Acquire);
        
        if b - t >= self.capacity {
            // Deque cheia — descarta
            return;
        }
        
        unsafe {
            let buffer = &mut *self.buffer.get();
            buffer[(b & self.mask) as usize] = Some(task);
        }
        
        core::sync::atomic::fence(Ordering::Release);
        self.bottom.store(b + 1, Ordering::Relaxed);
    }
    
    /// Pop local (owner) — remove task do bottom
    pub fn pop(&self) -> Option<Task> {
        let b = self.bottom.load(Ordering::Relaxed) - 1;
        self.bottom.store(b, Ordering::Relaxed);
        
        let t = self.top.load(Ordering::Acquire);
        
        if t <= b {
            // Deque não vazia
            let task = unsafe {
                let buffer = &*self.buffer.get();
                buffer[(b & self.mask) as usize]
            };
            
            if t == b {
                // Último elemento — tenta CAS para evitar race com steal
                if self.top.compare_exchange_weak(t, t + 1, Ordering::SeqCst, Ordering::Relaxed).is_ok() {
                    self.bottom.store(b + 1, Ordering::Relaxed);
                    return task;
                }
            } else {
                return task;
            }
        }
        
        // Deque vazia
        self.bottom.store(b + 1, Ordering::Relaxed);
        None
    }
    
    /// Steal remoto — remove task do top (para outras cores)
    pub fn steal(&self) -> Option<Task> {
        let t = self.top.load(Ordering::Acquire);
        let b = self.bottom.load(Ordering::Acquire);
        
        if t >= b {
            return None; // Deque vazia
        }
        
        let task = unsafe {
            let buffer = &*self.buffer.get();
            buffer[(t & self.mask) as usize]
        };
        
        if self.top.compare_exchange_weak(t, t + 1, Ordering::SeqCst, Ordering::Relaxed).is_ok() {
            task
        } else {
            None
        }
    }
    
    /// Retorna o número de tasks na deque
    pub fn len(&self) -> usize {
        let b = self.bottom.load(Ordering::Relaxed);
        let t = self.top.load(Ordering::Relaxed);
        b - t
    }
    
    /// Retorna true se a deque está vazia
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Work-Stealing Pool — gerencia múltiplas deques (uma por core)
pub struct WorkStealingPool {
    deques: [WorkStealingDeque; 8], // Até 8 cores
    num_workers: usize,
}

impl WorkStealingPool {
    /// Cria um pool com N workers (cores)
    pub fn new(num_workers: usize) -> Self {
        let deques = [
            WorkStealingDeque::new(),
            WorkStealingDeque::new(),
            WorkStealingDeque::new(),
            WorkStealingDeque::new(),
            WorkStealingDeque::new(),
            WorkStealingDeque::new(),
            WorkStealingDeque::new(),
            WorkStealingDeque::new(),
        ];
        
        WorkStealingPool {
            deques,
            num_workers: num_workers.min(8),
        }
    }
    
    /// Push task na deque local do worker
    pub fn push_local(&self, worker_id: usize, task: Task) {
        if worker_id < self.num_workers {
            self.deques[worker_id].push(task);
        }
    }
    
    /// Pop task da deque local do worker
    pub fn pop_local(&self, worker_id: usize) -> Option<Task> {
        if worker_id < self.num_workers {
            self.deques[worker_id].pop()
        } else {
            None
        }
    }
    
    /// Steal task de outra deque (work-stealing)
    pub fn steal(&self, worker_id: usize) -> Option<Task> {
        if self.num_workers <= 1 {
            return None;
        }
        
        // Tenta roubar de outras deques em ordem round-robin
        for i in 0..self.num_workers {
            if i != worker_id {
                if let Some(task) = self.deques[i].steal() {
                    return Some(task);
                }
            }
        }
        None
    }
    
    /// Executa tasks até todas as deques estarem vazias
    pub fn run_until_empty(&self, worker_id: usize) {
        loop {
            // Tenta pop local primeiro
            if let Some(task) = self.pop_local(worker_id) {
                unsafe { task(core::ptr::null_mut()); }
                continue;
            }
            
            // Se local vazia, tenta steal
            if let Some(task) = self.steal(worker_id) {
                unsafe { task(core::ptr::null_mut()); }
                continue;
            }
            
            // Se local vazia e steal falhou, verifica se todas estão vazias
            let all_empty = self.deques.iter().take(self.num_workers).all(|deque| deque.is_empty());
            
            if all_empty {
                break;
            }
            
            // Yield para evitar busy-wait
            core::hint::spin_loop();
        }
    }
}
