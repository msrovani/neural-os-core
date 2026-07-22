//! NVMe Driver for Bare-Metal x86_64
//! 
/// Minimal NVMe controller driver using PCIe MMIO.
/// Configures Admin Submission/Completion Queues and provides block I/O.

use core::ptr::{read_volatile, write_volatile};
use core::sync::atomic::{AtomicBool, AtomicU32, Ordering};

/// NVMe Controller registers (BAR0)
#[repr(C)]
pub struct NvmeRegisters {
    /// Controller capabilities
    pub cap: u64,
    /// Version
    pub vs: u32,
    /// Interrupt mask set
    pub intms: u32,
    /// Interrupt mask clear
    pub intmc: u32,
    /// Controller configuration
    pub cc: u32,
    _reserved1: [u32; 3],
    /// Controller status
    pub csts: u32,
    _reserved2: [u32; 3],
    /// Admin queue attributes
    pub aqa: u32,
    /// Admin submission queue base address
    pub asq: u64,
    /// Admin completion queue base address
    pub acq: u64,
}

/// NVMe Submission Queue Entry
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct SubmissionEntry {
    /// Command dword 0 (CDW0): Opcode, FUSE, PSDT, CID
    pub cdw0: u32,
    /// Namespace identifier (NSID)
    pub nsid: u32,
    /// Reserved
    pub rsvd1: [u64; 2],
    /// Metadata pointer
    pub mptr: u64,
    /// Data pointer
    pub dptr: [u64; 2],
    /// Command dword 10 (CDW10)
    pub cdw10: u32,
    /// Command dword 11 (CDW11)
    pub cdw11: u32,
    /// Command dword 12 (CDW12)
    pub cdw12: u32,
    /// Command dword 13 (CDW13)
    pub cdw13: u32,
    /// Command dword 14 (CDW14)
    pub cdw14: u32,
    /// Command dword 15 (CDW15)
    pub cdw15: u32,
}

/// NVMe Completion Queue Entry
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CompletionEntry {
    /// Command specific result
    pub result: u32,
    /// Reserved
    pub rsvd: u32,
    /// Submission queue head pointer
    pub sqhd: u16,
    /// Submission queue identifier
    pub sqid: u16,
    /// Command identifier
    pub cid: u16,
    /// Phase tag
    pub phase: u16,
}

/// NVMe Command Opcodes
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvmeOpcode {
    /// Delete I/O Submission Queue
    DeleteIoSq = 0x00,
    /// Create I/O Submission Queue
    CreateIoSq = 0x01,
    /// Get Log Page
    GetLogPage = 0x02,
    /// Delete I/O Completion Queue
    DeleteIoCq = 0x04,
    /// Create I/O Completion Queue
    CreateIoCq = 0x05,
    /// Identify
    Identify = 0x06,
    /// Abort
    Abort = 0x08,
    /// Set Features
    SetFeatures = 0x09,
    /// Get Features
    GetFeatures = 0x0A,
    /// Async Event Request
    AsyncEventRequest = 0x0C,
    /// Namespace Management
    NamespaceMgmt = 0x0D,
    /// Firmware Commit
    FirmwareCommit = 0x10,
    /// Firmware Image Download
    FirmwareDownload = 0x11,
    /// Device Self-Test
    DeviceSelfTest = 0x14,
    /// Namespace Attachment
    NamespaceAttachment = 0x15,
    /// Keep Alive
    KeepAlive = 0x18,
    /// Flush
    Flush = 0x00,
    /// Write
    Write = 0x01,
    /// Read
    Read = 0x02,
    /// Write Uncorrectable
    WriteUncorrectable = 0x04,
    /// Compare
    Compare = 0x05,
    /// Write Zeros
    WriteZeros = 0x08,
    /// Dataset Management
    DatasetManagement = 0x09,
    /// Reservation Register
    ReservationRegister = 0x0D,
    /// Reservation Report
    ReservationReport = 0x0E,
    /// Reservation Acquire
    ReservationAcquire = 0x11,
    /// Reservation Release
    ReservationRelease = 0x15,
}

/// NVMe Queue Pair
pub struct NvmeQueue {
    /// Submission queue entries
    sq_entries: *mut SubmissionEntry,
    /// Completion queue entries
    cq_entries: *mut CompletionEntry,
    /// Submission queue head pointer
    sq_head: AtomicU32,
    /// Submission queue tail pointer
    sq_tail: AtomicU32,
    /// Completion queue head pointer
    cq_head: AtomicU32,
    /// Completion queue tail pointer
    cq_tail: AtomicU32,
    /// Queue size
    size: u32,
    /// Queue identifier
    id: u16,
    /// Phase tag for completion queue
    phase: bool,
}

impl NvmeQueue {
    /// Create a new NVMe queue pair
    /// 
    /// # Safety
    /// The caller must ensure the memory regions are valid and properly aligned
    pub unsafe fn new(
        sq_base: u64,
        cq_base: u64,
        size: u32,
        id: u16,
    ) -> Self {
        Self {
            sq_entries: sq_base as *mut SubmissionEntry,
            cq_entries: cq_base as *mut CompletionEntry,
            sq_head: AtomicU32::new(0),
            sq_tail: AtomicU32::new(0),
            cq_head: AtomicU32::new(0),
            cq_tail: AtomicU32::new(0),
            size,
            id,
            phase: true,
        }
    }

    /// Submit a command to the submission queue
    pub unsafe fn submit(&self, entry: SubmissionEntry) -> Result<(), &'static str> {
        let tail = self.sq_tail.load(Ordering::Acquire);
        let head = self.sq_head.load(Ordering::Acquire);

        // Check if queue is full
        if (tail + 1) % self.size == head {
            return Err("Submission queue full");
        }

        // Write entry to submission queue
        let idx = tail as usize;
        write_volatile(self.sq_entries.add(idx), entry);

        // Update tail pointer
        self.sq_tail.store((tail + 1) % self.size, Ordering::Release);

        // Ring doorbell (this would write to the appropriate doorbell register)
        // For admin queue, this is at offset 0x1000
        // For I/O queues, it's at offset 0x1000 + (2 * qid * 4)

        Ok(())
    }

    /// Poll for completion
    pub unsafe fn poll_completion(&self) -> Option<CompletionEntry> {
        let head = self.cq_head.load(Ordering::Acquire);
        let tail = self.cq_tail.load(Ordering::Acquire);

        if head == tail {
            return None;
        }

        let idx = head as usize;
        let entry = read_volatile(self.cq_entries.add(idx));

        // Check phase tag
        let entry_phase = (entry.phase & 1) != 0;
        if entry_phase != self.phase {
            return None;
        }

        // Update head pointer
        self.cq_head.store((head + 1) % self.size, Ordering::Release);

        // Toggle phase when we wrap around
        if (head + 1) % self.size == 0 {
            self.phase = !self.phase;
        }

        Some(entry)
    }
}

/// NVMe Controller
pub struct NvmeController {
    /// Base address of MMIO registers
    base: u64,
    /// Admin queue pair
    admin_queue: Option<NvmeQueue>,
    /// Controller ready flag
    ready: AtomicBool,
    /// Namespace ID (for I/O operations)
    nsid: u32,
}

impl NvmeController {
    /// Create a new NVMe controller instance
    /// 
    /// # Safety
    /// The base address must be a valid MMIO region for an NVMe controller
    #[must_use]
    pub unsafe fn new(base: u64) -> Self {
        Self {
            base,
            admin_queue: None,
            ready: AtomicBool::new(false),
            nsid: 1, // Default to namespace 1
        }
    }

    /// Get the registers
    #[must_use]
    unsafe fn regs(&self) -> &NvmeRegisters {
        &*(self.base as *const NvmeRegisters)
    }

    /// Initialize the NVMe controller
    pub unsafe fn init(&mut self) -> NvmeResult<()> {
        let regs = self.regs();

        // Check if controller is ready
        let csts = read_volatile(&regs.csts);
        if csts & 0x1 == 0 {
            // Controller not ready, wait for it
            let mut timeout = 1000000;
            while timeout > 0 {
                let csts = read_volatile(&regs.csts);
                if csts & 0x1 != 0 {
                    break;
                }
                timeout -= 1;
            }
            if timeout == 0 {
                return Err("Controller not ready");
            }
        }

        // Disable controller before configuration
        let mut cc = read_volatile(&regs.cc);
        cc &= !0x1; // Clear enable bit
        write_volatile(&regs.cc, cc);

        // Wait for controller to disable
        let mut timeout = 1000000;
        while timeout > 0 {
            let csts = read_volatile(&regs.csts);
            if csts & 0x1 == 0 {
                break;
            }
            timeout -= 1;
        }

        // Configure admin queues (size = 64 entries)
        const SQ_SIZE: u32 = 64;
        const CQ_SIZE: u32 = 64;

        // Allocate memory for admin queues (simplified - in real implementation, use proper allocator)
        // For now, we'll use static memory regions
        let sq_base = 0x10000000u64; // Placeholder - should be allocated
        let cq_base = 0x10010000u64; // Placeholder - should be allocated

        // Configure queue attributes
        let aqa = ((CQ_SIZE - 1) << 16) | (SQ_SIZE - 1);
        write_volatile(&regs.aqa, aqa);
        write_volatile(&regs.asq, sq_base);
        write_volatile(&regs.acq, cq_base);

        // Create admin queue
        self.admin_queue = Some(NvmeQueue::new(sq_base, cq_base, SQ_SIZE, 0));

        // Configure controller
        // Enable controller, set IO queue entry size = 0, IO completion queue entry size = 0
        // Admin queue entry size = 0 (16 bytes), Admin completion queue entry size = 0 (16 bytes)
        cc = 0x46000001; // Enable, AMS = Round Robin, MPS = 0 (4K pages), CSS = NVM command set
        write_volatile(&regs.cc, cc);

        // Wait for controller to become ready
        let mut timeout = 1000000;
        while timeout > 0 {
            let csts = read_volatile(&regs.csts);
            if csts & 0x1 != 0 {
                break;
            }
            timeout -= 1;
        }
        if timeout == 0 {
            return Err("Controller failed to enable");
        }

        self.ready.store(true, Ordering::Release);

        Ok(())
    }

    /// Read a block from the NVMe device
    /// 
    /// # Arguments
    /// * `lba` - Logical Block Address to read from
    /// * `buffer` - Buffer to store the read data (must be at least 512 bytes)
    pub unsafe fn read_block(&self, lba: u64, buffer: &mut [u8]) -> NvmeResult<()> {
        if !self.ready.load(Ordering::Acquire) {
            return Err("Controller not ready");
        }

        if buffer.len() < 512 {
            return Err("Buffer too small");
        }

        let admin_queue = self.admin_queue.as_ref().ok_or("No admin queue")?;

        // Build read command
        let mut entry = SubmissionEntry {
            cdw0: (NvmeOpcode::Read as u32) | (0 << 8), // Opcode, FUSE=0
            nsid: self.nsid,
            rsvd1: [0; 2],
            mptr: 0,
            dptr: [buffer.as_ptr() as u64, 0],
            cdw10: (lba & 0xFFFFFFFF) as u32,
            cdw11: ((lba >> 32) & 0xFFFF) as u32,
            cdw12: 0, // Number of blocks = 1
            cdw13: 0,
            cdw14: 0,
            cdw15: 0,
        };

        // Submit command
        admin_queue.submit(entry)?;

        // Poll for completion
        let mut timeout = 100000;
        while timeout > 0 {
            if let Some(completion) = admin_queue.poll_completion() {
                // Check for errors
                if completion.phase & 0x1 != 0 {
                    // Success
                    return Ok(());
                } else {
                    return Err("Command failed");
                }
            }
            timeout -= 1;
        }

        Err("Read timeout")
    }

    /// Write a block to the NVMe device
    /// 
    /// # Arguments
    /// * `lba` - Logical Block Address to write to
    /// * `buffer` - Buffer containing the data to write (must be at least 512 bytes)
    pub unsafe fn write_block(&self, lba: u64, buffer: &[u8]) -> NvmeResult<()> {
        if !self.ready.load(Ordering::Acquire) {
            return Err("Controller not ready");
        }

        if buffer.len() < 512 {
            return Err("Buffer too small");
        }

        let admin_queue = self.admin_queue.as_ref().ok_or("No admin queue")?;

        // Build write command
        let mut entry = SubmissionEntry {
            cdw0: (NvmeOpcode::Write as u32) | (0 << 8), // Opcode, FUSE=0
            nsid: self.nsid,
            rsvd1: [0; 2],
            mptr: 0,
            dptr: [buffer.as_ptr() as u64, 0],
            cdw10: (lba & 0xFFFFFFFF) as u32,
            cdw11: ((lba >> 32) & 0xFFFF) as u32,
            cdw12: 0, // Number of blocks = 1
            cdw13: 0,
            cdw14: 0,
            cdw15: 0,
        };

        // Submit command
        admin_queue.submit(entry)?;

        // Poll for completion
        let mut timeout = 100000;
        while timeout > 0 {
            if let Some(completion) = admin_queue.poll_completion() {
                // Check for errors
                if completion.phase & 0x1 != 0 {
                    // Success
                    return Ok(());
                } else {
                    return Err("Command failed");
                }
            }
            timeout -= 1;
        }

        Err("Write timeout")
    }

    /// Check if controller is ready
    #[must_use]
    pub fn is_ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }

    /// Set the namespace ID for I/O operations
    pub fn set_nsid(&mut self, nsid: u32) {
        self.nsid = nsid;
    }

    /// Get the namespace ID
    #[must_use]
    pub const fn nsid(&self) -> u32 {
        self.nsid
    }
}

/// Result type for NVMe operations
pub type NvmeResult<T> = Result<T, &'static str>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_submission_entry_size() {
        assert_eq!(core::mem::size_of::<SubmissionEntry>(), 64);
    }

    #[test]
    fn test_completion_entry_size() {
        assert_eq!(core::mem::size_of::<CompletionEntry>(), 16);
    }

    #[test]
    fn test_nvme_registers_size() {
        assert_eq!(core::mem::size_of::<NvmeRegisters>(), 56);
    }
}
