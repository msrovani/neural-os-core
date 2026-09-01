# SESSION INDEX — neural-os-core v1.9.99 TEST

**Propósito:** Catálogo de sessões. A pasta viva `docs/memory/` mantém `SESSION_107+`; sessões históricas anteriores ficam em `docs/archive/sessions/`.

---

## Sessões Mantidas (107+)

| Sessão | Sprint | Bloco | Título | Principais Descobertas |
|---|---|---|---|---|
| 301 | Boot | #PF Fix | Kernel virtual range detection | Root cause: cr2-HHDM=140PB, correcto é kernel_phys+(cr2-kvirt); 0 #PFs; ATA FAT32 mount OK |
| 299 | Boot | Audit + ATA TCG fix + #PF fix | ATA probe em TCG habilitado; slog visibility; demand-page dual-range |
