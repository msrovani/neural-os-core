# SESSION_258 — Bughunt (auditoria + runtime) + Fixes Scheduler + MBR HW boot (2026-08-11)

**Escopo:** Bughunt de auditoria estática (4 lanes oracle) + validação runtime QEMU com
loader 2B v6 + fixes 3× HIGH no scheduler + fix de boot UEFI do pendrive HW real (MBR).
**Status:** ✅ Fechada — 2 commits (`f44d343` + `2dd6ffc`) — 0 erros — 186 testes PASS.

---

## 1. Bughunt — Auditoria estática (4 lanes oracle paralelas)

Lanes read-only; achados verificados pelo orquestrador contra o código real (nada de
confiar cegamente — o ora-1 produziu 1 falso HIGH que foi **refutado por medição própria**).

### Confirmados (verificados no código)

| Sev | Bug | Evidência |
|---|---|---|
| HIGH | `set_urgency("net", 180)` — manifest real do NetAgent é `"network_agent"` → **fix de starvation do s252 nunca aplicava** (rede rate-limited 80% após 50 Pending) | main.rs:2651 vs agents.rs:221 |
| HIGH | Watchdog `consecutive_pending > 10000` → `Crashed` sem recuperação (RESPAWN_QUEUE sem writers, hooks não wireados); agentes interativos com urgency>0 pollam todo tick → mortos em ~9 min | lib.rs:466-472 |
| HIGH | EventDriven dorme para sempre após 20 Pending (`has_event = consecutive < 20`, self-referential, sem wake externo) → 147 Agency specialists + AutoInstallerAgent (SYS_INSTALL nunca consumido) inertes | lib.rs:440 |
| MED | `grow_bump_auto` sem guard de wrap 2⁶⁴: `heap_start + offset` (high-half) wrapa p/ VAs baixas; boot atual cresceu 2560MB (offset ~2305MB) — empiricamente não crashou, mas é a classe de corrupção do s254 reaberta | allocator.rs:102 |
| MED | 2º probe QEMU-loader hardcoda `BITNET_2B_V4_BYTES` (604MB) — fix autodescritivo `v6_file_size` só no 1º site; truncaria v6 de 792MB nesse endereço | main.rs:2889-2892 |
| MED | BudgetManager inerte pós-reset_all: poll ≤1x/ciclo → `ticks_used∈{0,1}`<100 → Paused/overruns = dead code | lib.rs:409, budget.rs |
| MED | `map_region_uc_2mb_at` grava PDPTE com HUGE_PAGE (2MB no nível errado — deveria ser PDE) → 512 slots 2MB de 1GB alias na MESMA página física; latente (GPU AWAITING_HW) mas catastrófico se usado | apic.rs:440-446 |
| MED | Expert loader-scan hardcoded 0x129000000-0x129400000 cai DENTRO do arquivo 2B em 0x100000000 (792MB→0x131800000); `QEMU_LOADER_SCAN_START` nunca lido | main.rs:3161-3162,3253 |
| MED | `deallocate_frame` sem ownership check — stale/double-free libera frame vivo (kernel/stack/PT) | memory.rs:270-283 |
| MED | `detect_qemu_net_mode` reverse-scan lê RAM crua 1MB-a-1MB até 4GB → false-positive em bytes de modelo (sem flag) | net.rs:301-341 |
| LOW | `paused_ticks>=10000` unreachable (recover em >=1000 primeiro); `last_poll` antes de `check_budget`; respawn com `goal_urgency=0` | lib.rs:311-320,445 |

### Refutado (falso HIGH evitado)

ora-1 claim: "feat=0x42/act=10 rejeita 3 de 4 .v6 canônicos (AGENT/LEARNER/PRO)". O oráculo
leu os headers em offsets errados. **Medição própria com os offsets canônicos do
`v6_file_size` (hidden@18, tok_len@45, act/emb/feat após tokenizer):** todos os 4 passam no
gate (`feat&0xF8==0`, act≤1, emb≤2) — BITNET2B feat=0x07 act=1 emb=1, demais feat=0x04.
Lição: verificar achado HIGH medindo o artefato real antes de reportar.

## 2. Validação runtime QEMU (loader BITNET2B.v6 @0x100000000)

- **Rebuild real forçado:** binário de 02/08 vs código de 09/08 — cache incremental
  mascarava erros (o `cargo clean -p neural-kernel` removeu 0 arquivos: kernel é no_std,
  artefatos em `target/x86_64-unknown-none/`). Removido o dir do target + `cargo check
  --release` 1m19s: **0 erros**.
- Boot `-m 6G -smp 2 -cpu max -accel tcg` + OVMF + loader 2B v6: **8 fases + Runtime,
  sem panic/triple-fault** — crash ip=0 da SESSÃO_254 **não reincidiu** (fix da stack
  validado no tree atual).
- Auto-grow AIOS: 512→768→1024→1280→2048→2304→2560MB; LLM LOADED h=2560 L=30 (2B v6);
  FWD começou (lento em TCG soft-float, esperado).

## 3. Fixes scheduler 3× HIGH (commit `f44d343`)

Ora-3 (mesma sessão, contexto carregado) projetou o diff exato → fixer aplicou →
orquestrador conferiu linha a linha contra a spec.

1. **`set_urgency("net")` → `"network_agent"`** (main.rs:2651 + log): NetAgent finalmente
   isento do rate-limit. Verificado: nenhum manifest `name: "net"` existe; respawn já usava
   `"network_agent"` (main.rs:877).
2. **Watchdog só crashea sem urgency:** helper `watchdog_should_crash(urgency, consecutive)`
   = `urgency==0 && consecutive>10000` (espelha a isenção do rate-limit). Interativos
   (urgency>0) nunca mais Crashed por "nunca Done". (Nota: fleet idle sem urgency ainda
   crasha em ~46min — MED #5 pré-existente; remover a transição Pending→Crashed se virar
   requisito.)
3. **EventDriven via `has_pending()`:** trait `Agent::has_pending()` (default false) +
   helper `event_driven_has_event(last_poll, has_pending)` = `last_poll==0 || has_pending`.
   Run loop: EventDriven polla no 1º tick (announce) ou com trabalho pendente — nunca por
   contador self-referential. `AutoInstallerAgent` e `SgdbAgent` implementam via
   `receiver.has_pending()` (API já existia no event-bus; agents já gateavam o tick nela —
   só nunca eram pollados após 20 ticks).
4. **Teste host novo:** `sched_semantics_event_driven_and_watchdog` (agent-core).

**Bônus:** teste `hwexpert_v6_matches_v5_predictions` (cortex) estava quebrado desde
`372afd6` (paths include_bytes apontavam p/ modelos movidos p/ `legacy/` e `target1/`).
Corrigido + goldens trackeados (un-ignore explícito — lição SESSÃO_249: include_bytes de
arquivo gitignored quebra em clone fresco).

**Verificação:** 186 testes PASS (0 failed), `cargo check --release` 0 erros.

## 4. Fix boot UEFI pendrive HW (commit `2dd6ffc`)

**Sintoma:** pendrive (unidade E:) com imagem HW real não é reconhecido como dispositivo
de boot (Windows monta a partição de dados FAT32 como E:, firmware UEFI não lista o stick).

**Causa raiz (regressão `df88cc0`, 29/07):** `write_removable_mbr` (build_usb_unified.py)
escrevia MBR **só com slot0=dados 0x0C** — sem entrada 0xEE protetora (UEFI spec: sem
0xEE o firmware trata o disco como MBR-legacy com FAT sem boot flag → não lista como
bootável) e sem o slot ESP. O commit anterior (pré-df88cc0) tinha slot0=dados+slot1=ESP
0xEF(0x80) e bootava, mas o Windows montava o ESP FAT32 em vez de NEURAL-OS — o "fix"
trocou boot UEFI por letra no Explorer. O docstring do arquivo ainda documentava o design
correto (`Slot1 — ESP 0xEF`); a implementação foi removida sem atualizar o doc.

**Fix:** MBR híbrido — `slot0=0xEE` protetora cobrindo o disco TODO (firmware UEFI
reconhece GPT → acha ESP em LBA 2048; mesmo padrão do uefi.img/mk_esp_fat.py que já boota
no OVMF) + `slot1=dados 0x0C` (Windows monta E:, skipa a 0xEE como protetora). Docstring
do módulo atualizado.

**Validação:** imagem de teste (639MB) gerada pelo script → boot QEMU/OVMF: `boot=limine`
+ 8 fases + DriverInit OK (o mesmo caminho de firmware UEFI do notebook). MBR inspecionado:
slot0 type=0xEE start=1 size=disco todo, slot1 type=0x0C, sig 55AA.

**Pendência do usuário:** regravar `target/usb_hw.img` (build completo `PACK_LLM=2b
--size 6144`, ~6.3GB) no pendrive via Rufus DD (Secure Boot OFF). O build completo foi
abortado 1× (output buffered por `Select-Object -Last` parecia travado; não estava).

---

## 5. Lições

1. **Falso HIGH em auditoria:** oráculo leu headers .v6 em offsets errados (pegou bytes do
   tokenizer como act/feat). Verificar achado HIGH medindo o artefato real antes de reportar.
2. **`cargo clean -p neural-kernel` remove 0 arquivos** quando o kernel é no_std
   (artefatos em `target/x86_64-unknown-none/`). Para rebuild real: remover esse dir.
3. **`set_urgency` com nome errado = fix morto:** verificar o nome do manifest antes de
   aplicar urgency (o bug do s252 "rede morre" só não era visto porque... agora é).
4. **Watchdog que não distingue idle de runaway mata fleet inteiro:** crashar agente por
   "nunca Done" é errado quando Continuous retorna Pending por design. Gate por urgency
   (ou remova o Pending→Crashed — o rate-limit já limita CPU).
5. **EventDriven com contador self-referential dorme para sempre:** `consecutive < 20`
   sem caminho de reset externo = 147 agentes mortos. Solução: `has_pending()` na trait
   (o `Receiver::has_pending()` já existia no event-bus).
6. **MBR de pendrive UEFI exige protetora 0xEE:** sem ela o firmware trata como MBR-legacy
   e não lista como bootável. Híbrido 0xEE(disco todo)+dados(0x0C) satisfaz firmware UEFI
   e Windows Explorer. Regressão silenciosa veio de "consertar" o Windows sem testar o boot.
7. **Output buffered em build longo parece travado:** `Select-Object -Last` segura todo o
   stdout até o fim; em build de minutos, o usuário aborta achando que travou. Stream direto.
8. **include_bytes! de modelo gitignored quebra o teste em clone fresco** (lição s249
   reaplicada): un-ignore + trackear goldens (`!legacy/hw_expert_v4.bitnet` etc).
