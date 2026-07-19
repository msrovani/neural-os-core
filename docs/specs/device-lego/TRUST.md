# TRUST — DeviceRecipe + Firmware

Reutiliza ADR-0052/0053 + `k_nano::identity::verify_trusted`. **Não** inventar PKI.

## Envelopes

| Envelope | Algoritmo | Cobre |
|----------|-----------|--------|
| Recipe | FNV-1a64 `content_hash` + Ed25519 `signature` | bind, stages, lista FW+hashes, caps, honesty |
| FW blob | `blob_hash` citado na recipe assinada | Integridade do binário |
| BE R1 nativo | Kernel assinado | Código MMIO |

## Políticas

1. Unsigned ≠ LEGO ativo (draft Escalate).
2. `imported` → `sandbox_status: passed` + trusted.
3. Session key assina drafts; promoção fleet = trusted ou HITL `/approve`.
4. Cap lease ⊆ `capabilities_required`.
5. Offsets só na recipe assinada ou BE in-tree.
6. `honesty: no_fake_ready` no corpo canônico.

## Frontmatter mínimo

```yaml
trust_class: escalate
provenance: imported
sandbox_status: pending
firmware:
  - fat_name: AT10K_F6.BIN
    role: FW_IMAGE
    blob_hash: "<hex>"
content_hash: "<hex16>"
signature: "<hex128>"
```
