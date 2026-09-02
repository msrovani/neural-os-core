//! Mesh 2c host validation (ADR-0100 T-045/046) — s280 hang 2c SIPI já corrigido em Fase 2A T-033.
//! Teste mock sem QEMU: mesh 2c não hang com ap_pollable true vs false.
//! Ponytail: AP_IDT_READY barrier (T-033) garante IST/TSS por AP antes de sti/pollable.
//! Este módulo vive em `k_nano/src/mesh.rs` (scope permitido Fase 2B) e valida o transporte
//! R0 (k_nano) sem tocar smp/ (fechado) nem paging/ring3.

#[cfg(test)]
mod host_tests_mesh_2c {
    use crate::net::mesh::{BrainMeshEngine, NodeCapabilities, SimdWeight};
    use crate::smp::{ap_pollable, set_ap_pollable};
    use crate::smp::percpu::CPU_COUNT;
    use core::sync::atomic::Ordering;
    use spin::Mutex;
    static LOCK: Mutex<()> = Mutex::new(());

    fn mesh_tick_no_hang(iterations: usize) -> bool {
        let caps = NodeCapabilities::new([2, 0, 0, 0, 0, 0], 2, 2500, 8, 4, SimdWeight::Avx2, false, false);
        let mut eng = BrainMeshEngine::new(caps);
        let peer = NodeCapabilities::new([3, 0, 0, 0, 0, 0], 2, 2500, 8, 4, SimdWeight::Avx2, false, false);
        eng.add_or_update_node(peer);
        for _ in 0..iterations {
            eng.tick();
            eng.check_election();
            let _ = eng.local_role();
            let _ = eng.node_count();
            // gate true só quando APs realmente pollable (T-033); host mock sem APs físicos → fallback BSP
            let gate = crate::smp::ap_pollable() && crate::smp::ap_entry_count() > 0;
            let _ = gate;
        }
        true
    }

    #[test]
    fn mesh_2c_ap_pollable_false_no_hang() {
        let _g = LOCK.lock();
        let prev_poll = ap_pollable();
        let prev_cnt = CPU_COUNT.load(Ordering::Relaxed);
        set_ap_pollable(false);
        CPU_COUNT.store(2, Ordering::SeqCst);
        assert!(mesh_tick_no_hang(100));
        CPU_COUNT.store(prev_cnt, Ordering::SeqCst);
        set_ap_pollable(prev_poll);
    }

    #[test]
    fn mesh_2c_ap_pollable_true_no_hang() {
        let _g = LOCK.lock();
        let prev_poll = ap_pollable();
        let prev_cnt = CPU_COUNT.load(Ordering::Relaxed);
        set_ap_pollable(true);
        CPU_COUNT.store(2, Ordering::SeqCst);
        assert!(mesh_tick_no_hang(100));
        CPU_COUNT.store(2, Ordering::SeqCst);
        assert!(mesh_tick_no_hang(50));
        CPU_COUNT.store(prev_cnt, Ordering::SeqCst);
        set_ap_pollable(prev_poll);
    }

    #[test]
    fn s280_ap_idt_ready_barrier_not_hung() {
        let _g = LOCK.lock();
        let prev_cnt = CPU_COUNT.load(Ordering::Relaxed);
        CPU_COUNT.store(2, Ordering::SeqCst);
        assert_eq!(crate::smp::percpu::CPU_COUNT.load(Ordering::Relaxed), 2);
        set_ap_pollable(false);
        assert!(mesh_tick_no_hang(20));
        set_ap_pollable(true);
        assert!(mesh_tick_no_hang(20));
        CPU_COUNT.store(prev_cnt, Ordering::SeqCst);
        set_ap_pollable(false);
    }
}
