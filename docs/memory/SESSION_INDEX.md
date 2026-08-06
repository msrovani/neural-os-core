# SESSION INDEX — neural-os-core v1.9.5 TEST

**Propósito:** Catálogo de sessões. A pasta viva `docs/memory/` mantém `SESSION_107+`; sessões históricas anteriores ficam em `docs/archive/sessions/`.

---

## Sessões Mantidas (107+)

| Sessão | Sprint | Bloco | Título | Principais Descobertas |
| 252 | ADR-0086 | Instalação + Update OTA unificados | Processo canônico (consolida 0079/0031§1/0074) + execução completa (10 gaps) | ADR-0086 Accepted: consolida instalação (ADR-0079 + plan, deprecados) + update (ADR-0031 §1, deprecado) + ADR-0074 (lacuna: sem arquivo, só codemap/`git_thin.rs`). **10 gaps fechados em 8 commits, 0 erros:** U2 (shell `update`), U6 (filtro ESP 0xEF + UPDATE.CFG na ESP), U1 (`switch_slot` promove slot→kernel.elf — zero mudança no Limine), U4 (rollback com guarda tries + BootSelfHeal em PANIC), I9 (`boot_mode::mode()` — CONFIG.TXT + NeuralFS 0x7F), I6 (AutoInstallerAgent registrado + shell `install`), I4 (ModelProvisioner: slots vazios via UPDATE.CFG), I5 (leitura NeuralFS no boot + persist /models/), I10 (`SELF.STATE` na SGDB + record_life_event — autobiografia), I11 (telemetria: POST /api/logs + do_POST no serve_update.py), I7 (VRAM real via tamanho de BAR0), I8 (self_check real via resolve_path+CRC32C), I12 (`build_image.py --mini`: PACK_LLM=none + MODELS_SOURCE=network). Limpeza: stub morto `CHANNEL_MANIFEST_URL`/poll_channel removido (URL do server só no config). **Lições:** hardcoded de ambiente (IP) ≠ contrato de layout (nomes 8.3/limine) — config file é dado; "resolver gap" = escolher o alvo mais simples (ESP FAT32 em vez de NeuralFS); agente que só loga o evento = dívida; stub morto = apagar, não configurar. U3 (Ed25519/TPM) = defer (hardening, FNV-1a cobre integridade). **Continuação (NeuralFS F1-F16 + compatibilidade):** revisão profunda do FS (oracle) — F1 CRÍTICO alocador contíguo (free-stack LIFO corrompia re-escrita), F2 ordem CoW (dados→commit→reclaim), F3 mount seguro (probe backup, nunca formata volume existente), F5 journal corrompido recusa mount, F6 format zera journal, F8 `read_range` (AirLLM streaming), F10 valid_name, F12 dead code (extent/checksum_tree removidos), F13/F15/F14, F16 flush barrier (LiberFS). Licença BAFS corrigida MIT→GPL-3.0 (lib-1: repo `cl8dep/bazzulto-bafs` congelado v1.2). **Compatibilidade NeuralFS/MHI/SGDB (C1-C10):** C1 CRÍTICO TickvLite gravava LBA 2048 (colidia com ESP+NeuralFS — brick NVMe real) → região fim do disco; C2 log RAM volátil; C4 episodic tail O(n) removido (fonte única doc); C9 ponte provision↔SGDB (pkg/model meta). Pendências: C6 ArcCache morto, C5 MHI hinting-only, C7 tiers, C8 rebuild. Commits f07834f + 6a8f379. |
| 251 | ADR Tier 0+1 | 0041/0083a/0045/0082 + fix boot | Fila ADR por complexidade (1–4) + fix raiz reboot loop IST | 4/4 itens: 0041 aceite (evidência `docs/evidence/boot-whpx-20260805.txt`: QUEUE_NOTIFY NotifySent, h5_demo Allow/Deny/Deny, AS restore CR3 OK, P2–P9 OK, 57 agents + Runtime); 0083a warn honesto fallback LCG em `init_router_weights` + ROUTER.BITNET no FAT confirmado; 0045 = cutover JÁ feito em e51a48b (docs/bridge reconciliados — truth=jarbas, residuals: soft-float/VITS, UAC AWAITING_HW, dedup HDA pendente); 0082 Onda CPU (`ns::HW` + `populate_hw_namespace` em store.rs, tick ADR). 🔴 **Fix raiz do reboot loop do commit 2662d50**: GDT usava `&TSS_ARRAY[0]` cru (ISTs zerados) e o lazy_static `TSS` nunca era dereferenciado → entrega #PF/#GP/timer faz push para VA 0 → #DF → triple (sintoma QEMU `CR2=0xfffffffffffffff8`). Fix: `Descriptor::tss_segment(&*TSS)`. + checks HUGE_PAGE e3/e2 no `map_page_direct` (SESSION_250 §4). |
| 250 | AIOS | RAM → HMI → auto-adaptação + Boot 2B | Heap self-adapting + AirLLM gate + wrap 2⁶⁴ | Premissa AIOS implementada: `heap_initial_mb = clamp(75% RAM detectada, 512..1536)` + `grow_bump_auto` (auto-grow sob demanda com verificação `heap_pte_present`), eliminando `resize_bump_heap(2048)` hardcoded; `needs_airllm()` gate em model_fit; HMI já expõe RAM (SysInfoAgent 9001). 2B v6 convertido (792MB canônico, encoder Q6_K vetorizado 0.012s vs horas); scan autodescritivo `v6_file_size` (era const v4 604MB truncando o v6). 🔴 **Wrap 2⁶⁴ no bump heap**: HEAP_BUFFER @ high-half, `heap_start+offset` envolve em ~2044MB → o 2B (offset 2158MB) escreve em VA 0 → #PF CR2=0 no memcpy. Fix HEAP_EXT_BASE falhou (map_page_direct sem check HUGE_PAGE) → revertido, known-issue (check HUGE_PAGE em todos os níveis = fix real). |
| 249 | ADR-0084/85 | Engine BitNet + .bitnet v6 | Formato canônico v6 + fidelidade 2B4T (F0–F6b + F1b) | Implementação completa: F0 writer canônico `bitnet_writer.py` + `save_model_v6` + parity byte-exact (`v6_writer_parity` PASS); F1+F1b 8 conversores → v6 (incl. silu no forward de treino ADR-0085 §10.3); F2 loader v6 estrito + fallback legado WARN; F3/F4 kernels (unpack branchless `(pair&1)-(pair>>1)`, activation-parallel gated m≥8, tiling consts); F5 fidelidade M1–M4 (act_type relu2/silu nos 4 forwards, eps 1e-5, rms_ffn_norm intermediate, theta header, embed Q6_K encoder+loader+lookup+unembed) + `bitnet_fwd_parity.py` fortalecido; F6 `cortex::model` ModelView + `ModelHub::register_bytes` + call sites LLM do main.rs roteados. 🔴 **Bug latente crítico corrigido:** `f16_to_f32` (gguf.rs) tinha `sign=(bit>>15)* -1.0` → todo f16 positivo virava **-0.0**, quebrando todos os dequants GGUF. Testes: 18 cortex PASS (parity, Q6_K cross-check Rust↔Python, round-trip v6 host). 0 erros workspace. Pendentes por design: boot QEMU v6 (precisa download 2B), F7 W2A8 (gated WHPX/HW real), retreino TinyStories/RustCoder. |
| 248 | Verificação | HW Expert v4 — Veredito Arquitetura | 12 lanes: NN não identifica hardware além da tabela (teto de sinal 59-63%) | Transformer ternário colapsa (60.67% ≈ majoritário). Controle decisivo: mesmo arch fp32 = 60.58% → ARQUITETURA é a vilã (atenção truncada q_dim=32 + mean pool), não a quantização (QAT não é o fix). MLP 202KB no alvo vendor: plain 39.71% específico, inv-freq 58.97%, stage-2 sem imbalance 63.27% → teto = SINAL (vid:did → família de driver específica ~59-63%, nomes pci.ids cobrem 54.7%). Veredito: kernel = tabela (100% conhecidos) + heurística class byte; NN gated off (`ea696c3`). Reivindicação "260KB NN ≥ DB 40MB" refutada pela medição. Infraestrutura entregue: sweep QEMU multi-device, validator Rust-exato, split honesto, controle contínuo, MLP probes, relabel v2/v3 (ground-truth independente). Protocolo de re-habilitação: provar ≥65% específico no protocolo honesto. |
| 247 | Auditoria | HW Expert v4 + ADR-0084 + CI + testes host | Artefato v4 degenerado → retreino validado; ADR-0084 Engine BitNet; CI; cargo test host | Artefato `hw_expert_v4.bitnet` era 100% zeros (export threshold 0.5 vs init ±1/√128≈±0.088 → tudo 0; H2 CONFIRMADO — kernel sem bug). `tools/retrain_hw_expert_v4.py`: split honesto 90/10 por (vid,did) seed 42, early stopping, threshold de export tunável, embed ROW-MAJOR (não .T); `validate_hw_expert_v4.py`: port Rust-exact (parse_end, fração não-zero ≥1% GATE, predições não-constantes, holdout do ARQUIVO). Loader v5: formato export_v4 = num_params u32 + prefixo `u32 len + u32 scale`, scale vestigial→1.0. SSE tail clamp (heads 17/9/10 — n%4≠0). build_card: tabela curada SEMPRE vence o ML. cargo test host habilitado (139 testes): gate `#[cfg(target_os="none")]` (não cfg(test) — inerte em dep), IDT cfg(not(windows)), p2p_sim gated feature, NVMe layout 72B pinado (spec 64B, AWAITING_HW), quarantine padrões lowercase (dead code), FedYogi teste honesto. CI workflow (check+test+build+boot smoke Phase 6+tick). ADR-0084 (Proposed): M1 relu2, M2 SubNorms, M3 theta 500000, M4 embed Q6_K; F1 decode branchless → F2 activation-parallel → F3 fidelity+Q6_K → F4 W2A8 gated; receita 1-bit (tanh 30×, LR cooldown, QAT suave). Scrub: README sem superlativos + DCO no CONTRIBUTING. |
| 246 | Auditoria | Gap camada IA | Auditoria 7.x: camada neural era ruído + correções (ADR-0083) | 7.2 router MoE = LCG seed=42; 7.3 treino = regressão scalar sem backprop (inputs=targets=1.0); 7.4 memória = FNV-1a; 7.5 saudação = pool canado. Fixes: `load_router_from_file()` (ROUTER.BITNET, format 20B header + tensor 64B name, avanço n_orig*4+n_quant), log honesto trained/deterministic_fallback; `TransformerTrainer` backprop real (CE 2.7018→1.7487 self-test PASS boot QEMU T+815); saudação argmax real (constrained decode mantido p/ clima); `tools/train_router.py` 93.5% acc exporta ROUTER.BITNET; FAT 8.3 trunca ("ROUTER.BIT"). cargo clean revelou erros xHCI (ISOC_SLOTS cast, use-after-move UVC ring → configure_uvc_endpoint Option<()>). |
| 238 | ADR-0081 | Segurança Fase A | TOFU + fail-closed + anti-replay + FRAG MTU (commit e56e5d4/916d155) | RX era fail-open (verify com chave local → fallback aceitava tudo). Fase A: RX fail-closed (sem assinatura/pk vinculada → DROP), TOFU `PEER_KEYS[16]` via `PK\0`+pk no heartbeat (seam SKYNET), anti-replay clock, todos TX assinam — QEMU dual `sec: unsigned=0 badsig=0 replay=0`. Veredicto: NÃO implementar BitTorrent (3-5k LOC; HTTP Range+broadcast cobrem; uTP patenteado até 2027); só merkle piece verification futuro. `FRAG\0` header 21B: fragmentação pós-sign, reassembly pré-verify (2 slots, fora-de-ordem, bitmask seen[8], timeout 500 ticks); gate 1200B removido — matmul 64×64 ~17.5KB em 18 frags OK. |
| 237 | Jcode | Memória 4-tier | Integração memória jcode-inspired + fix xHCI #PF (TCG) | 6 linhas jcode implementadas: `k_ai::tiers::consolidate_tiers` L1→L5 (SleepCycle CONSOLIDATE), gated RAG + blacklist 10 patterns, skills por embedding `skill:<name>` (sim≥0.4), `TOPIC_CHANGE` em 5 pontos de mutação, F5 promote wasm real. BGE statics duplicados: bin `pub use k_ai::memory_systems::*` (recall 384d real). 🔴 xHCI #PF sob TCG: `set_page_uc` só seta UC em mapeamento existente; fix loop `map_page_uc` 16 páginas do BAR (padrão e1000). WHPX quebrado nesta máquina; retry AP-wake TCG resolve. |
| 236 | Codemap | Index do repo | Codemap: 66 mapas + atlas raiz | Skill `codemap` indexou 738 arquivos/67 dirs → 66 codemap.md + atlas raiz + seção Repository Map no AGENTS.md (linha 414); `.slim/codemap.json` change-detection. Drifts docs vs código: `probe_uefi_framebuffer` removido (→`probe_raw_framebuffer`, limine_boot.rs:155); jarbas/audio = fonte única (truth antiga stale); neural-kernel src/{fs,vfs,neural_fs} = espelhos NÃO compilados (`pub use hermes`); update_tecnologias.py inexistente; claim bios.img stale (Limine). Corrigido `MpmcQueue`. cargo check 0 erros (29 warnings). |
| 234 | ADR-0081 | Mesh P2P k_nano | P2P Mesh real entre QEMUs + migração transporte→k_nano | Duas instâncias AIOS (10.0.3.2/.3) descobriram-se e trocaram skills via e1000 + UDP broadcast 42069 (Master push 15 skills, Worker apply). Migração commit 0eec18f (16 arquivos): `udp_broadcast.rs`+`mesh.rs` p2p_tick→k_nano R0 (~100 LOC, sem deps novas); non-heartbeat→EventBus `P2P_PACKET`; bin `pub use k_nano::nic_globals::*`; `set_nic_config` só pós-config (driver-init envia heartbeat em sandbox). Script: ASCII puro, `$Root=$PSScriptRoot`, OVMF 8.3 path, `-NoDisk`, -smp 2 MTTCG. `nodes=1` persiste (node_id=local_role colide; next: derivar do MAC/IP). `hermes::net` statics = espelhos mortos (limpeza pendente). |
| 228 | HW boot | Primeiro boot real | HW Real boot + ESP GPT + Mouse PS/2 Fix | Primeiro boot em HW real (notebook, pendrive unificado Limine UEFI) até interface Jarbas. `mk_esp_fat.py` MBR→GPT completo (protective 0xEE + EFI PART + backup) — Limine exige GPT, não MBR. MouseAgent: `ps2_check_exists()` (self-test 0xAA→0x55, timeout 5K loops vs 100K) antes de `enable_ps2_mouse()` — 8042 ausente em notebook moderno travava boot. SMP bare-metal: allow_smp hypervisor=None, MADT→BOOT_APIC_IDS, trampoline retry 3×. BOOT.LOG não escrito (xHCI enumeração tardia). Desafios: trackpad I2C-HID sem driver. |
| 225 | Limine | Migração higher-half | Limine Migration + HHDM + Desktop na tela + Soft Power Off | Migração bootloader 0.11→Limine 6.x (higher-half 0xffffffff80000000+); `PHYS_MEM_OFFSET.store` no início de kernel_boot (main.rs:1268) antes de drivers. Fixes: P6 raw_vec overflow (TRY_ENTER_RING3=false); e1000 RX #PF loop (guard `pmoff==0` em recv/any_rx_dd); BPE scan bound 0x200000000→0x180000000 (RAM 6GB); NeuralFS CRC32C sem zerar campo CRC (fix nas 2 cópias k_nano/hermes); WHPX PIT skip via CPUID 0x40000000; NTP/TLS sandbox skip; Power menu 3 opções→ACPI PM1a_CNT. Boot T+107 FS, T+333 Desktop, 55 agentes, 7+ modelos. |
| 224 | ADR-0076 | Cross-OS Ecosystem | Implementação Pesada — 23 entregas + rename JARVIS→JARBAS | 23 entregas ADR-0076 (v1.9.99-adr0076): Skill Manifest FYY (25 manifests), WASM host fns 1→6 + WASI P1 + 18 WAT tests, telemetry SPSC 4096, membrane+permission HITL, syscalls 13→9, SYS_MAP_FB real, proof-gated mutations (ruvix-proof), kernel HNSW (ruvix-vecgraph + patches no_std), Ring-3 ELF loader + ProcessManager + TRY_ENTER_RING3=true, rename JARVIS→JARBAS (16 arquivos). Lições: fixers paralelos sobrescrevem `pub mod` no lib.rs; ruvix-vecgraph precisa patch no_std; `AgentTickResult::Continue` não existe (usar Pending); PS `-replace` é regex. |
| 215 | ADR-0075 | Emagrecer bin | Análise profunda + plano cirúrgico (v1.9.10-emagrecer-plan) | Bin ~29.431 LOC, ~12.000 (41%) bin_ahead (cortex, agents, neural_fs, bpe, gguf...) — canônicos já nos crates. Correções vs dashboard: main.rs=3.102 LOC, net+netstack=1.774; alvo realista pós-E4 ≈11.000 LOC (não 3-5k). Sequência E0 freeze (PR policy >200 LOC + diff_bin_crate.py --strict) → E1a/E1b/E1c promover cortex/bpe/gguf, agents/neural_fs/vfs/fs, boot_logger/virtio_net/usb_msc → E2 ADR-0062 Limine → E3 wire → E4 audio→jarbas (~2.900 LOC). |
| 177 | pós-v1.9.9 | F-series ROI | ROI × viabilidade bare-metal (skip 7-9) | Smoke e2e L1→ckpt→remount→get (`k_ai::sgdb::memory_checkpoint_e2e_smoke`); SleepCycle 1 impl (`hermes::agents::SleepCycleAgent`); pseudo-emb sem BGE (`embed_or_pseudo`); rescore FP32 top-k BQ (`bq+fp32`); `.cursor/plans` removido do tree. Skips honestos: TicKV/NoProto/HNSW 10M. Gate `cargo check --release -p k_ai -p hermes -p neural-kernel --features fat-boot-log`=0; commit 2282e15; `gh release` pendente (CLI ausente). |
| 165 | ADR-0059 | Runtime App Factory F3-F7 | F3-F7 + Cleanup (v1.9.5 TEST) | `WasmExecutor` (~150 LOC) removido; `wasm.rs`→`wasmi_rt::run_wasm`; `VersionEntry.bytecode` Vec<Op>→Vec<u8>; `evolve.rs` hot-swap/rollback rewired com `DynamicSkill::with_wasm()`. F4 `decode_harness.rs` (reconhece→gera→valida, self-test add(3,5)=8 PASS); F5 promote completo; F6 MicroPython.wasm via wasmi + fallback; F7 ring gate: só A executa (`isolation_ring_available()=false`). `PackageSignature` (xorshift) em package_hub. Lição: export section WASM exige byte kind+LEB128 após nome. WAT assembler full postergado (PONYTAIL, aguarda `wat` no_std). |
| 245 | Auditoria | Segurança 6.1–6.4 | Modelo de confiança unificado — portão único ADR-0052 + docs honestos + token fail-closed + RDRAND | verify_skill_md agora DELEGA para verify_artifact_md(PackageKind::Skill) (schema/kind/7 seções/content_hash/Ed25519) — fim do portão fraco da auto-evolução. verify_and_register = sign-first → verify estrito → register (fail-closed). Generators emitem contrato completo; seeds embedded via register_trusted_skill (SESSION_230). AGENTS.md: anéis R0–R3 = organização de código, NÃO fronteira do processador. CapabilityToken::Ed25519 => false (sem mensagem vinculada). mix_session_seed usa hw_rng RDRAND (gate probe_done&&rdrand). cargo check 0 erros. Commit separado (sessão concorrente em main.rs NVMe/AHCI/ATA/USB-MSC fora). |
| 244 | Consolidação | NeuralFS fonte única | NeuralFS triplicata → fonte única em k_nano + guarda check_duplication | NeuralFS existia em 3 cópias (k_nano morto 294L / hermes avançado 686L / bin vivo 653L); consolidado: agent vivo do bin movido p/ k_nano (crate::slog_bin!, crate::globals::USB_MSC, impl FilesystemAgent→pub fn inerentes pois trait é ring-local), hermes+bin viram facades `pub use k_nano::neural_fs::*` + adapter `impl FilesystemAgent for k_nano::NeuralFsAgent` (orphan OK), 15 arquivos deletados. Novo `tools/check_duplication.py` (exit 1 se .rs não-facade em ≥2 crates) — resolve "nada avisa". cargo check --release 0 erros. Dívida restante visível: camada fs/, net, espelhos cortex/k_ai. |
| 243 | ADR-0082 | Ring3 Isolation Production (F1–F4) | Isolamento Ring3/SFI de produção: create_sandbox_as (kernel supervisor-only), TSS_ARRAY per-process, SYSCALL/SYSRET gated por hypervisor real (WHPX rejeita wrmsr LSTAR/STAR/FMASK → #GP; probe_done() + fallback int 0x90), ELF loader (RX/RW por segmento + relocations RELATIVE), ring3_run_native() dual path, arena W^X USER (escrita via HHDM — VA sandbox ∉ CR3 kernel → #PF), app_factory B/C gated. Boot TCG 2c 8G -NoDisk: P6 Ring3 OK, ELF+USER arena PASS, P7/P8/P9 OK, 54 agents, WASMI A. Commits 8d3eb90/1450108/6b073bf/4c7a2e9. 0 erros. |
| 242 | ADR-0081 | Phase 2 Complete | Mesh P2P Reliability: ACK seletivo + backoff + health TTL + capacity scoring + token bucket + JSON dashboard | REASSEMBLY 2→16 slots + FRAG\0→FRACK\0 stop-and-wait (3 retries); probe_node exponential backoff 50→3200 ticks; cleanup_peer_health_ttl (>60s); PeerHealth expandido (avg_rtt EWMA, rtt_samples[32], peer_p99_rtt via aritmética inteira no_std); ARP cache PEER_MAC_CACHE + recv_*_with_mac; capacity_weighted_assign health-aware (unreachable→0); token bucket rate limiting (1/tick, burst 20); publish_mesh_health JSON array no tópico MESH_HEALTH + mesh_health_json::parse no_std no Jarbas + cards coloridos. Commit 7a97556. cargo check k-nano/cortex/jarbas 0 erros. |
| 241 | TLS | Bridge Fix | hermes→kernel wiring | `hermes::tls` era dead code — `register_https_get()` nunca chamado, consumers HTTP-only, fallback `http://host:443/path` bug. Fix: `fetch_url()` dispatcher único (https→kernel TLS, http→net_bridge), bridge wire no Phase 7, 11 consumers roteados. Ver SESSION_241. 0 erros. |
| 240 | ADR-0081 | Fase B gate L/F | Tier cripto Relativizado (HMAC) vs Full (Ed25519) | Decisão maintainer: mesmo range/datacenter relativiza cripto dos DADOS (HMAC-SHA256 + chave segmento, ~1.3µs/pacote) vs externo full. Custo: Ed25519 verify ~26-46µs (fixo, ~0.3Gbps/core) domina ~2 ordens; HMAC ~8Gbps; datacenter +8-40% RTT visível, WAN invisível — onde dá p/ relativizar o custo é alto. k_nano/crypto.rs (hmac_sha256 RFC 4231 + ct_eq + self-test), mesh.rs SEGMENT_KEY/crypto_tier()/set_segment_key (fail-closed), udp_broadcast sign tiered + authentic, RX controle sempre Ed25519. ADR-0081 Fase B atualizado. 0 erros. |
| 239 | ADR-0081 | Fase C completa | experts + DSD + NodeTier + FL + CRDT | C2 experts ED\0/EDR\0 (capacity_weighted_assign); DSD cortex/speculative.rs; NodeTier L0-L4 score_bonus; C5 FL FD\0/FM\0 FedYogi; C4 CRDT\0 version sync LWW. Padrão TaskType::Inference + assinado + FRAG\0; Fase A intacta. VALIDADO: CRDT publish bilateral + FL stats + matmul 64×64 frag. Commit 866e0e6. |.md (simbolos verificados) + atlas raiz + secao Repository Map no AGENTS.md. .slim/codemap.json change-detection (739 files). Drifts: probe_uefi_framebuffer removido (-> probe_raw_framebuffer); jarbas/audio fonte unica (ADR-0045 stale); neural-kernel src/{fs,vfs,neural_fs} espelhos NAO compilados (pub use hermes); update_tecnologias.py inexistente; migrate_k2chj.py arquivado; claim bios.img stale (Limine). Corrigido MpmcQueue. |
| 235 | ADR-0081 | Mesh apps 1+2+3+4 | Marketplace/PROMOTE/Papéis/compute distribuído | Marketplace 14 skills reais broadcast (throttle TIMER_TICKS). PROMOTE Worker→Master 'PROMOTE\0name\0desc'. Papéis 'ROLE\0target\0role' → set_role (B=Memory). Fix eleição: local [node_id(),0,0,0,0,0] (era MAC → todos Worker). Item 4: cortex feature 'p2p' — matmul distribuído Worker→Master ('MW\0'/'MR\0', gate MTU 1200B, espera síncrona ~200 ticks, Master responde mesmo Undecided) — VALIDADO: B request size=1107 → A resposta → B ok shape=(16,16) primeiro=120.0 (mesh dispatch). Self-test 16×16 + retry 5x (DIAG roda pré-eleição). Commits 9239ac9/e4917c1/50bdf6b/b6ab13b. |
| 231 | — | HW Expert v4 + ADR-0082 | HardwareInfo Registry + HW Expert v4 multi-head + SGDB /hw/pci/ | ADR-0082 criada e implementada. HW Expert v4: 59.905 amostras, 5 heads, 260KB. build_card() tenta ML→tabela→heurística. Boot carrega HWEXPRT4.BIN. xsave removido da gate AVX2. find_child_byte16_sse runtime dispatch. |
| 230 | — | Boot Speed | skip Ed25519 + VFS I/O em seed agents | seed_agent() ~8.5s de boot por Ed25519 signing 41× + VFS I/O 82×. Fix: guard `tier=="native"` skip signing + I/O. Seeds são trusted-by-compilation, não precisam runtime. `crates/hermes/src/package_hub.rs`. |
| 227 | ADR-0079 | Neural AutoInstaller M0-M4 | Installer completo pendrive→HD/SSD/NVMe com IA | SysInstaller reativado + dual partition ESP+NeuralFS + HwProfiler + AutoInstallerAgent + Cortex InstallAdviser + self_check + rollback + hw_change + self_heal_disk + net_fallback + detect_ram_mb() real + format_fat32_esp() + ModelHub compat. 0 erros |
| 217 | — | E1a+P001+Boot+CKPT | Cortex crate promotion + P001 SKILL_REGISTRY + boot path + checkpoint expand | SKILL_REGISTRY shadow fix; Agency fallback; Safety I4 Merkle verify; Checkpoint v2 (CR3+heap+driver hash); env/block_dev drift |
| 216 | — | SGDB Agent (A-026) | Bridge EventBus ↔ SGDB + versionamento skills | SgdbAgent EventDriven: store_version, rollback, list_versions, list_skills, store_skill, recall. 229 LOC, 0 erros |
| 176 | ADR-0063 | SGDB Memory Quality | SleepCycle ckpt + recall L4 + V-flag + ART SIMD | Pós-pesquisa TicKV/NoProto/ART/BQ; AIOS memória e2e |
| 175 | ADR-0063 | SGDB D-series | Hamming + L0/L1 RAM + Tickv ckpt + bench 100k/10k | Aceite D-series; Visão vs Ship; DoD pleno residual |
| 174 | ADR-0063 | SGDB quality jump | Q1–Q5 GC/ART48/BQ/AUD2/View/bench | Aceite intermediário; DoD 10M/100k residual |
| 173 | ADR-0063 | SGDB AIOS adoção | SgdbStore + HANR/Audit/Pkg/Skills | Facade namespaces; memory_store híbrido; persist_backend honesty; FAT=blobs |
| 172 | ADR-0063 | SGDB MVP + ghost vector-db | TickvLite + Hermes RAG + k_ai::sgdb F2–F7. Crate vector-db criada mas nunca integrada → deletada v1.9.11. SGDB real é k_ai::sgdb (ART/BQ/MemoryDoc). | Ondas 0–5; SESSION_173 = adoção consumidores |
| 171 | ADR-0062 | P1+P2+P3+P24a | StorageBus + NVMe I/O + HID kb | Emagreçer disk_agent→k_nano; NVMe qid=1 BlockDevice; policy NVMe>AHCI>ATA; HID bringup multi-porta |
| 219 | ADR-0065 | WM cosmic-like fix compilação jarbas | Corrigidos 74 erros de compilação no crate jarbas | WindowId definido em tiling.rs (antes ausente + circular); notifications.rs criado; AppId pub re-export; WasmSkill(usize); pixel color de tuplas resolvido (fill_rect nativo); drcula (22)→(200,100,180); current_theme() com consts estáticas (fim E0515); borrow conflicts render/toggle_app/spawn_window resolvidos; 14 métodos WM adicionados (register_app, ensure_hermes_overlay, render_window→free fn draw_window_fb, spawn_card, card_click, toggle_app, etc); cargo check 0 errors ✅ |
| 220 | ADR-0065 | FASES 1.1/1.2/2.1/2.2/3.1/3.2 COMPLETE | WM cosmic-like + BlitBackend GPU 2D + intel_display HW + P13 APs-IDT + P16 Async | FASE 1.1: decorations.rs (title bar 28px, [×][□][─], rounded corners, hit_test), notifications.rs (Urgency enum, EventBus NOTIFICATION_ACTION), Window unificação (AppWindow/CardWindow/CardClick removidos, Window+WindowContent unificado). FASE 1.2: compositor draw_window_fb modularizado → decorations::draw_window_decorations. FASE 2.1: blit.rs (BlitEngine Cpu/IntelBcs, blit_2d, cpu_blit, fill_rect_2d, run_blit_canary 64×64 gradient), CapToken::GpuBlitReady=15, init_blit+canary no backend.rs. FASE 2.2: intel_display.rs (305 LOC, page_flip_hw DSPSURF, cursor_set_hw/move/disable CURBASE+CURPOS+CURCNTR 64x64 ARGB, run_page_flip_canary + run_cursor_canary via k_nano::memory::GLOBAL_ALLOCATOR). FASE 3.1: TSS_ARRAY[8] pré-alocada no GDT lazy_static, tss_selectors[8], init_ap_tss(ap_index, ist_tops), ap_load_idt_and_tss (lidt+ltr+sti), PerCpu tss_ptr+ist_stacks, init_ap_ist (3×16KB #DF/#PF/#GP), ap_entry: init_ap_ist→init_ap_tss→ap_load_idt_and_tss→AP_IDT_READY barrier→set_ap_pollable(true). FASE 3.2: TimerFuture (ticks_remaining AtomicU64, poll decrement+wake), init_async_rt registra demo 100 ticks, process_wakes do timer IRQ. Heap 2GB→4GB (BitNet 1.3B/2B). Commits 289339c + 0fdf20e. Tags adr0065-fase1-3-complete + adr0065-fase2.2-3.2-complete. cargo check 0 errors ✅ |
| 222 | v1.9.12 | Power Management Completo | cpufreq (P-state) + MWAIT (C-state) + S3 Suspend/Resume | cpufreq.rs MSR IA32_PERF_CTL/STATUS/ENERGY_PERF_BIAS + governor Performance/Powersave/Ondemand + APERF/MPERF actual_ratio; MWAIT real no AP idle loop (monitor/mwait); MONITOR_FLAG wake no enqueue; S3 entry ACPI (SLP_TYP=3) + FACS wake vector (0x7000) + trampoline 64-bit restore CR3/RSP → s3_resume_entry (APIC/PIT/EPB reinit); save_e1000/restore_e1000 (16 regs + MTA); ondemand tick no scheduler halt closure; S3 resume handler reinit core. 10 arquivos modificados, 0 erros. |
| 221 | BitNet Recommendations | Todas as recomendações implementadas | soft_stride=1, max_gen 32/24, BPE encode merge-order, ADR-0061 SSE4.2 dispatch, MPMC queue, BudgetManager, Cellular SleepCycle | soft_stride=3→1 (cortex.rs); max_gen 8→32/6→24 (cortex.rs); weather_step_candidates + weather_bigram_bias relaxados (bpe.rs); encode_merge_order() com merge-order iterativo (bpe.rs); bitnet_sse.rs com SSE4.2 + dispatch dinâmico AVX-512→AVX2→SSE4.2→scalar; export_bpe_bin.py MRG1 v2 com rank u32; MpmcQueue lock-free (k_nano::p2p::mpmc); BudgetManager com CompressionTier e token tracking (k_ai::economy); Cellular sleep_cycle batch processing (cortex::cellular). Commit 5ea319a, tag bitnet-recommendations-v1. Build 0 errors. |
| 218 | ADR-0062 | P31/P18/P27/P7/P16/P14/P35 | Notifications toast, NTP resync, Virtual consoles F1-F6, i225 NIC, Async executor, IPC MessageBus wire, fw_cfg file I/O | Toast overlay fade-out; NTP periodic resync + server rotation; 6 console buffers + Ctrl+Alt+Fn; i225 raw ptrs + kick_rx/prove_rx; std Future + Waker + APIC timer; mailbox_drain no scheduler; fw_cfg read_file/read_by_name/write_file |
| 170 | HW USB | MSC bring-up stick boot | Address Device+BOT p/ BOOT.LOG | Fix Cap regs/CRCR; bringup_boot_msc; ADR-0062 P11 |
| 169 | HW boot | soft-reboot BOOT.LOG | Loop reinício HW pós-JARVIS | `NEURLOG!` sem `NEURDONE` → soft-reboot loop; flush agora retorna; feature soft-reboot OFF |
| 168 | Display | Splash persistente pós-claim_graphics | Tela preta no gap entre `clear_fb_pixels()` e primeiro render do compositor (LLM 6K+ ticks) | Fix: `splash_draw_text()` após clear reusa font 8x16. `DENY MAP_FB` é do P4 demo (Cap::EMPTY), não bloqueia DisplayAgent (escreve direto no FB via write_volatile). 1 file, 14 linhas |
| 167 | ADR-0062 | SGDB TicKV+NoProto+Índices IA | **Proposed** — TicKV NVMe + NoProto Zero-Copy + ART + BQ Flat SIMD | FASE 0: FlashController NVMe; FASE 1: TicKV append/get/GC; FASE 2: NoProto schemas L0-L7; FASE 3: AiosDatabaseEngine ponte; FASE 4: ART Index L0-L3; FASE 5: BQ Flat SIMD L4-L5; FASE 6: Integração L0-L7; FASE 7: SIMD Dispatch; FASE 8: Power-loss + carga |
| 166 | ADR-0060 | BEI BitNet Cognitivo | **7/7 ondas** MPMC+Economia+Células+MoE+Memória+Afeto+Supervisor+Soul Mirror | Onda 6 v2: EgoLayer (EMA, latência), PonderNet (parada sigmoidal, lambda), SupervisorVerdict::Train/PromoteSkill/Ponder; Onda 7: SoulMirrorRenderer (AffectVector→visual), Avatar8State (8 estados), EXECUTIVE_SUPERVISOR global; ~2900 LOC, 11 commits, 0 erros |
| 164 | FitPolicy | llmfit-inspired | #468 host+guest fit | pack_filter FIT_GATE; cortex::model_fit; ModelHub escalate |
| 163 | emagrecer + ADR-0057/0058 | Onda 0–6 + Compute + UI | cutover bin→crates + compute dispatch + card desktop | diff_bin_crate; wake multi-AP (-smp4 APs=3); #412 decode; embedded-graphics cards (close/mover/resize); orb+HUD |
| 162 | LLM | BitNet ladder 850 | #PF AVX2 + BPE SP32/MRG1 | OOB store n%4; encode=HF; coh semântica residual |

| 161 | WiFi | ath10k A3 | BMI→fw_ready | CE0/1+LZ; FW_IMAGE 681KB; PASS só FW_IND pós-DONE |
| 160 | WiFi | ath10k | pivô Note1050 QCA6174 | 168C:003E; FW 1.45MB; BMI/CE scaffold; iwl secondary |
| 159 | WiFi | S0+prepS1 | honesty + DID/FAT | VERDICT AWAITING; fw_resolve=SKIP QEMU; sem ALIVE |
| 158 | TLS | N4 | #123 PKI híbrido | pins+TOFU; smoke root_learn→root_pin; CertVerify residual |
| 157 | TLS | N4 | #123 smoke PASS | google HTTPS 80952 B; WHPX qemu64; soft polyval/aes/sha2 |
| 156 | TLS | N4 | #123 wire https_get | NetTcpIo+KernelRng; VERDICT=WIRED trust=unsecure |
| 155 | TLS | — | #123 probe A PASS | `embedded-tls` 0.19 soft-float ✅ `tools/check-tls` |
| 154 | pesquisa | — | TLS #123 (B) + WiFi firmware/plano | Opções A–D N4; API77 ~7,51 MB; FW-MAC; S0–S5; #408 reclass |
| 153 | release | — | **v1.9.0 TEST** | Pós-LAN + Residuals 0–7; tag v1.9.0; ≠ v2.0.0 |
| 152 | Pós-LAN | — | B-01 fila unlock ondas 0–5 | net_bridge; NetFs PASS; TLS BLOCKED; deadlock NETSTACK pós-L5 fix |
| 151 | planos | — | Fecho Residuals 0–7 | PreFlight pass_marker; LAN✅; WiFi AWAITING; TLS/418 PARTIAL |
| 150 | Onda 7 | — | DNS raw + HTTP L4/L5 | skip_dns_name; OK raw google; L5 301; internet smoke |
| 149 | Onda 7 | — | LAN RX destravado (e1000 TX) | aliases 0x0420 no-op QEMU; TDT=0x3818; L3.5 PASS |
| 148 | Onda 6–7 | — | AirLLM-DMA + NET/WIFI-HW | residuals ATA; lan fila BLOCKED; PreFlight cache |
| 147 | Trilha R | — | soft-float/VITS pesquisa | defer hardfloat; neural-lite permanece |
| 146 | Onda 5 | — | MHI-DMA + GPU-HW AWAITING | Vram hook #67; GDS stub; [GPU-HW] [MHI-DMA] sem fake Ready |
| 145 | Onda 4 | — | USB Trust + UAC-HW | usb_trust/usb.tbl; enforce+disable_port; [UAC-HW] AWAITING; soft-float defer |
| 144 | Onda 3 | — | exFAT write + FS agents fecho | `exfat_write` + EXFAT_WRITE=1; 282e–g ✅; 282h ⏳; #418 BLOCKED |
| 143 | docs | — | Auditoria ideias antigas | STALE→✅; VIABLE→Onda+lan; DEFER/💰/❌; fecho por ID |
| 142 | Ondas 0–1 | — | PreFlight + NeuralFS evidência | preflight_wave.py; depends_on: lan; Onda 7 crônicos; smoke_level2/power_loss; NRFS-HW AWAITING |
| 142 | Models | — | Multi-model hub | TinyStories/3B/850M slots; gguf_wasm SkillMarket; RustCoder 2B/3B FAT |
| 141 | ADR-55 | — | FeatureGate + SMP revision | PlatformProbe; WHPX SMP off; TCG APs=1; RSDP BootInfo; CorePools |
| 140 | ADR-41 | — | H4+/H5+/AS + HalOffer Cap → v1.8.6 | QUEUE_NOTIFY real; Cap grant bind; AS shallow PoC; 1.8.x |
| 139 | HW | — | USB BOOT.LOG + console FB | FAT 0x0C+ESP; BltOnly patch; console_clear |
| 107 | 107 | — | Boot A/B + Cap P0–P9 + ADR-0042 | Runtime QEMU OK; cadeia k-nano→…→jarbas; próximo N1 legível |
| 107_CLOSE | 107 | — | Fecho Voice I/O | 5 loops WHPX; PASS parcial forte+; handoff de gaps para Sprint Sound |
| 108 | 107 | — | N1 ✅ + BitNet 2B LOADED → v1.7.0 | Soft-float/cargo nk; 2B ~590MB L=30 FWD; TTS empty generate; e2e clima PARCIAL |
| 109 | 107 | — | ADR-0045 Sound Voice Stack | Truth=`neural-kernel/audio`; jarbas espelho; sherpa/Vosk/Kokoro/Wyoming/Rustpotter ❌; v1.7.1 docs |
| 110 | 107 | — | Sprint 107 loops 1–5 clima e2e → v1.7.2 | GEN 'O tempo esta'; Piper neural-lite; WakeWord registrado; STT ctc=''; HWEXPERT FAIL |
| 111 | Sound / ADR-42 | — | Handoff voz 107→Sprint Sound; pista limpa ADR-0042 | Docs v1.7.3; 107 Voice ✅ FECHADA; leftovers Sound; N2 próximo |
| 112 | ADR-42 N2 | — | N2 SelfHeal VID+Trust CLOSED | v1.7.4; QEMU `[N2-SELFHEAL]`+`[TRUST]`; N2.5 allocator; pista N3→N5 |
| 113 | ADR-42 N3 | — | N3 cortex LOADED + Trinity CLOSED | v1.7.5; `[N3-CORTEX] criteria=MET`; N3.5 allocator |
| 114 | ADR-42 N4 | — | N4 Hermes orchestrator CLOSED | v1.7.6; `[N4-HERMES] criteria=MET`; N4.6 allocator |
| 115 | ADR-42 N5 | — | N5 jarbas ego/UI CLOSED | v1.7.7; `[N5-JARBAS] criteria=MET`; N5.7 allocator; N1–N5 ✅ |
| 116 | ADR-42 N2.5 | — | k_ai crate wired no bin | v1.7.8; allocator único no bin; espelhos trust/self_heal removidos |
| 117 | ADR-42 N3.5 | — | cortex crate wired no bin | v1.7.9; 9 espelhos removidos; residuals cortex/bpe/global_arena/cortex_mmap |
| 118 | ADR-42 N4.6 | — | hermes crate wired no bin | v1.7.10; 37 espelhos removidos; residuals agents/net*/fs/aios_api |
| 119 | ADR-42 N5.7 | — | jarbas crate wired no bin | v1.7.11; 29 espelhos removidos; audio truth residual ADR-0045 |
| 120 | v1.8.0 | — | Marco K³CHJ pós-jornada | ADR-0042 N1–N5 + wire N2.5→N5.7; docs; tag v1.8.0; pista Sound |
| 121 | 108 | — | Self-Evolving Agents CLOSED | self_evolve engine; verify; SIL wired; SelfEvolveAgent; SleepCycle REFLECT |
| 122 | Sound | — | Sprint Sound CLOSED (parcial honesto) | Mic→Wake gate; STT PCM; UAC parse; neural-lite; VAD/SER; soft-float/VITS aberto |
| 123 | FS | — | NeuralFS I/O usavel (RAM) | B-tree insert/delete; create/read/write; /mnt/neural; smoke OK |
| 124 | ADR-0040 | — | FS MVP aceite (por_fazer→completa) | MHI soft-migrate; exFAT driver; IDEA #417–423; defer #421–423 |
| 125 | ADR-0040 | — | Triagem residuals deferidos | Nenhum viável agora; todos `por_fazer`; ADR MVP intacta |
| 125 | Cortex / ADR-0047 | — | N-gram speculative decoding OK | Cache por posição LWW; verify draft[0]; KV truncate sem double-forward; IDEA #443 |
| 126 | ADR-0047 família | — | LatentBus+Evolve+Probe+GPU/HMI MVP | L1–L3 + G1/G2 + H1/H4; residuals G3–G5/H2–H5 |
| 127 | ADR-0046 | — | AirLLM GGUF streaming MVP | Layer-wise load/forward; soft prefetch; ATA set_model; Net/DMA/K-quant defer |
| 126 | ADR-0047 | — | Família MVP PoC Accepted parcial | LatentBus+Evolve+Probe+GPU G1/G2+HMI H1/H4; IDEA #444–448 |
| 127 | ADR-0047 | — | Wave2 bench/Genesis/G3–G5/H2/H5 | Descartes ISA/adapter/H3; IDEA #449–453 |
| 128 | ADR-0046 | — | Hot-swap Net→FAT→AirLLM | `hot_swap_from_net` + FAT 8.3 write fix; `/model-fetch`; L3.5/RX honest fail |
| 129 | v1.8.5 | — | Consolidação pós-v1.8.0 (teste) | Self-Evolve, Sound, NeuralFS, AirLLM, ADR-0047 e propostas GPU 0048–0050 |
| 130 | fix / polish | — | SkillMarket total_cmp + bpp dinâmico + 0 warn | `wasm_rt::top` NaN-safe; FB bpp do GOP; release 0/0 warnings |
| 131 | HW-PnP | — | HwCapabilityCard + Expert v4 schema | cards EventBus; sem OA5US free-text; train_hw_expert_v4 seed |
| 132 | FS | — | NeuralFS multi-nivel + USB teste + exFAT dados | B-tree 3+; MBR 0x7F USB; mkexfat; VolumeLength@72 |
| 133 | FS | — | Fechar residuals implementáveis | USB format lock; GPT NeuralFS; unified exFAT; boot checksum |
| 134 | ADR-0051 | — | Agency/nativos → AGENT.md | 214+41 manifests (stubs); VFS bridge; seed embutido |
| 135 | ADR-0052 | — | Contrato artefatos + deny stubs | Regra Cursor; validate deny; AGENCY=0; stubs apagados |
| 136 | ADR-0053 | — | HANR parity marketplace+trust | Session Ed25519; market; memory; MCP mínimo |
| 137 | ADR-0053+ | — | Cognitive Bridge superior ao HANR | BGE+Trinity prompt; SOUL≠PERSONA; budget; `/search`; REFLECT nudge |
| 138 | ADR-0048–50 | — | GPU Multivendor Unlock fundação | Caps honestas; NKP; canário; LegacyAcr/Gsp/Gen9/Arc/KiQ; packers host |
| 139 | HW / USB | — | BOOT.LOG + console FB legível | MBR FAT dados+ESP; BltOnly SetMode; heap_ready; console_clear/print; K0–K17 |

## Sessões Históricas e Lacunas

Sessões 079, 080, 081, 082, 083, 089, 093, 106.3 e 106.5 estão preservadas em `docs/archive/sessions/`. Os identificadores **094** e **095** aparecem em registros históricos, mas os arquivos individuais nunca estiveram disponíveis nesta sanitização; o conteúdo está consolidado em `CHANGELOG.md`, no plano arquivado da Sprint 106 e neste índice.

| Sprints | Tópicos Chave |
|---------|---------------|
| 1-13 | Toolchain, VGA, IDT, Memory, SIMD, Tensor, BitLinear, 2-bit Packing, PIC, SMP |
| 19-38 | PCI, ACPI, ATA, FAT12, RTL8139, e1000, Neural Cortex, Transformer, xHCI, Self-Healing |
| 56-68 | FAT32, Bootloader 0.11, DisplayAgent, VFS, GPU Architecture, Auto-Skills, USB-MSC |

---

## Lições Críticas (NÃO REPETIR)

Estes são caminhos já trilhados que terminaram em dead-end ou soluções já encontradas:

1. **Xuvisco no boot (Sprint 71, 79):** UEFI/OVMF não mapeia 0xB8000 → page fault pre-IDT. Solução: VGA sequencer I/O ports (0x3C4/0x3C5) para Screen Off. **Nunca escrever em 0xB8000 antes da IDT.**

2. **AVX2 sob WHPX (Sprint 80):** Instruções VEX/AVX2 causam VM exit (~10k+ ciclos). Scalar é 2x+ rápido. `has_avx2()` deve detectar hypervisor via CPUID 0x40000000 e retornar false.

3. **e1000 TDT bug (Sprint 23-24):** `send()` escrevia REG_TDT = idx (== TDH) → hardware via ring vazio. **TDT = (idx+1) % NUM_DESC.** NUM_DESC RX mínimo = 48 para 82540EM.

3b. **e1000 TX aliases QEMU (SESSION_149):** `TDBAL/TDT` em `0x0420/0x0438` são aliases Intel **não wired** no QEMU → write no-op, `TDT` fica 0, ARP nunca sai, RX=0. **Usar `0x3800/0x3818`.**

3c. **DNS name compression (SESSION_150):** ao pular nomes DNS, **não seguir** pointer `0xC0xx` no offset de continuação — só avançar 2 bytes no wire (`skip_dns_name`). Seguir o pointer corrompe o parse do A record.

3d. **Hermes net espelho (SESSION_152):** FE (Browser/Search/Market) **não** usa `hermes::net` (NETSTACK vazio). **Registrar `net_bridge` no boot** → `neural-kernel::resolve_and_http_get_safe`.

3e. **Deadlock NETSTACK (SESSION_152):** `smoke_if_online`/`tcp_exchange` **dentro** de `NETSTACK.lock()` em `bootstrap_early` → hang pós-L5. Smoke só **após** return do bootstrap.

3f. **HTTPS / TLS soft-float (SESSION_152–157):** **nunca** strip `https://`→:80. Build precisa `polyval_force_soft`+`aes_force_soft`+`sha2/force-soft` (LLVM CLMUL). Smoke: WHPX `-cpu qemu64` (não `host`/APX); TCG soft-AES muito lento.

3g. **iwlwifi ≠ SoftMAC clássico (SESSION_154):** AX200/210 = **FW MAC**; ACK no firmware. Não priorizar Embassy/#408 como SoftMAC ACK ~10µs (ath9k). Plano S0–S5; gap `.pnvm`.

4. **Ramdisk via bootloader (Sprint 79):** FAT partition autosized ~64MB insuficiente para modelos >100MB. **Usar QEMU loader (`-device loader,addr=0x100000000`) para dev, NVMe/FAT32 para HW real.**

5. **QEMU loader com -m 2G (Sprint 79):** Modelo em 512MB conflita com frame allocator do bootloader. **Usar -m 4G+ e addr=0x100000000** (acima de 4GB).

6. **VirtIO-GPU GET_DISPLAY_INFO (Sprint 45):** Resposta 0x0 no QEMU TCG. Bug de emulação. **Framebuffer UEFI é mais confiável.**

7. **FAT12 removido (Sprint 75):** FAT32-only. 102 LOC eliminados. **Novos FS devem ser FAT32+.**

8. **Partition mask 0x1C (Sprint 74):** Hidden FAT32 LBA. Bootloader aceita (mbr_nostd mapeia 0x1C→Fat32). SO não monta. **Usar 0x0C para compatibilidade com outros OS.**

9. **TPM fallback (Sprint 74):** TPM ausente → 0xFFFF FFFF no probe. **Fallback silencioso.** Ed25519 é enforcement primário.

10. **Hermes event-driven (Sprint 76):** ReAct cycle só avança com entrada real. **84 linhas/seg → 0 quando ocioso.**

11. **Sprint 106-1: Cargo workspace (2026-07-13):** Workspace com 5 membros (k_nano, k_ai, cortex, hermes, jarbas) com resolver="2". Isolamento de dependências entre camadas lógicas. **Dependências não devem vazar entre anéis.**

12. **Sprint 106-2: Rename crates (2026-07-13):** k_ia → k_ai (Ring 1 Lógico), jarvis → jarbas (Ring 2 HCI). Backups preservados. **Nomes devem refletir a arquitetura de anéis.**

13. **Sprint 106-5: RustPython viabilidade (2026-07-13):** RustPython **NÃO é no_std nativo** — depende de `std`. **Rota principal = MicroPython/WASM (106-6)** via `wasm_rt.rs` e `micropython_wasm.rs`.

14. **Sprint 106-6: MicroPython/WASM (2026-07-13):** Compilado MicroPython para .wasm, sandbox isolado. **WASM é sandbox seguro para skills.**

15. **Sprint 106-7: Page faults (2026-07-13):** Ordem correta: allocator → events → agents. lazy_init!() para agentes dependentes de heap. **Inicialização deve seguir ordem estrita.**

16. **Capability ADR-0041 P0–P9 (2026-07-14):** Platform sync ANTES dos drivers. Toda demo Cap é **non-fatal**. Ring3 PoC existe (`iretq`/stub) mas **default off** (`TRY_ENTER_RING3=false`) no boot estável. VirtIO vring = layout+pin **sem QUEUE_NOTIFY**. #PF = PRESENT only (**sem I/O no fault**). Syscall soft = `int 0x90` (não 0x80).
17. **Adequação ADR-0042 (2026-07-14):** Cadeia `k-nano → k-ai → cortex → hermes → jarbas`. Identidades: legível / HW-AI+SelfHeal / cérebro / orquestra / ego+10%. Boot OK = N0; implementar **N1→N5** sem regredir Runtime. Boot OK ≠ visão completa. **`v2.0.0` só quando N1–N5 prontos**; até lá tags `1.x` (ex. 1.5.7 Cap, **1.7.0** N1+2B LOADED).

18. **v1.7.0 / soft-float + 2B LOADED (2026-07-15):** Nightly SSE em `x86_64-unknown-none` → soft-float + `cargo nk`. FAT free-scan por setor (não 1 I/O/entry). BitNet 2B real ~590MB/30L (não confiar ficheiro ~203MB truncado). QEMU load+FWD: timeout serial **≥~5 min**. **LOADED ≠ generate**: `[JARBAS-TTS] FAILED empty generate` é known issue.

19. **ADR-0045 Sound (2026-07-16):** Voz bootável = HDA + Piper (+formant) + STT CTC nativos em `neural-kernel/src/audio`. **Não** reabrir sherpa-onnx / Vosk / Kokoro-primário / Wyoming / Rustpotter como stack de kernel. `jarbas/audio` é espelho não wired. WakeWord **registrado** (Loop 5); leftovers (Mic→WAKE e2e, STT retrain, Piper VITS, UAC, jarbas wire) → **Sprint Sound (reaberta)** — ver SESSION_111.

20. **HW USB / FB console (SESSION_139, 2026-07-17):** Stick removable Windows = MBR slot0 FAT32 dados (`0x0C`) + slot1 ESP (`0xEF`) — não 0xEE-first nem GPT-only. Sem serial: `console_clear` + `console_print` (limpa faixa); `fb_print`→mesmo path; limpar TRACE no probe. `BOOT.LOG` só após heap (`heap_ready`). GOP Intel `BltOnly` → vendor bootloader `SetMode` Rgb/Bgr. Early boot: sem alloc/`println` em `disable_vga_plane`.

---

## MAPA DE MEMÓRIA (MemPalace + Docs)

| Domínio | Onde encontrar | Propósito |
|---------|---------------|-----------|
| Estado atual | `docs/memory/STATE.md` | Versão, sprint atual, arquitetura, pendências |
| Ideias | `docs/memory/IDEA_BANK.md` | 354+ ideias catalogadas com status e sprint |
| Decisões | `docs/architecture/*.md` | 38+ ADRs (ADR-0001 a ADR-0037+) |
| Planos históricos | `docs/archive/sprints/` | Planos concluídos, incluindo Sprints 92–100 e Sprint 106 |
| Checklist | `TODO.md` | Checklist mestre com sub-itens, goals, dificuldades |
| Sessões (aprendizado) | `docs/memory/SESSION_*.md` | 42+ sessões com descobertas e correções |
| Este índice | `docs/memory/SESSION_INDEX.md` | Catálogo de sessões + lições críticas |
| Sprints detalhados | `docs/archive/sprints/SPRINT-106.md` | Sprint 106-1 a 106-10 com ações e resultados (arquivado) |
| Roadmap completo | `ROADMAP.md` | v1.0 → v2.0 com status de cada sprint |
| CHANGELOG | `CHANGELOG.md` | Histórico de versões |
| Código fonte | `crates/neural-kernel/src/` | Kernel bare-metal (135+ arquivos Rust) |
| Workspace crates | `crates/k_nano/`, `crates/k_ai/`, `crates/cortex/`, `crates/hermes/`, `crates/jarbas/` | Anéis lógicos K³CHJ (v2.0) |
| Config VM | `tools/` | Scripts de build, QEMU launch, image creation |

---

## SPRINT 106 — RESUMO (2026-07-13)

| Sprint | Status | Descrição |
|--------|--------|-----------|
| 106-1 | ✅ | Cargo workspace estrito (k_nano, k_ai, cortex, hermes, jarbas) |
| 106-2 | ✅ | Rename crates (k_ia→k_ai, jarvis→jarbas) |
| 106-3 | ✅ | SOUL.md parser: `neural_kernel::fs::read_vfs()` — 0 refs ATA_DRIVER em jarbas |
| 106-4 | ✅ | Trinity MoE router: roteia para Hermes agents |
| 106-5 | ✅ | RustPython no_std (embed #![no_std], bridge abi_x86_interrupt) |
| 106-6 | ✅ | MicroPython/WASM (sandbox isolado) |
| 106-7 | ✅ | Page faults: allocator → events → agents |
| 106-8 | ✅ | AIOS API (aios_net, aios_fs via RAG) |
| 106-9 | ✅ | Escalonamento Evolutivo (Python→WASM via SkillOpt) |
| 106-10 | ✅ | SkillOpt: Tradução Python→Rust no_std |

**Status v2.0:** ✅ Sprint 106 concluída (10/10). ✅ Sprint 107 Voice FECHADA (PASS parcial forte+).  
**Pista ativa:** gate `v2.0.0` review + residual soft-float/VITS. Sprint Sound ✅ (`SESSION_122.md`). ADR-0042 **N1–N5 + wire N2.5–N5.7 ✅** no marco v1.8.0.

---

**Índice atualizado para v1.8.5 TEST:** release não estável; política viva `SESSION_107+`; históricos em `docs/archive/sessions/`.

