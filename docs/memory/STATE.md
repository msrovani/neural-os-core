# STATE — neural-os-core v1.9.99-s349 — Falcon3 WHPX measure + OOM guards

#   PISTA ATIVA: Falcon3-3B lab tok/s WHPX ✅ ~0.15 tok/s; OOM UI T+793 guards (SESSION_349)
#   Script: tools/measure-falcon3-toks.ps1 -Accel whpx; header v6 > FAT; parse in-place
#   Guards: Tensor checked_mul; max_seq heavy clamp 4096; embed_for_kv ctx≤512
#   Retest Window+FAT pendente pós-rebuild (site exato do 4.4GB ainda sob observação)
#   PISTA ANTERIOR: ADR-0101 Onda 0–3 ✅ código (SESSION_348)
#   PISTA ANTERIOR: Onda 6 Ring3 HITL `/ring3` ✅ código (SESSION_347); T-052/053 AWAITING_HW
#   P0 stick: ADR-0103 S1 BOOT.LOG real ainda AWAITING_OPERATOR
#   Não declarar v2.0.0
