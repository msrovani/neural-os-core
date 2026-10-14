//! AP Worker Jobs — mesh/FL/CRDT tasks offloaded from BSP bei_tick (P5/Item 10).
//!
//! These jobs are enqueued by bei_tick and executed by AP workers in their
//! idle loop (k_nano::smp::ap_work::ap_idle_loop). The BSP submits jobs via
//! k_nano::smp::ap_work::enqueue and the AP workers execute them in their
//! idle loop (k_nano::smp::ap_work::ap_idle_loop).

use core::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};

/// Job: mesh/FL/CRDT tick — runs the mesh/FL/CRDT tasks that were previously
/// in bei_tick. This job is submitted by bei_tick and executed by AP workers.
pub unsafe fn mesh_fl_crdt_tick_job(_job_id: usize, _worker: usize) {
    // F1 (ADR-0081 Fase B): advertise de capabilities no heartbeat — refresh
    // a cada ~110 ticks (cadência do heartbeat) p/ capturar modelos carregados
    // tardiamente no boot. Cada heartbeat TX lê `local_caps()`.
    static LAST_CAPS_REFRESH: AtomicU64 = AtomicU64::new(0);
    let caps_now = k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
    let caps_last = LAST_CAPS_REFRESH.load(Ordering::Relaxed);
    if caps_last == 0 || caps_now.wrapping_sub(caps_last) >= 110 {
        LAST_CAPS_REFRESH.store(caps_now, Ordering::Relaxed);
        let mut caps = k_nano::net::mesh::CAP_COMPUTE;
        if cortex::cortex::model_is_loaded() {
            caps |= k_nano::net::mesh::CAP_LLM;
        }
        if cortex::cortex::hwexpert_is_loaded() {
            caps |= k_nano::net::mesh::CAP_HWEXPERT;
        }
        if cortex::cortex::rustcoder_is_loaded() {
            caps |= k_nano::net::mesh::CAP_RUSTCODER;
        }
        if k_ai::sgdb::ready() {
            caps |= k_nano::net::mesh::CAP_SGDB_READY;
        }
        k_nano::net::mesh::set_local_caps(caps);
    }

    // Com peer TOFU settled, ativa sync de skills + marketplace (idempotente).
    // Settle evita Master push/ROLE antes do Worker vincular nossa pk (drops
    // "peer desconhecido"). Na 1ª transição settled→true, limpa synced p/
    // re-empurrar skills que teriam sido marcadas cedo demais.
    if k_nano::net::mesh::tofu_settled() {
        static WAS_SETTLED: AtomicBool = AtomicBool::new(false);
        if !WAS_SETTLED.swap(true, Ordering::AcqRel) {
            // GOAL3: skill nova só no Master, antes do re-push (Worker não a tem).
            crate::skill_sync::register_mesh_g3_probe_on_master();
            crate::skill_sync::clear_synced_for_resync();
            k_nano::slog_bin!("P2P", "ok", "TOFU settled — SkillSync/MKTP liberados");
        }
        crate::skill_sync::activate_global();
        crate::skill_marketplace::activate_global();
    }

    // Consumo dos pacotes P2P não-heartbeat (EventBus) — lazy subscribe + dreno.
    crate::skill_sync::poll_p2p();
    crate::skill_marketplace::poll_p2p();

    // RX de requests mesh (Master responde só se settled + Master — ver gates).
    cortex::compute::poll_mesh_requests();
    cortex::mesh_distrib::poll_expert_requests();
    cortex::speculative::dsd_tick(0);

    // TX de dados (skills/CRDT/FL/MKTP/self-test) só após TOFU settle.
    // Heartbeat/ROLE já gated em k_nano::mesh (FORCE_HB + assign_roles).
    if !k_nano::net::mesh::tofu_settled() {
        return;
    }

    // Sync de skills (Master push / diff) + marketplace broadcast periódico.
    let now = k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
    crate::skill_sync::sync_skills(now);
    crate::skill_marketplace::marketplace_tick(k_nano::net::mesh::node_id());

    // Memória coletiva L4: difunde os pares do SelfLearningAgent (k_ai) como
    // MemoryDocs (throttled ~500 ticks interno). RX "MEM\0" de qualquer layer
    // já é aplicado via put_doc no mesh_knowledge::poll_p2p.
    crate::mesh_knowledge::broadcast_learner_memory();

    // Self-test matmul 64×64 (~17KB → FRAG): qualquer não-Master após TOFU.
    // AIOS: sob pressão de RAM (<1.5G) NÃO dispara FRAG — DEGRADED honesto.
    static MESH_SELFTEST_TRIES: AtomicU32 = AtomicU32::new(0);
    let role = k_nano::net::mesh::local_role();
    let non_master = matches!(
        role,
        k_nano::net::mesh::NodeRole::Worker
            | k_nano::net::mesh::NodeRole::Memory
            | k_nano::net::mesh::NodeRole::Compute
    );
    if MESH_SELFTEST_TRIES.load(Ordering::Relaxed) < 5
        && non_master
        && k_nano::net::mesh::MESH_ENGINE.lock().as_ref().map_or(false, |e| e.node_count() >= 1)
    {
        if k_nano::memory::refuse_heavy_frag() {
            if MESH_SELFTEST_TRIES.fetch_add(1, Ordering::Relaxed) == 0 {
                let ram = k_nano::memory::TOTAL_RAM_MB
                    .load(Ordering::Relaxed);
                k_nano::slog_bin!(
                    "MESH", "warn",
                    "self-test FRAG skip DEGRADED RAM={}MB (AIOS: nó frugal — mesh UI ok, matmul local)"
                    , ram
                );
            }
        } else {
            MESH_SELFTEST_TRIES.fetch_add(1, Ordering::Relaxed);
            cortex::compute::mesh_matmul_self_test();
        }
    }

    // ADR-0081 Fase C (C4/C5): CRDT version sync + FL federado via P2P real.
    // k_ai consome o EventBus P2P_PACKET (assinatura já verificada no ingress
    // do k_nano — Fase A fail-closed). No-op quando role == Undecided.
    k_ai::sgdb::crdt_sync::crdt_sync_global(0);
    k_ai::fl_trainer::mesh_tick_global(0);

    // Diagnóstico FL/CRDT (throttle ~500 ticks do TIMER — SESSION_235).
    static LAST_FL_LOG: AtomicU64 = AtomicU64::new(0);
    let fl_now = k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
    let fl_last = LAST_FL_LOG.load(Ordering::Relaxed);
    if fl_last == 0 || fl_now.wrapping_sub(fl_last) >= 500 {
        LAST_FL_LOG.store(fl_now, Ordering::Relaxed);
        let (r, gr, nw) = k_ai::fl_trainer::fl_stats_global();
        let (cv, peers) = k_ai::sgdb::crdt_sync::crdt_stats_global();
        k_nano::slog_bin!(
            "FL", "ok",
            "fl round={} global={} grads={} | crdt v={} peers={}",
            r, gr, nw, cv, peers
        );
    }

    // Phase 4: MESH_HEALTH — periódico + flush se peer subiu/caiu (grafo dinâmico).
    k_nano::net::mesh::flush_mesh_health_if_dirty();
    static LAST_MESH_HEALTH: AtomicU64 = AtomicU64::new(0);
    let mh_now = k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64;
    let mh_last = LAST_MESH_HEALTH.load(Ordering::Relaxed);
    if mh_last == 0 || mh_now.wrapping_sub(mh_last) >= 110 {
        LAST_MESH_HEALTH.store(mh_now, Ordering::Relaxed);
        k_nano::net::mesh::publish_mesh_health();
    }

    // SESSION_237 (ADR-0081 C2): Worker com peer anuncia os experts locais ao
    // Master 1x (static flag — o EventBus do Master segura o pacote até ele
    // processar, mesmo que a eleição ainda esteja em andamento).
    static DISTRIB_BROADCAST_DONE: AtomicBool = AtomicBool::new(false);
    if !DISTRIB_BROADCAST_DONE.load(Ordering::Relaxed)
        && k_nano::net::mesh::local_role() == k_nano::net::mesh::NodeRole::Worker
        && k_nano::net::mesh::MESH_ENGINE.lock().as_ref().map_or(false, |e| e.node_count() >= 1)
    {
        // Honesty: só sticky após TX; falha permite retry no próximo tick.
        if cortex::mesh_distrib::broadcast_local_experts() {
            DISTRIB_BROADCAST_DONE.store(true, Ordering::Relaxed);
        }
    }
}

/// Submete o job de mesh/FL/CRDT para os APs. Retorna true se enfileirado.
pub fn submit_mesh_fl_crdt_tick() -> bool {
    k_nano::smp::ap_work::enqueue(mesh_fl_crdt_tick_job, 0)
}