# SESSION INDEX — neural-os-core v1.9.99 TEST

**Propósito:** Catálogo de sessões. A pasta viva `docs/memory/` mantém `SESSION_107+`; sessões históricas anteriores ficam em `docs/archive/sessions/`.

---

## Sessões Mantidas (107+)

| Sessão | Sprint | Bloco | Título | Principais Descobertas |
|---|---|---|---|---|
| 305 | Boot/QEMU | P6 + Jarbas greeting | QEMU 4c loop PASS | int 0x90 DPL=3 na IDT k_nano; marker P6 @+48 pós-mailbox; ring3_can_iretq=true; saudacao+TTS+desktop_ready TCG |
| 302 | Ring3 | Onda 6 ADR-0102 | CPL=3 sandbox wired | k_nano::ring3 mailbox+T-056; P6 demos reais; register_native_ring gated T-053; 0 erros check |
| 301 | Boot | #PF Fix | Kernel virtual range detection | Root cause: cr2-HHDM=140PB, correcto é kernel_phys+(cr2-kvirt); 0 #PFs; ATA FAT32 mount OK |
| 299 | Boot | Audit + ATA TCG fix + #PF fix | ATA probe em TCG habilitado; slog visibility; demand-page dual-range |
