//! Core Pair Allocator for Elastic Local Scaling
//! 
//! Implements core pair allocation with MWAIT power management and wake-up
//! triggers based on affect vector (uncertainty/urgency) from hermes.

use core::sync::atomic::{AtomicBool, AtomicU8, AtomicU64, Ordering};
use crate::async_rt::SpscChannel;

/// Core role in the system
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreRole {
    /// System Core: runs k-nano (I/O, VFS), hermes (supervisor), jarbas (UI)
    System = 0,
    /// Compute Core: dedicated to BitNet inference loop via SIMD
    Compute = 1,
    /// Memory Core: handles external memory VFS and fact indexing
    Memory = 2,
    /// Worker Core: small logical verification cells or async support tasks
    Worker = 3,
    /// Idle: core in MWAIT low-power state
    Idle = 4,
}

/// Core pair state
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorePairState {
    /// Active: both cores running
    Active = 0,
    /// MWAIT: pair in low-power state
    MWait = 1,
    /// Waking: transitioning from MWAIT to active
    Waking = 2,
}

/// Core pair descriptor
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct CorePair {
    /// First core ID
    pub core0: u8,
    /// Second core ID (hyper-threading sibling or L2/L3 cache sharing)
    pub core1: u8,
    /// Current state
    pub state: CorePairState,
    /// Role of core0
    pub role0: CoreRole,
    /// Role of core1
    pub role1: CoreRole,
    /// Last wake timestamp
    pub last_wake: u64,
}

impl CorePair {
    /// Create a new core pair
    #[must_use]
    pub const fn new(core0: u8, core1: u8) -> Self {
        Self {
            core0,
            core1,
            state: CorePairState::MWait,
            role0: CoreRole::Idle,
            role1: CoreRole::Idle,
            last_wake: 0,
        }
    }

    /// Check if this pair contains a specific core
    #[must_use]
    pub const fn contains(&self, core: u8) -> bool {
        self.core0 == core || self.core1 == core
    }

    /// Get the sibling core
    #[must_use]
    pub const fn sibling(&self, core: u8) -> Option<u8> {
        if core == self.core0 {
            Some(self.core1)
        } else if core == self.core1 {
            Some(self.core0)
        } else {
            None
        }
    }
}

/// Bipole mode configuration (2-core fallback)
#[repr(C)]
#[derive(Debug)]
pub struct BipoleMode {
    /// Core 0 (System Core)
    pub system_core: u8,
    /// Core 1 (Compute Core)
    pub compute_core: u8,
    /// Communication channel (64-byte SpscChannel)
    pub channel: *mut SpscChannel<u8>,
    /// Active flag
    pub active: AtomicBool,
}

impl BipoleMode {
    /// Create bipole mode configuration
    #[must_use]
    pub const fn new(system_core: u8, compute_core: u8) -> Self {
        Self {
            system_core,
            compute_core,
            channel: core::ptr::null_mut(),
            active: AtomicBool::new(false),
        }
    }

    /// Activate bipole mode
    pub fn activate(&mut self, channel: *mut SpscChannel<u8>) {
        self.channel = channel;
        self.active.store(true, Ordering::Release);
    }

    /// Deactivate bipole mode
    pub fn deactivate(&self) {
        self.active.store(false, Ordering::Release);
    }

    /// Check if bipole mode is active
    #[must_use]
    pub fn is_active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }

    /// Send message from system core to compute core
    pub fn send_to_compute(&self, data: u8) -> bool {
        if !self.is_active() || self.channel.is_null() {
            return false;
        }
        unsafe {
            (*self.channel).try_push(data)
        }
    }

    /// Receive message on compute core
    pub fn receive_on_compute(&self) -> Option<u8> {
        if !self.is_active() || self.channel.is_null() {
            return None;
        }
        unsafe {
            (*self.channel).try_pop()
        }
    }
}

/// Affect vector from hermes for wake-up triggers
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct AffectVector {
    /// Uncertainty level (0.0 to 1.0)
    pub uncertainty: f32,
    /// Urgency level (0.0 to 1.0)
    pub urgency: f32,
    /// Confidence level (0.0 to 1.0)
    pub confidence: f32,
}

impl AffectVector {
    /// Create a new affect vector
    #[must_use]
    pub const fn new(uncertainty: f32, urgency: f32, confidence: f32) -> Self {
        Self {
            uncertainty,
            urgency,
            confidence,
        }
    }

    /// Check if wake-up is triggered (uncertainty > 0.75 or urgency > 0.75)
    #[must_use]
    pub fn should_wake(&self) -> bool {
        self.uncertainty > 0.75 || self.urgency > 0.75
    }

    /// Get wake-up priority score
    #[must_use]
    pub fn wake_priority(&self) -> f32 {
        self.uncertainty * 0.6 + self.urgency * 0.4
    }
}

/// Core Pair Allocator
/// 
/// Manages core pair allocation with MWAIT power management and dynamic wake-up.
pub struct CorePairAllocator {
    /// Array of core pairs (max 64 cores = 32 pairs)
    pairs: [Option<CorePair>; 32],
    /// Number of active pairs
    active_pairs: AtomicU8,
    /// Total number of cores
    total_cores: u8,
    /// Bipole mode configuration (fallback for 2-core systems)
    bipole: BipoleMode,
    /// Current affect vector
    affect: AffectVector,
    /// MWAIT C-state (0 = C0, 1 = C1, 2 = C2, etc.)
    mwait_cstate: AtomicU8,
}

impl CorePairAllocator {
    /// Create a new core pair allocator
    #[must_use]
    pub const fn new(total_cores: u8) -> Self {
        const INIT: Option<CorePair> = None;
        Self {
            pairs: [INIT; 32],
            active_pairs: AtomicU8::new(0),
            total_cores,
            bipole: BipoleMode::new(0, 1),
            affect: AffectVector::new(0.0, 0.0, 1.0),
            mwait_cstate: AtomicU8::new(1), // C1 by default
        }
    }

    /// Initialize core pairs based on CPU topology
    /// 
    /// For simplicity, this assumes pairs are (0,1), (2,3), (4,5), etc.
    /// In a real implementation, this would query ACPI MADT for actual topology.
    pub fn initialize(&mut self) {
        let num_pairs = self.total_cores / 2;
        
        for i in 0..num_pairs as usize {
            let core0 = (i * 2) as u8;
            let core1 = (i * 2 + 1) as u8;
            self.pairs[i] = Some(CorePair::new(core0, core1));
        }

        // Configure bipole mode for 2-core systems
        if self.total_cores == 2 {
            self.bipole = BipoleMode::new(0, 1);
        }
    }

    /// Activate bipole mode (2-core fallback)
    pub fn activate_bipole(&mut self, channel: *mut SpscChannel<u8>) {
        if self.total_cores == 2 {
            self.bipole.activate(channel);
            
            // Set roles
            if let Some(pair) = &mut self.pairs[0] {
                pair.role0 = CoreRole::System;
                pair.role1 = CoreRole::Compute;
                pair.state = CorePairState::Active;
            }
        }
    }

    /// Allocate a core pair for a specific role
    pub fn allocate_pair(&mut self, role0: CoreRole, role1: CoreRole) -> Option<&mut CorePair> {
        for pair in &mut self.pairs {
            if let Some(p) = pair {
                if p.state == CorePairState::MWait {
                    p.role0 = role0;
                    p.role1 = role1;
                    p.state = CorePairState::Active;
                    self.active_pairs.fetch_add(1, Ordering::Release);
                    return Some(p);
                }
            }
        }
        None
    }

    /// Put a core pair into MWAIT state
    pub fn mwait_pair(&mut self, core: u8) -> bool {
        for pair in &mut self.pairs {
            if let Some(p) = pair {
                if p.contains(core) && p.state == CorePairState::Active {
                    p.state = CorePairState::MWait;
                    self.active_pairs.fetch_sub(1, Ordering::Release);
                    return true;
                }
            }
        }
        false
    }

    /// Wake up core pairs based on affect vector
    /// 
    /// Called by hermes when uncertainty or urgency exceeds threshold
    pub fn wake_by_affect(&mut self, affect: AffectVector) {
        self.affect = affect;

        if !affect.should_wake() {
            return;
        }

        // Wake up idle pairs based on priority
        let priority = affect.wake_priority();
        let pairs_to_wake = if priority > 0.9 { 4 } else { 2 };

        // Pass 1: collect indices of MWait pairs to wake (immutable borrow of self.pairs)
        let mut wake_indices: [Option<usize>; 32] = [None; 32];
        let mut woken = 0usize;
        for (i, pair) in self.pairs.iter().enumerate() {
            if woken >= pairs_to_wake {
                break;
            }
            if let Some(p) = pair {
                if p.state == CorePairState::MWait {
                    wake_indices[i] = Some(i);
                    woken += 1;
                }
            }
        }

        // Pass 2: mutate state and send IPIs (mutable borrow of self.pairs)
        let timestamp = self.get_timestamp();
        let mut actually_woken = 0u8;
        for i in 0..32 {
            if wake_indices[i].is_some() {
                // ponytail: extract core IDs before mutable borrow to avoid conflict with send_wake_ipi
                let (core0, core1) = {
                    if let Some(p) = &self.pairs[i] {
                        if p.state == CorePairState::MWait {
                            (p.core0, p.core1)
                        } else {
                            continue;
                        }
                    } else {
                        continue;
                    }
                };
                // Mutate state in a block that drops the mutable borrow before calling send_wake_ipi
                {
                    if let Some(p) = &mut self.pairs[i] {
                        if p.state == CorePairState::MWait {
                            p.state = CorePairState::Waking;
                        }
                    }
                }
                self.send_wake_ipi(core0);
                self.send_wake_ipi(core1);
                {
                    if let Some(p) = &mut self.pairs[i] {
                        p.state = CorePairState::Active;
                        p.last_wake = timestamp;
                    }
                }
                self.active_pairs.fetch_add(1, Ordering::Release);
                actually_woken += 1;
            }
        }
        let _ = actually_woken;
    }

    /// Send wake IPI to a specific core
    fn send_wake_ipi(&self, core: u8) {
        // In a real implementation, this would use APIC IPI
        // For now, this is a stub
        let _ = core;
    }

    /// Get current timestamp
    fn get_timestamp(&self) -> u64 {
        // In a real implementation, this would read TSC
        // For now, return a placeholder
        0
    }

    /// Set MWAIT C-state
    pub fn set_mwait_cstate(&self, cstate: u8) {
        self.mwait_cstate.store(cstate, Ordering::Release);
    }

    /// Get MWAIT C-state
    #[must_use]
    pub fn mwait_cstate(&self) -> u8 {
        self.mwait_cstate.load(Ordering::Acquire)
    }

    /// Get number of active pairs
    #[must_use]
    pub fn active_pairs(&self) -> u8 {
        self.active_pairs.load(Ordering::Acquire)
    }

    /// Get bipole mode configuration
    #[must_use]
    pub const fn bipole(&self) -> &BipoleMode {
        &self.bipole
    }

    /// Get current affect vector
    #[must_use]
    pub const fn affect(&self) -> &AffectVector {
        &self.affect
    }

    /// Update affect vector
    pub fn update_affect(&mut self, affect: AffectVector) {
        self.affect = affect;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_core_pair_creation() {
        let pair = CorePair::new(0, 1);
        assert_eq!(pair.core0, 0);
        assert_eq!(pair.core1, 1);
        assert_eq!(pair.state, CorePairState::MWait);
    }

    #[test]
    fn test_core_pair_contains() {
        let pair = CorePair::new(0, 1);
        assert!(pair.contains(0));
        assert!(pair.contains(1));
        assert!(!pair.contains(2));
    }

    #[test]
    fn test_core_pair_sibling() {
        let pair = CorePair::new(0, 1);
        assert_eq!(pair.sibling(0), Some(1));
        assert_eq!(pair.sibling(1), Some(0));
        assert_eq!(pair.sibling(2), None);
    }

    #[test]
    fn test_bipole_mode() {
        let mut bipole = BipoleMode::new(0, 1);
        assert!(!bipole.is_active());
        
        let mut channel = SpscChannel::new();
        bipole.activate(&mut channel as *mut _);
        assert!(bipole.is_active());
    }

    #[test]
    fn test_affect_vector_wake_trigger() {
        let affect = AffectVector::new(0.8, 0.5, 0.7);
        assert!(affect.should_wake());
        
        let affect = AffectVector::new(0.5, 0.5, 0.7);
        assert!(!affect.should_wake());
    }

    #[test]
    fn test_affect_vector_priority() {
        let affect1 = AffectVector::new(0.9, 0.5, 0.7);
        let affect2 = AffectVector::new(0.5, 0.9, 0.7);
        
        assert!(affect1.wake_priority() > affect2.wake_priority());
    }

    #[test]
    fn test_core_pair_allocator_init() {
        let mut allocator = CorePairAllocator::new(4);
        allocator.initialize();
        
        assert_eq!(allocator.total_cores, 4);
        assert!(allocator.pairs[0].is_some());
        assert!(allocator.pairs[1].is_some());
    }

    #[test]
    fn test_core_pair_allocator_bipole() {
        let mut allocator = CorePairAllocator::new(2);
        allocator.initialize();
        
        let mut channel = SpscChannel::new();
        allocator.activate_bipole(&mut channel as *mut _);
        
        assert!(allocator.bipole().is_active());
    }

    #[test]
    fn test_allocate_pair() {
        let mut allocator = CorePairAllocator::new(4);
        allocator.initialize();
        
        let pair = allocator.allocate_pair(CoreRole::Compute, CoreRole::Compute);
        assert!(pair.is_some());
        
        assert_eq!(allocator.active_pairs(), 1);
    }

    #[test]
    fn test_mwait_pair() {
        let mut allocator = CorePairAllocator::new(4);
        allocator.initialize();
        
        allocator.allocate_pair(CoreRole::Compute, CoreRole::Compute);
        assert_eq!(allocator.active_pairs(), 1);
        
        let result = allocator.mwait_pair(0);
        assert!(result);
        assert_eq!(allocator.active_pairs(), 0);
    }

    #[test]
    fn test_wake_by_affect() {
        let mut allocator = CorePairAllocator::new(4);
        allocator.initialize();
        
        // Put pairs in MWAIT
        allocator.allocate_pair(CoreRole::Compute, CoreRole::Compute);
        allocator.mwait_pair(0);
        assert_eq!(allocator.active_pairs(), 0);
        
        // Wake by affect
        let affect = AffectVector::new(0.9, 0.5, 0.7);
        allocator.wake_by_affect(affect);
        
        assert!(allocator.active_pairs() > 0);
    }

    /// Simulate dual-core boot (i3 fallback)
    #[test]
    fn test_dual_core_boot_simulation() {
        // Simulate a 2-core system (e.g., Intel i3)
        let mut allocator = CorePairAllocator::new(2);
        allocator.initialize();
        
        // Verify bipole mode is configured
        assert_eq!(allocator.total_cores, 2);
        assert_eq!(allocator.bipole().system_core, 0);
        assert_eq!(allocator.bipole().compute_core, 1);
        
        // Activate bipole mode with communication channel
        let mut channel = SpscChannel::new();
        allocator.activate_bipole(&mut channel as *mut _);
        
        assert!(allocator.bipole().is_active());
        
        // Verify core roles are set
        if let Some(pair) = &allocator.pairs[0] {
            assert_eq!(pair.role0, CoreRole::System);
            assert_eq!(pair.role1, CoreRole::Compute);
            assert_eq!(pair.state, CorePairState::Active);
        }
        
        // Test communication between cores
        let test_data = 0xAB;
        assert!(allocator.bipole().send_to_compute(test_data));
        
        let received = allocator.bipole().receive_on_compute();
        assert_eq!(received, Some(test_data));
        
        // Test affect-based wake-up (should not wake in bipole mode as both cores are active)
        let affect = AffectVector::new(0.9, 0.5, 0.7);
        allocator.wake_by_affect(affect);
        
        // Both cores should remain active in bipole mode
        assert_eq!(allocator.active_pairs(), 1);
    }

    /// Simulate 4-core system with elastic scaling
    #[test]
    fn test_quad_core_elastic_scaling() {
        let mut allocator = CorePairAllocator::new(4);
        allocator.initialize();
        
        // Initially, only first pair should be active
        allocator.allocate_pair(CoreRole::System, CoreRole::Compute);
        assert_eq!(allocator.active_pairs(), 1);
        
        // Put pair in MWAIT
        allocator.mwait_pair(0);
        assert_eq!(allocator.active_pairs(), 0);
        
        // Wake by high uncertainty
        let affect = AffectVector::new(0.9, 0.5, 0.7);
        allocator.wake_by_affect(affect);
        
        // Should wake up pairs based on priority
        assert!(allocator.active_pairs() > 0);
    }
}
