# STATE — neural-os-core v1.9.99-s308 — SMP anti-churn + papéis (ADR-0088/0089)
#   SESSION_308: should_redistribute; inflight; IPI 0→1; Memory N≥5; Net ring3;
#   steal_burst half∩4; stats/64; HUD 32. Host 28 tests PASS.
#   QEMU 4c: roles worker=1 memory=0; runqueue 2× (anti-churn). cargo check 0 erros.
#   SESSION_307: init_roles_from_pools(N); MAX_CORES=256 RQ; smp-runqueue default ON.
