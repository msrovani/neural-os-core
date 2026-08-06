# SESSION_251 — Tier 0+1 ADR (0041/0083a/0045/0082) + Fix raiz reboot loop IST (2026-08-05)

**Escopo:** Implementar a fila ADR ordenada por complexidade (Tier 0+1: itens 1–4 do TODO).
**Status:** 4/4 itens completos · boot QEMU desbloqueado (fix raiz IST) · evidência 0041 commitada em `docs/evidence/`.

---

## 1. Entregas por item

### 1.1 0041 — Aceite QEMU slog (item 1, Tier 0) ✅
- **Evidência:** `docs/evidence/boot-whpx-20260805.txt` (WHPX, 88KB).
- Strings de aceite validadas no log:
  - `[VirtIO] [notify] - QUEUE_NOTIFY pci q=0 @ 0x800003000 bus=0:6:0` → **NotifySent ≥1×**
  - `[VirtIO] [h4] - OK: 1/1 VirtIO-PCI QUEUE_NOTIFY enviado`
  - `[Cap] [h5_demo] - R1=Allow R3_no_cap=Deny FE_no_bind=Deny` + `FE_after_grant=Allow`
  - `[AS] [r1] - restore CR3 OK - shallow PoC done`
  - `BOOT: MVP-C` + `P3/P4/P5×2/P6/P7/P8/P9` todos OK (non-fatal)
  - `[AgentFleet] 57 agents` + `[Sched]` + `[BOOT:Runtime]` — sem #GP/#PF fatal.
- **Fix raiz necessário** para o boot sequer passar (ver §2) — sem ele, WHPX **e** TCG reboot-loop em T+0.

### 1.2 0083a — Log honesto fallback LCG + assets FAT (item 2, Tier 1) ✅
- `crates/cortex/src/trinity.rs` `init_router_weights`: warn honesto no fallback determinístico
  (`k_nano::slog_cortex!` "fallback deterministic LCG seed=42 UNTRAINED — nenhum peso de arquivo carregado").
  O `load_router(..., trained=false)` já logava; fechado o buraco do caminho sem statics.
- `ROUTER.BITNET` **confirmado no FAT**: `tools/mkfat32.py:200` + `models/ROUTER.BITNET` (27.780 B);
  boot loga `Router MoE weights: DETERMINISTIC FALLBACK (LCG seed=42, UNTRAINED)` quando o arquivo não carrega (evidência no boot).

### 1.3 0045 — Cutover jarbas pleno (item 3, Tier 1) ✅ (documental)
- **Descoberta:** o cutover **já aconteceu** no commit `e51a48b` (E4 emagrecer) — o bin re-exporta
  `jarbas_crate::audio::*` e jarbas tem os 20 arquivos truth; `neural-kernel/src/audio` = facade de 4 linhas.
- O que estava stale: docs + log do bridge. Reconciliados:
  - `jarbas_bridge.rs`: doc header + log → `audio_truth=jarbas-crate cutover=done(e51a48b)`;
    checks `topics_in_sync`/`settings_contract_ok` marcados tautológicos (re-export), mantidos como contrato legacy.
  - ADR-0045 §1/§2/§3/§5/§7: truth = jarbas; residuals honestos: soft-float/VITS aberto, UAC ▶️ AWAITING_HW,
    **dedup HDA k_nano↔k_hal pendente** (guarda `check_duplication.py`).
  - TODO.md §SPRINT SOUND + STATE.md: linhas "jarbas/audio wire ▶️"/"Unify truth↔espelho ▶️" → ✅ e51a48b.

### 1.4 0082 — HardwareInfo Onda CPU (item 4, Tier 1) ✅
- `crates/k_ai/src/sgdb/store.rs`: `ns::HW = "hw/"` + `populate_hw_namespace()` em `boot_init()` (Fase 6).
- Escritas (valores string lowercase, convention ADR-0082): `hw/cpu/{isa,avx2,avx512,fma,hv}`,
  `hw/cache/{l1d,l1i,l2,l3}` (bytes), `hw/mem/total_mb` (TOTAL_RAM_MB). Falhas de put → warn não-fatal.
- ADR-0082 checklist tickado: `- [x] boot_init() escreve /hw/cpu/*` (Onda CPU — SESSION 2026-08-05).
- Evidência no boot: `[SGDB] [hw] - Onda CPU: /hw/* populado (isa=avx2+fma, hv=WHPX, ram_mb=7168)`.
- Ponytail: sem wrappers tipados / snapshot WASM (YAGNI — ninguém consome ainda).

---

## 2. 🔴 Fix raiz do reboot loop (bloqueava o 0041)

**Sintoma:** boot QEMU (WHPX **e** TCG) em **triple-fault loop** em T+0, logo após
`heap auto-alvo` (main.rs:1446), antes de HardwareDiscovery. O commit 2662d50 do editor
(SESSION_250) **veio com este reboot loop como known-issue** (§4-5: "boot QEMU com reboot loop"; o
editor só testou o working tree com HEAP_EXT_BASE, o revert commitado nunca bootou OK).

**Diagnóstico (via `qemu -d int,cpu_reset`):**
- 1ª exceção: `v=0e` #PF em `Tag::write` do TALC (`0xffffffff807b0cf9`), CR2=`0x400000080000` (LARGE_HEAP_START) — o claim do TALC escreve metadados no span não-mapeado (esperado; o handler deveria curar via `try_fault_in_heap`).
- **A entrega da #PF falha** (`check_exception old: 0xe new: 0xe` → v=08 #DF → `old: 0x8 new: 0xe` → **Triple fault**).
- #DF com **CR2=`0xfffffffffffffff8`** = push de frame para stack ~0 → **stack IST inválida**.

**Causa raiz:** `crates/k_nano/src/interrupts.rs` — o GDT lazy_static fazia
`Descriptor::tss_segment(&TSS_ARRAY[0])` **cru** (`TaskStateSegment::new()` = `interrupt_stack_table`
todos zerados), enquanto o lazy_static `TSS` (que preenche os ISTs) **nunca era dereferenciado**.
Resultado: qualquer gate com IST (#PF/#GP/timer/#DF) entrega a exceção empurrando o frame para a
stack IST = **0** → #PF na entrega → #DF (também com IST zerado) → triple fault.

**Fix (1 linha):** `gdt.add_entry(Descriptor::tss_segment(&*TSS))` — o deref `&*TSS` força o
lazy_static a rodar e configurar os ISTs no `TSS_ARRAY[0]` antes do descriptor ser criado.

**Fix adicional (defensivo, prescrição do SESSION_250 §4):** `map_page_direct` agora checa
`HUGE_PAGE` nos níveis e3 (P4→P3) e e2 (P3→P2) além do e1 já existente — não desce para walk
quando a entrada já é 1GB/2MB page (evita ler P2 garbage → páginas não-mapeadas → #PF).

**Verificação:** boot WHPX completo (T+441, scheduler vivo), TCG chega a T+1900+ (lento/não-determinístico);
`cargo check --release -p neural-kernel -p k-nano -p k_ai -p cortex` 0 erros.

---

## 3. Lições

1. **lazy_static que preenche um static mut compartilhado pode nunca rodar** — o GDT usava
   `&TSS_ARRAY[0]` direto e o `TSS` lazy_static (que seta ISTs no mesmo array) era dead-code.
   `&*TSS` no ponto de uso força o init. Regra: se um lazy_static só existe para inicializar
   outra static, dereferecie-o explicitamente no caminho de boot.
2. **IST zerado ≠ sem IST**: gate com IST field ≠ 0 mas TSS.ist[field] = 0 entrega a exceção com
   push para VA 0 → #DF → triple. Sintoma QEMU: `CR2=0xfffffffffffffff8` no #DF.
3. **Reboot loop com fallback que não dispara**: o script `run-qemu-whpx.ps1` só troca WHPX→TCG
   em erro de launch; **hang/triple-fault silencioso não aciona o fallback**. Validar sempre com
   `-d int,cpu_reset` e greppar `Triple fault`/`check_exception`.
4. **Boot TCG é lento e não-determinístico** (~1900+ ticks para DriverInit, para em pontos
   variáveis); evidência de aceite de boot deve usar **WHPX** quando disponível.
5. **Commit de sessão paralela pode vir quebrado**: 2662d50 (SESSION_250) commitou o revert do
   HEAP_EXT_BASE sem re-bootar (o working tree testado tinha bump_virt). Confirmar boot de
   HEAD antes de confiar em commits de outra sessão.

---

## 4. Arquivos tocados

| Arquivo | Mudança |
|---|---|
| `crates/k_nano/src/interrupts.rs` | **Fix raiz:** `&*TSS` (força lazy_static → ISTs) |
| `crates/k_nano/src/allocator.rs` | Checks HUGE_PAGE em e3/e2 no `map_page_direct` |
| `crates/cortex/src/trinity.rs` | Warn honesto no fallback LCG (`init_router_weights`) |
| `crates/k_ai/src/sgdb/store.rs` | `ns::HW` + `populate_hw_namespace` (Onda CPU) |
| `crates/neural-kernel/src/jarbas_bridge.rs` | Log/docs → cutover=done(e51a48b) |
| `docs/architecture/0045-sound-voice-stack.md` | §1/§2/§3/§5/§7 → truth=jarbas, residuals honestos |
| `docs/architecture/0082-hardware-info-registry.md` | Tick Onda CPU |
| `docs/evidence/boot-whpx-20260805.txt` | Evidência 0041 (WHPX) |
| `TODO.md`, `STATE.md`, `SESSION_INDEX.md`, `CHANGELOG.md` | Registros pós-tarefa |

**Pendências herdadas (não desta sessão):** dedup HDA k_nano↔k_hal (guarda 0045), UAC ▶️ AWAITING_HW,
soft-float/VITS, ROUTER.BITNET treinado (0083 item 11), wrap 2⁶⁴ do grow runtime 2B (SESSION_250 §4).
