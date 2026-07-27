//! Process manager — Ring-3 userspace processes (ADR-0076 Item 1).
//! Inspired by Ferrum-OS: ELF loader, per-process address spaces, demand paging.
//!
//! Process lifecycle: load_elf → spawn → schedule → exit

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use lazy_static::lazy_static;
use ticket_lock::TicketLock;
use crate::address_space::AddressSpace;

lazy_static! {
    pub static ref PROCESS_MANAGER: TicketLock<ProcessManager> = TicketLock::new(ProcessManager::new());
}

/// Process ID.
pub type Pid = u64;

/// Maximum processes.
const MAX_PROCS: usize = 64;

/// Process state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessState {
    Ready,
    Running,
    Sleeping,
    Exited(u32),
}

/// A userspace process.
pub struct Process {
    pub pid: Pid,
    pub name: String,
    pub state: ProcessState,
    pub entry: u64,
    pub stack_top: u64,
    pub heap_brk: u64,
    pub address_space: AddressSpace,
}

impl Process {
    pub fn new(pid: Pid, name: &str, entry: u64, stack_top: u64, aspace: AddressSpace) -> Self {
        Self {
            pid,
            name: String::from(name),
            state: ProcessState::Ready,
            entry,
            stack_top,
            heap_brk: 0,
            address_space: aspace,
        }
    }
}

/// Process manager singleton.
pub struct ProcessManager {
    processes: BTreeMap<Pid, Process>,
    next_pid: Pid,
}

impl ProcessManager {
    pub fn new() -> Self {
        Self { processes: BTreeMap::new(), next_pid: 1 }
    }

    pub fn spawn(&mut self, name: &str, entry: u64, stack_top: u64, aspace: AddressSpace) -> Pid {
        let pid = self.next_pid;
        self.next_pid += 1;
        let proc = Process::new(pid, name, entry, stack_top, aspace);
        self.processes.insert(pid, proc);
        pid
    }

    pub fn get(&self, pid: Pid) -> Option<&Process> {
        self.processes.get(&pid)
    }

    pub fn get_mut(&mut self, pid: Pid) -> Option<&mut Process> {
        self.processes.get_mut(&pid)
    }

    pub fn exit(&mut self, pid: Pid, code: u32) {
        if let Some(proc) = self.processes.get_mut(&pid) {
            proc.state = ProcessState::Exited(code);
        }
    }

    pub fn count(&self) -> usize { self.processes.len() }

    /// List PIDs of all processes.
    pub fn list(&self) -> Vec<Pid> {
        self.processes.keys().copied().collect()
    }
}
