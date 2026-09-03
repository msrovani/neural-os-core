# STATE — neural-os-core v1.9.99-s306 — Dual QEMU 4c mesh Master/Worker (ADR-0081)
#   SESSION_306: 2× QEMU 4c TCG socket P2P — A=Master B=Worker+TOFU; netmode STATIC 10.0.3.2/3;
#   slog P2P/Net visível (ok); STATIC skip L3.5/L4/L5; detect candidatos ≥4GiB; target1/uefi sync.
#   Falcon3 dual NÃO — host ~5GB free. TICKV=RAM (NoDisk). Jarbas: cursor compositor-only + orb leve.
#   SESSION_305: QEMU 4c TCG loop exit 0 — saudacao+TTS Jarbas, ring3_can_iretq=true, NSGDB virtio-blk.
#   SESSION_302: isolamento CPL=3 wired em k_nano + bin facade (ADR-0102).
#   Aceite HW T-052/053 pendente. Dual Falcon + split IA = residual (RAM/host).
#   cargo check --release → gate pós-commit s306.
