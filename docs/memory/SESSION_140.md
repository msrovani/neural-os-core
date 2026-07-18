# SESSION_140 — ADR-0041 H4+/H5+/AS + HalOffer Cap (v1.8.6 TEST)

**Data:** 2026-07-18  
**Versão:** v1.8.6 TEST (não estável)  
**Foco:** Fechar gap ADR-0041 restante — QUEUE_NOTIFY real, residual MMIO→k-hal, Cap enforce, AS shallow.

## Contexto

Plano: H4+ → residual MMIO → H5+ Cap → AS R1/R3 shallow. Mantém **1.8.x**; gate v2.0.0 intacto.  
HalOffer = API R3; VirtIO = transporte BE apenas.

## Feito

### Fase 1 — H4+ QUEUE_NOTIFY
- `k_hal::virtio`: map BAR UC + `try_pci_queue_notify` (cap NOTIFY_CFG)
- Aceite: `NotifySent` (ou `NotifySkipped` honesto); slog `[VirtIO] [notify]`

### Fase 2 — Residual MMIO
- hermes: FE wifi via `pub use k_hal::net::*`; `link_watcher` sem BAR fake
- jarbas: `virtio_gpu` FE (HalOffer + kick k-hal); `fb_remap_uc` → `disable_intel_vga_plane` no k-hal
- `gpu/mod.rs` = `pub use k_hal::gpu::*` (+ cube)
- virtio-net bin: HalOffer Net + `try_pci_queue_notify`

### Fase 3 — H5+ Cap
- `cap_gate`: `grant_fe` / `revoke_fe` / `check_fe_bound` / `FeVideo`
- `offer::bind` granta Cap; ports `fe_*` Deny sem bind
- Hermes: CapDenied → Quarantined

### Fase 4 — AS shallow
- `address_space::demo_as_r1_r3_shallow` no boot (CR3 + touch BAR + restore; R3 MAP_BAR Deny)
- Limite documentado: monólito; shallow L4; **≠** isolamento produção

### Docs / versão
- ADR-0041 checklist por fase; STATE; INDEX; TECNOLOGIAS; IDEA #459
- Tag **v1.8.6** TEST — não v2.0.0

## Aceite

| Check | Resultado |
|-------|-----------|
| `cargo check -p k-hal/hermes/jarbas/neural-kernel --release` | 0 erros |
| Lifecycle ADR-0041 | `fazendo` (PoC até aceite QEMU maintainer) |
| Gate v2.0.0 | fechado |

## Lições

1. **HalOffer ≠ VirtIO** — nomear API de produto separado do transporte evita confusão R3.
2. **Cap lógica no bind** — FE Deny sem bind fecha H5+ sem CR3 ainda.
3. **AS shallow no bin** — reusa P2; não inventar segundo kernel.
4. Orfãos `jarbas/gpu/*.rs` no disco (não `mod`) confundem `rg`; limpeza higiene ≠ runtime.

## Próximo

- Boot QEMU: confirmar `NotifySent` + slog Cap/AS non-fatal
- Sprint Net (DHCP/RX)
- Não declarar ADR-0041 `completa` / v2.0.0 sem OK humano
