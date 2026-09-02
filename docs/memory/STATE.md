# STATE — neural-os-core v1.9.99-s305 — QEMU 4c loop PASS + P6 iretq (ADR-0102)
#   SESSION_305: QEMU 4c TCG loop exit 0 — saudacao+TTS Jarbas, ring3_can_iretq=true, NSGDB virtio-blk, Runtime+desktop_ready.
#   Fixes: IDT 0x90 DPL=3 k_nano; marker P6 +48; tcg_lite boot gates; virtio-blk BOOT.LOG; Jarbas slog ok.
#   Aceite HW T-052/053 pendente. Piper/LLM ABSENT no FAT (TTS formant drain).
#   SESSION_302: isolamento CPL=3 wired em k_nano + bin facade.
#   H1–H3 ✅ | N1–N7 ✅ | T-056 opcode verifier ✅ | ELF path ✅ | N5 USER mask ✅ | N2 GS.base ✅.
#   register_native_ring gated: can_iretq(self-test) + metal + ring3_mark_hw_gate_passed (T-053).
#   wasmi (A) default em QEMU/TCG. Aceite HW T-052/053 pendente.
#   cargo check --release → 0 erros (36 warnings bin).
#   SESSION_303: T-001 BOOT_AI lock-free (k_nano 5×AtomicU32 + k_ai boot_metrics facade), serial BOOT_AI ok + EventBus BOOT_AI, verify após hydrate+ingest, tests 2/2+6/6+6/6.
#   SESSION_303b: 0077 §6 cindido em 4 (AS/syscall/WHPX+HW/register) + 0102 §9→10→11 + 0100 §6.3 duplicata removida; T-017 verified — usb_hw.img 3199MB regenerado 2026-09-01 (FALCON3.V6 788M, BOOT.LOG 255K, ESP 128M), limine-esp==uefi 772d3660; stale 31/08 resolvido.
#   SESSION_304 Onda 3 T-022..032 (R3 hermes/k_ai): T-022 serve_update.py evidência via ota.rs net_bridge + example payload; T-024 NET_READY+first_boot gate k_ai::provision ↔ hermes::provision (Installed+first_boot, 2/2 tests); T-026 Cron LogAgent POST /api/logs backoff 2000 ticks (≈110s) via cron try_log_push; T-028 [L]ive/[I]nstall 5s validado em hermes::agents InputAgent (boot_mode set); T-030 tries 3+last_good ChromeOS-like em hermes::ota OtaState + self_update parse_bootcfg (4 host tests); T-031 HITL note: MODELS_SOURCE=network default SÓ com OK maintainer (não implementado); T-032 A1 Ed25519 defer público mantido; cargo check 0 erros, hermes 177+5 new /k_ai 45 OK.
