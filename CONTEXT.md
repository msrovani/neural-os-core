# CONTEXT.md — Glossário de Domínio (neural-os-core)

Linguagem compartilhada entre humanos e agentes. Este arquivo só fixa **vocabulário** — use os termos abaixo para nomear arquivos, variáveis, testes e decisões com a mesma língua. Regras e processo vivem em `AGENTS.md`; este arquivo não manda em nada além de palavras.

## Princípios

- **AIOS** — Sistema Operacional com Inteligência Artificial desde o boot. IA não é feature: é o modo de operar (ADR-0088).
- **Premisa Máxima** — 5 regras irrevogáveis (ADR-0088): AIOS-first; auto-tudo (adaptar/curar/upgrade/gerar/pesquisar); toda decisão tratada com inferência/adaptação/memorização/versionamento; nada bypassado sem análise (IDEA → ADR → SESSION); busca incessante dos 10% de melhoria. Primeira coisa a analisar em toda decisão.
- **HITL** — human-in-the-loop: a IA decide e age, o humano valida nos gates. Sempre.
- **Tudo é Agent ou Skill** — sem tasks, sem serviços, sem drivers standalone. Todo agente tem manifesto, capabilities e lifecycle.
- **Anéis (R0–R3)** — organização **lógica** de dependência (R0 fundação → R3 aplicação), NÃO privilégio do processador: todo o código roda em Ring 0 real (CPL=0). Isolamento efetivo hoje = wasmi (Caminho A) + Ring3 gated (ADR-0060).

## Arquitetura

- **K³CHJ** — stack de crates: `k_nano` (R0) ← `k_hal` (R1) ← `cortex`/`k_ai` (R2) ← `hermes`/`jarbas` (R3) + `neural-kernel` (bin de boot). K²CHJ = histórico sem k_hal.
- **Wire** — integração de crate no bin: alias `*-crate` + `pub use`. "Wired" = ligado ao boot; sem wire o código existe mas não roda.
- **Emagrecer** — política: lógica nova nas crates; o bin só faz wire/bridge (`.cursor/rules/neural-emagrecer-bin.mdc`).
- **Residual** — trabalho adiado com veredicto registrado (ex.: "Residuals 0–7 ✅").
- **Gate** — checkpoint formal (net gate canônico = e1000 + smoltcp; gate v2.0.0 = N1–N5 + wire + review). SLIP/COM2 é debug congelado, não é path de gate.
- **Trinity MoE** — LLM + router treinável + experts. **AutoLearn**: detecta necessidade → treina → registra expert.
- **CapGate** — gate de capabilities nos host-imports (`aios::*`).
- **Agency** — frota de agentes (~50 nativos + HW) com schedule (Oneshot/Continuous/EventDriven/PollEvery) e trust por (token, agent, skill).

## Boot

- **8 fases event-driven** — SafeHarbor → MemoryCore → SystemBringup → Diagnostics → HardwareDiscovery → DriverInit → AgentFleet → Runtime. Cada fase publica `BOOT_PHASE` no EventBus.
- **DeviceTree / H1** — `k_hal::init` pós-PCI, **antes** dos drivers. Árvore de `DeviceCap` é a evidência de silício; SelfHeal lê `from_khal`, não rescaneia ATA.
- **boot_bind / boot_observe** — R0 instala ordem NIC+storage; R2 (`k_ai`) observa a árvore, aplica Trust `(1,boot_observe,plan)` e recipe HITL. Bin só executa. Escalate ≠ Auto.
- **Observe→Plan→Act→Verify→Remember** — “IA desde o boot” (ADR-0088): DeviceTree → plano k_ai → probe na ordem → SelfHeal na mesma árvore → HANR `hydrate_memory`. Não é CortexAgent no T+0.
- **Limine** — bootloader atual. `kernel.elf` no ESP é contrato de path.
- **uefi.img / bios.img** — só UEFI/OVMF boota; a imagem BIOS dá triple-fault.

## Storage

- **Ordem de probe storage** — NVMe > AHCI > USB-MSC > ATA PIO (`StorageKind` + `storage_probe`). ATA PIO é último, não default. Residual #513: `measure_bandwidth` / BMIDE 0xC8.
- **NeuralFS** — FS principal (vive em k_nano). Herança BAFS. **Contrato**: blocos contíguos (data_block+count) — o alocador DEVE validar contiguidade (ordenar + `w[i+1]==w[i]+1`, fallback bump).
- **Ordem CoW** — dados novos → commit → SÓ ENTÃO reclaim antigos (freeing adiado 1 commit).
- **SGDB vs FAT** — SGDB = path cognitivo (HANR/Audit/Pkg meta/Skills/Episodic/RAG); FAT = blobs/firmware/WIFI.CFG/BOOT.LOG.
- **ESP 0xEF** — partição FAT32 real do Limine; GPT GUID é sempre `.bytes_le`. Nunca formatar volume que existe mas não monta (fsck explícito exigido).
- **Storage cru (TickvLite)** — nunca LBA fixo perto das partições GPT (brick em NVMe real). Região calculada no fim do disco ou partição própria.
- **`NeuralVolume`** — exige `&mut dyn BlockDevice`; cast `let dev: &mut dyn BlockDevice = g;` (nunca `&mut **g`).

## Rede

- **Net gate canônico** = e1000 + smoltcp (user/slirp). SLIP/COM2 frozen (debug).
- **e1000 TX 0x3800/0x3818** — offsets 0x0420/0x0438 são aliases Intel não-wired no QEMU (write no-op).
- **DMA pages UC** — buffers de DMA (TX/RX rings) DEVEM usar `map_page_uc` (PWT|PCD); senão o NIC lê cache stale (RX=0).
- **Mesh P2P (ADR-0081)** — transporte em k_nano R0; reassembly 16 slots + ACK seletivo (FRAG\0→FRACK\0), tiers de segurança (L=HMAC-SHA256 dados, F=Ed25519 controle/TOFU), token bucket, PeerHealth com p99 EWMA.
- **smoltcp clock** — TIMER_TICKS ≠ ms (~55ms/tick); sempre `Instant::from_millis(now * 55)`.
- **WHPX** — aceleração QEMU nativa; TCG = fallback lento/não-determinístico. Wifi bridge sobre TCG é instável.

## Build & Dev

- **cargo nk** — alias de check release com clean (`.cargo/config.toml`).
- **cargo clean -p neural-kernel** — antes do check após mudanças estruturais (cache incremental mascara erros).
- **Target dirs isolados** — `target/agent-*`, `target/check-*`; nunca `target-*` na raiz.
- **Known Warnings** — dead-code é esperado; **0 erros** é obrigatório.
- **Testes host** — `cargo test --workspace --exclude neural-kernel --exclude boot`; HW-only gated com `cfg(target_os = "none")`, NÃO `cfg(test)`.
- **check_duplication.py** — guarda: mesmo `.rs` (não-facade) em ≥2 crates = exit 1.
- **Nightly 1.98** — `nightly-2026-07-05`: ≥1.99 quebra x86_64 0.14.13 (trait Step); ≤1.97 falta `str_from_utf16_endian`.

## Contratos de ambiente

- **Nomes 8.3 FAT** (`KERNEL~1`) e path Limine (`kernel.elf`) — contratos; configurá-los quebra o sistema.
- **RDLEN** — múltiplo de 128B (min 8 descritores × 16B) no RX do e1000.
- **Artefato exportado é o contrato** — validar o ARQUIVO (.bitnet/.bin), nunca métricas em memória.
- **Hypervisor probe** — `hypervisor()` devolve None antes de `detect()`; gate de MSR exige `probe_done()` + hv real.
