# STATE — neural-os-core v1.9.99-s301 — Boot Fix: #PF + cognitive OOB
#   SESSION_301: 2 milestones this session:
#   1) #PF fix: kernel virtual ≠ HHDM — fórmula kernel_phys+(cr2-kvirt).
#   2) cognitive.rs OOB fix: per-layer head_dim derivado de q.shape.1.
#   ZERO #PFs, ZERO panics em boot QEMU TCG 4-core. FAT32 OK. SMP OK.
#   Training loop roda (lento no TCG ~2min). Model loaded OK.
#   Commits: b533364 (diagnostics), 67b4613 (#PF fix), 44f52e1 (docs),
#            6d18405 (cognitive OOB fix), e2f7a14 (cognitive docs).
