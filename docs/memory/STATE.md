# STATE — neural-os-core v1.9.99-s301 — #PF Fix: kernel virtual range
#   SESSION_301: Root cause do #PF corrigido — kernel virtual ≠ HHDM.
#   Fórmula correta: kernel_phys + (cr2 - kernel_virt).
#   ZERO #PFs em boot QEMU TCG 4-core. FAT32 mount OK. SMP OK.
#   Panic em cognitive.rs:770 (index OOB) — bug separado.
#   Commits: 456afab (ATA+slog), bb90042 (#PF dual-range), b533364 (diagnostics), 67b4613 (THE FIX).
