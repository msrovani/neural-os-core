# SESSION_237 — Integração memória jcode-inspired + fix xHCI #PF (TCG)

**Data:** 2026-08-01
**Objetivo:** Implementar as 6 linhas da análise jcode (memória 4-tier, retrieval passivo, swarm notify, self-dev F5, skill-embedding, RAM) no neural-os-core; validar com smoke QEMU e **corrigir o erro encontrado** (xHCI #PF sob TCG).

## Implementação (jcode-inspired, ADR-0059/0063/0081, IDEA #218)

| Conceito jcode | O que entrou | Arquivos |
|---|---|---|
| Memória por embedding + consolidação ambient | `k_ai::tiers::consolidate_tiers(tick)` — promoção L1→L2→L3→L4→L5 (Working→Episodic→Semantic→Procedural): batch `prompt_slice(400)` → tópicos por frequência (stopwords pt/en) → doc L3 `topic/<name>` → L4 `sem/<tick>/<name>` (estabilidade ≥2 ciclos) → snapshot L5 `proc/skills` do SKILL_REGISTRY. Publica transições em `TOPIC_MEMORY_TIER` no EventBus. Chamado do SleepCycle CONSOLIDATE. | `k_ai/src/tiers.rs` (novo) + `k_ai/src/lib.rs` + `hermes/src/agents.rs` (CONSOLIDATE) |
| BGE statics duplicados (bug pré-existente) | Boot carregava BGE nas statics do bin; `k_ai` nunca via → recall rodava em pseudo-64d silenciosamente. Fix: bin `memory_systems.rs` vira `pub use k_ai::memory_systems::*;` (fonte única). Recall passa a usar BGE 384d real. | `neural-kernel/src/memory_systems.rs` |
| Retrieval passivo com verificador + gate #314 | `gated_rag_context` em cognitive_bridge: path-trust (`bq+fp32`/`bq`/`empty`) + blacklist 10 padrões injetáveis (do self_evolve) + orçamento 3 hits, antes de injetar no prompt. | `hermes/src/cognitive_bridge.rs` |
| Skills injetadas por embedding | Skills indexadas como `skill:<name>` via `index_embedding`; `find_skill_hint(intent)` (semantic_search, sim ≥0.4) → `[SKILL-HINT]` no system prompt. `invalidate_skill_index()` para refresh. | `hermes/src/skill_loader.rs` |
| Swarm: notificação de mudança | `TOPIC_CHANGE`/`publish_change(what,name)` publicado em 5 pontos de mutação (evolve hot_swap/rollback, self_evolve verify_and_register, skill_sync mesh apply, wasmi_rt register_wasm_skill, bin `/learn`). SelfEvolveAgent (bin) drena + invalida índice. | `hermes/src/self_evolve.rs`, `evolve.rs`, `skill_sync.rs`, `wasmi_rt.rs`, `neural-kernel/src/agents.rs` |
| Self-dev nível WASM (ADR-0059 F5) | `promote_ephemeral_to_wasm` era log-only; agora gera `wasmi_rt::generate_wasm_module()` e promove via `hot_swap` (sandbox wasmi + rollback). Nativo (Ring3) permanece gated (ADR-0060, TRY_ENTER_RING3=false). | `hermes/src/evolve.rs` |

**Verificação build:** `cargo clean -p neural-kernel && cargo check --release` → **0 erros** (29 warnings conhecidos pré-existentes). Lanes: exp-1/exp-2 (mapeamento) → fix-1 (BGE dedup) → fix-2 (k_ai tiers) → fix-3 (hermes) → fix-4 (bin drain) → fix-5 (registros docs) — cada um com `--target-dir target/check-*` isolado (regra do projeto).

## 🔴 Bug encontrado no smoke: xHCI #PF sob TCG

**Sintoma:** boot com `run-qemu-whpx.ps1` (tem `-device qemu-xhci`) parava em **#PF storm** logo após `[BOOT:DriverInit] ATA probe=found` + scan AHCI:
```
[EXC] #PF ip=0xffffffff8070c3a6 ... CR2=0xffff80c000004000 err=0x0 (repetindo infinito)
```
Resolução de símbolo no `kernel.elf` (script Python parse ELF64): ip entre `k_nano::xhci::try_read_config_descriptor` e `init_xhci`. CR2 decodificado = `pmoff + 0xc0000000 + 0x4000` = **BAR xHCI (QEMU 0xc0000000) + RTSOFF 0x4000**.

**Causa raiz:** `init_xhci` (k_nano, HEAD — não tocado por esta sessão) usava `apic::set_page_uc(mmio, pmoff)` que **só seta flags UC em mapeamento EXISTENTE** (retorna cedo se a entrada não é PRESENT — apic.rs:212/225/238). O BAR xHCI nunca foi mapeado → primeiro `r32(base,0)` #PF. O e1000 (funciona) usa `map_page_uc` que **cria** o mapeamento. WHPX mascarava o bug (27/07 boots OK); hoje WHPX falha nesta máquina (`warning: Ignoring request for interrupt vector 0`) → TCG expõe.

**Fix (padrão e1000):** mapear 16 páginas do BAR xHCI com `map_page_uc`:
```rust
// xHCI BAR cobre Cap+Op+Runtime+Doorbell (~64KB). Mapeia TODAS as páginas UC.
for page in 0..16 {
    crate::apic::map_page_uc(mmio + page * 0x1000, pmoff);
}
```

**Validação do fix:** run WHPX pós-fix passou do ponto de fault — `[BOOT:DriverInit] NIC/ATA/AHCI/xHCI/GPU probes concluidos` → `[USB-TRUST] sync ok` (T+18985) → HalOffer `usbhost Available` + LEGOXHCI. **Zero #PF.**

## Smoke QEMU (scripts preparados do repo)

| Script | Config | Resultado |
|---|---|---|
| `run-qemu-whpx.ps1 -Smp 2` (tem qemu-xhci) | WHPX→fallback TCG | pré-fix: #PF storm xhci; **pós-fix: passou xHCI, boot segue** |
| `run-qemu-p2p-mesh.ps1 -NoDisk` (SEM qemu-xhci, config testada SESSION_233) | TCG 8G smp2, 2 nós | **PASS 2/2**: A: Runtime + `MESH_ENGINE node_id=2` + MeshKnowledge P2P_PACKET; B: WASM real self-test PASS `add(2,3)=5` (ADR-0059 A) + netmode STATIC mesh. Scheduler vivo (`tick=361 agents=54`) |

Notas de ambiente:
- **WHPX quebrado nesta máquina** (`Ignoring request for interrupt vector 0`, smp 2 e 4) → tudo caiu para TCG. Não é regressão de código; WHPX funcionava em 27/07.
- **Nó A do mesh #PFou no 1º run** (`ip==CR2==0xffffffffa097c768`, err=0x11 = PTE bit RSVD) no **AP-wake TCG** — flaky documentado no próprio script (ADR-0057: "se A travar em INIT-SIPI-SIPI, relançar (retry resolve)"). **Retry resolveu** (2º run: A bootou até Runtime). Nó B sempre OK.
- `run-qemu-uefi.ps1` tem **erro de parse PS5.1** (string terminator ~linha 235 — provável não-ASCII/em-dash) — não usado; mesh/whpx são os canônicos.
- Run whpx 192852 carregou 10 modelos (LLAMA8B 1.9GB etc) via QEMU loader → TCG mais lento (ainda em boot quando timeout matou), sem #PF.

## Registros (docs-lane fix-5)

- `IDEA_BANK.md`: #218 → ✅ + nota jcode/SESSION_237; linha nova 2026-08-01 na Seção 5.
- `CHANGELOG.md`: entrada `### SESSION_237: Jcode-inspired memory integration (2026-08-01)` + 6 bullets + cargo check 0 erros.
- `STATE.md`: bullet "Memory Integration SESSION_237" (tiers/BGE/gate/SKILL-HINT/CHANGE_NOTIFY/F5).
- `AGENTS.md`: lição nova "Statics duplicados bin↔crate (SESSION_237, BGE case)".

## Next

- Boot completo até SCHEDULER/TIMER com qemu-xhci sob TCG (janela maior) para ver SleepCycle CONSOLIDATE → log `[TIERS]` e `MEMORY_TIER` em runtime.
- Consertar parse do `run-qemu-uefi.ps1` (PS5.1, não-ASCII).
- Ring3/self-dev nativo segue gated (ADR-0060).
