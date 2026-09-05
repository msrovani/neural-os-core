# SESSION 314 — P0 BOOT.LOG metal + k_hal USB BE + governança

**Data:** 2026-09-05  
**Escopo:** Registo P0 (evidência no stick) + cutover USB hub→MSC em `k_hal` + S0 dedupe ADR-0103; aceite Alienware **aberto**.  
**ADR:** **0103** (#549) S1 · **0092** (#539) canal A · **0100** Onda 1 I/O · **0088**  
**Não abre:** 0102 Ring3, 0103 S2–S6 / Fase 2, 0101 sprint, 0089 runqueue/malha  

---

## Objetivo

Alienware já chega ao desktop (SESSION_313) mas `E:\BOOT.LOG` fica placeholder e `NSGDB.BIN` zerado. Fechar o ciclo de evidência **sem** perder o boot metal: política MSC em R1, primitivos em R0.

---

## Feito nesta sessão (código + docs)

### S0 — dedupe (ADR-0103)

- `multi_user` canónico em `k_ai`; removido de `k_nano`; bin `pub use k_ai::multi_user`
- `hnsw` canónico em `cortex`; removido de `k_nano`; hermes/vfs + bin → cortex
- `cargo check --release` 0 erros (crates + neural-kernel)

### S1 — USB host BE (em curso)

- `k_hal::usb` (`hub_msc` route+TT): hook via `register_msc_bringup`
- `k_hal::init_h1` / `install_bringup_hooks` antes do DriverInit
- Early + retry path no bin: `k_hal::usb::probe_and_install()` (não root-only direto)
- SESSION_313: Event Ring `RTSOFF+0x20`, TRB IOC/CC, handoff metal, UI sem probe pós-live

### Governança P0

- ADR-0103 criada; IDEA #549; INDEX + IDEA_BANK
- Plano Cursor **BOOT.LOG metal P0** → INDEX Planos Cursor (esta SESSION)
- TODO secção ADR-0103; STATE pista P0
- Checklist flash: `docs/memory/HW_FLASH_s314.md`
- Template aceite: `logs/hw_alienware_s314/REPORT.md`

---

## Aceite metal (obrigatório — operador)

Ver `HW_FLASH_s314.md`. PASS só com:

1. `E:\BOOT.LOG` = BOM + `[S] neural-os-core` + checkpoints (não placeholder)
2. Desktop + mouse vivos ≥ 2 min
3. Se MSC FAIL: **foto FB** `--- USB ramlog ---` (sem COM1) + stick ainda placeholder
4. Ideal P0.b: `NSGDB.BIN` não-zero

**Status aceite:** ⏳ AWAITING_OPERATOR — imagem **nova** `target/usb_hw.img` 6271 MB · kernel ESP `03eed4dfdce72791` · pack falcon3 completo · Rufus DD + boot Alienware.

---

## Lateralização hub (pós-FAIL placeholder) — 2026-09-05

Pesquisa (Linux `xhci-mem.c` / U-Boot route+TT / Redox Mar/2025 hub driver):

1. **Bug:** speed do filho lia `stbuf` **antes** do clear C_RESET → speed/TT errados. Fix: re-`GetPortStatus` após reset.
2. **Clear C_PORT_CONNECTION** (feature 16) antes do reset (padrão hub Linux).
3. **MTT:** bit 25 no slot do hub (`characteristics&1`) + **DEV_MTT no filho LS/FS** se hub Multi-TT (Linux/U-Boot).
4. **FB dump** `boot_ramlog::dump_usb_hint` no early + DriverInit MSC fail (notebook sem COM1).

`cargo check -p k-hal -p k-nano --release` OK. Rebuild `usb_hw.img` + Rufus DD + foto FB se ainda FAIL.

---

## Freeze (até 2 boots com log)

- ADR-0102 / Onda 6 Ring3 no notebook  
- ADR-0103 S2–S6 e Fase 2 schemes  
- Falcon3 sprint / mesh 2c / smp-runqueue como prioridade  

---

## S1 — fecho honesto (sem falso PASS)

S1 **não** está fechado: falta evidência metal no REPORT.

Se após flash o RESULTADO for FAIL:
1. Não cortar FS/SMP/Ring3
2. Só slog `ok`/`warn` em `k_hal::usb/hub_msc.rs` + xHCI host
3. Nova SESSION (315+) com PORTSC/hub/CC

Se PASS: marcar S1 `[x]` em ADR-0103 + TODO e só então P1 NSGDB.


## Lições

- Hub interno (notebook) = root-only MSC = BOOT.LOG morto; política em `k_hal::usb`.  
- Duplicata `multi_user`≠`percpu`; paths errados no rascunho Redox → ADR-0103 corrige.  
- Aceite de persistência = ficheiro em `E:`, não QEMU.
