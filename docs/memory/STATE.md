# STATE — neural-os-core v1.9.99-s328 — Full Infer D+B+C (Falcon3 off BSP)

#   PISTA ATIVA: InferQueue WS-H — UI viva durante generate
#   SESSION_328: CortexAgent submit-only; InferWorker+AP poll_slice; stream+TTS parcial
#   Decisão residual s328: NÃO GPU/NPU no job agora (WS-D/E Layer S); NÃO
#     agent_tick_offload_safe; Prefill AirLLM = sprint seguinte candidata;
#     CPU W2A8 ≠ GPU W2A8 (SESSION_328 § Residual)
#   Próximo: aceite metal → Prefill yield → métricas/budget TSC → Layer S
#     (SESSION_328 § Roadmap)
#   PISTA ANTERIOR: freeze HW real @ tick 1370 network_agent + MSC CCS=0 (SESSION_316/s327)
#   SESSION_316: escada de instrumentos FB (s318→s327) — ver docs/memory/SESSION_316.md
#   Fixes s327: HDA DMA pool PMM, cap poll ZERO-delay, watchdog TRINITY, MSC PP=1 RMW
#   PISTA ANTERIOR: k_nano microkernel slimming (ADR-0103 FASE A-H) — SESSION_321
#   SESSION_321: FASE A-H analysis complete — 7 commits, 4 facades created
#   k_nano: 87 modules, ~25.5k LOC (was 88/~27k)
#   k_hal: 22 modules (was 21) — NIC + FS + Storage facades added
#   FASE G+H: analisados, nao migrados (dependencias k_nano + risco)
#   SESSION_319: Emotion unification + Soul delegation + SER→Affect + LoopPhase→Display
#   SESSION_318: FASE 1-4 complete — KvCache, MoE, Emotion, Soul, MCP, HNSW, dead code
#   SESSION_317: ReActLoop + H2O eviction + CodebookVQ + dead code removal
#   FREEZE até aceite: Ring3 Onda 6, 0103 S2–S6, Falcon3 sprint, 0089
#   HW: target/usb_hw.img 6271MB READY (PACK_LLM=falcon3, kernel sha256=8c7a49179a4d5d6e, pós-s328)
#     FALCON3.V6 pack ~2045MB (target1 FALCON3.BIN path; stub 0B V6 ignorado por find_large)
#   hermes: Emotion canonical + Soul canonical + VOICE_EMOTION + LoopPhase + HNSW boot
#   jarbas: SoulProfile delegates hermes; TTS InferQ partial (INFER_TTS_PARTIAL)
#   cortex: InferQueue + Persistent KvCache + H2O + MoE 0.05 + Federated health
#   hermes: Emotion→Affect + Soul→LLM + MCP dynamic + HNSW search + 17 dead removed
#   Self-Heal: 5/5 detectores wired; LLM loop via InferQueue; user notify; NSGDB memory
#   DOCS s329: TECNOLOGIAS/README/TODO/INDEX reconciliados c/ tree (148K LOC, 41 nativos,
#     Falcon3 22L/3072, testes 784/6); ADR legados → archive/notes; licença AGPL vs MIT pendente
#   METAL s330: freeze UI = deadlock SKILL_STORAGE (fix c174c504); timer morto = x2APIC sem
#     read-back (36c9d1a6); lentidão ~1Hz = calibração LAPIC + ADR-0104 (TimerCap R1, rails/dwell/HITL).
#     USB/BOOT.LOG no metal = placeholder; "probe nao chegou"/"skip models (no MSC)" — ABERTO
#   UI s331: orb v2 (gate wall-clock 30fps, OrbState enum, paleta rails, halo SWAR sem
#     divisão, ring_spans LUT, LOD; bench 112-130µs/frame) + FFT Goertzel (tap cap 1024)
#     + Hub Health panel agent-driven (HubHealthAgent hermes decide; compositor desenha;
#     dados medidos, n/a ≠ 0; F12/orb click/badge). USB metal ABERTO (diag FB pronto p/ ler)
#   UI s332-s335: fluidez (TARGET_FPS 60, DamageList heapless, chrome cache, SSE2) +
#     FB WC/PAT/movnti + cursor HW + BCS (gated, default-OFF; validacao METAL pendente) +
#     invalidacao de janelas/cards; testes 845 pass/0 fail; imagem regenerada
#   UI s336: fix orb (intrinsics SSE2 mal-compilados no soft-float -> copy_nonoverlapping);
#     QEMU verificado: orb ciano com G intacto, frame cost 18ms, animacao viva
#   Não declarar v2.0.0
