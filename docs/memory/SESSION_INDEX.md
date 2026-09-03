# SESSION INDEX — neural-os-core v1.9.99-s308 TEST

**Propósito:** Catálogo de sessões. A pasta viva `docs/memory/` mantém `SESSION_107+`; sessões históricas anteriores ficam em `docs/archive/sessions/`.

---

## Sessões Mantidas (107+)

| Sessão | Sprint | Bloco | Título | Principais Descobertas |
|---|---|---|---|---|
| 309 | Falcon3 GGUF | GGUF/cortex | Falcon3 inferência GGUF | TQ2_0 (type 25) + BF16; type IDs corretos (spec padrão); metadata value IDs corrigidos (6=FLOAT32, 8=STRING, 9=ARRAY, 10=FLOAT64); GgufBackedModel auto-config; GGUF magic scan no boot; QEMU 4c GGUF LOADED→CURRENT_MODEL + greeting 40s |
| 308 | SMP AIOS | ADR-0089 | Anti-churn + Memory N≥5 | should_redistribute; inflight; IPI 0→1; steal_burst half∩4; Net ring3; stats/64; HUD 32 |
| 307 | SMP AIOS | ADR-0088/0089 | N-cores roles + runqueue | MAX_CORES=256 RQ; init_roles_from_pools; smp-runqueue default; affinity ring0/1/2; steal+affinity; online==madt slog |
| 306 | Mesh/P2P | ADR-0081 + Jarbas | Dual QEMU 4c Master/Worker | slog P2P/Net `info`→TRACE mudo; STATIC skip L3.5; netmode @0x13E000000; A Master B Worker+TOFU; Falcon dual ❌ RAM; target1/uefi sync |
| 305 | Boot/QEMU | P6 + Jarbas greeting | QEMU 4c loop PASS | int 0x90 DPL=3 na IDT k_nano; marker P6 @+48 pós-mailbox; ring3_can_iretq=true; saudacao+TTS+desktop_ready TCG |
| 302 | Ring3 | Onda 6 ADR-0102 | CPL=3 sandbox wired | k_nano::ring3 mailbox+T-056; P6 demos reais; register_native_ring gated T-053; 0 erros check |
| 301 | Boot | #PF Fix | Kernel virtual range detection | Root cause: cr2-HHDM=140PB, correcto é kernel_phys+(cr2-kvirt); 0 #PFs; ATA FAT32 mount OK |
| 299 | Boot | Audit + ATA TCG fix + #PF fix | ATA probe em TCG habilitado; slog visibility; demand-page dual-range |
