# Sessão 062 — Sprint 62: VFS + MHI Bridge + Storage Agents

**Data:** 30/06/2026
**Versão:** v0.62.1
**Marco:** 🏆 **Primeiro VFS funcional — 8 mounts + 3 FS Agents + ARC tiering**

---

## O que entrou

### Sprint 62.1 — VFS Layer + MHI + Storage Agents

**VFS Foundation (v0.62.0):**
- `vfs/mod.rs`: VfsRegistry com mount, resolve, lookup, list_dir
- `vfs/mod.rs`: VfsNode (arvore de diretorios), VfsMount, FileMode (File, Directory, AgentMount, Virtual)
- `vfs/path.rs`: canonicalize(), split(), join(), filename(), parent(), match_mount()
- 8 mounts registrados no boot: /, /mnt/ram, /mnt/hdd, /mnt/sdhc, /chat, /dev, /proc, /system, /inference

**MHI ARC Evolution (v0.62.0):**
- `arc_suggest_tier()` — ZFS-ARC-inspired: MFU→Dram, MRU→Nvme, cold→Hdd
- `AllocTier::UsbMsc` — novo tier para USB Mass Storage
- `suggest_migration()` atualizado para usar arc_suggest_tier()

**Storage Agents (v0.62.1):**
- `fs/mod.rs`: FilesystemAgent trait + FS_AGENTS static registry + VFS bridge (read_vfs/write_vfs/list_vfs)
- `fs/ata_agent.rs`: AtaAgent — /mnt/hdd/sda (MBR), /mnt/hdd/sda1, /mnt/hdd/info
- `fs/dev_fs_agent.rs`: DevFsAgent — /dev/pci/list, /dev/pci/<vid:did>, /dev/rtl8139, /dev/xhci, /dev/mem
- `fs/proc_fs_agent.rs`: ProcFsAgent — /proc/agents, /proc/meminfo, /proc/uptime, /proc/cpuinfo, /proc/version, /proc/profile, /proc/mhi

### Planos de Sprint Criados
- `docs/sprint-061-desktop.md` — 6 sub-sprints, ~2800 LOC
- `docs/sprint-062-fs.md` — 6 sub-sprints, ~2400 LOC (atualizado com MHI)
- `docs/sprint-063-www.md` — 7 sub-sprints, ~2600 LOC

---

## Bugs Corrigidos na Sessão

### Bug 1: RTL8139 TSD_SIZE_SHIFT=16 (TX abortado)
- **Sintoma:** `tsd=0x30a000` com TABT=1, SIZE=0 nos bits 0-12
- **Causa raiz:** TSD_SIZE_SHIFT=16 colocava SIZE em bits 16-27, QEMU lia bits 0-12 = 0
- **Correção:** TSD_SIZE_SHIFT=0
- **LOC:** 1 linha

### Bug 2: RX buffer de 32KB alocado em 4KB
- **Sintoma:** `allocate_contiguous(8)` retorna 0 (fragmentado)
- **Causa raiz:** Frame allocator fragmentado por alocações de xHCI, etc.
- **Correção:** `init_driver_rtl8139()` chamado em kernel_main (antes da fragmentação)
- **LOC:** 3 linhas

### Bug 3: iPXE RX buffer — CAPR avancado
- **Sintoma:** CAPR=32752, rx_offset=0, nunca igualam
- **Causa raiz:** Bootloader (iPXE) preenche RX buffer antes do kernel
- **Correção:** `rx_offset = CAPR` após init
- **LOC:** 2 linhas

### Bug 4: e1000 Page Fault no MMIO
- **Sintoma:** `CR2=0xfebc0000` — Page Fault ao acessar BAR0 do e1000
- **Causa raiz:** PCI MMIO não mapeado nas page tables (bootloader só mapeia RAM)
- **Correção:** `map_page_uc()` — cria page table entries para MMIO
- **LOC:** ~60 linhas em `apic.rs`

---

## Decisões de Arquitetura

1. **MHI + FS fundidos**: Cada VfsMount tem AllocTier. ARC-style (ZFS) para sugestão de tier
2. **VRAM via namespace FS + raw ptr**: Catálogo via VFS, compute via ponteiro direto
3. **FilesystemAgent trait**: Interface padronizada para todos os FS agents (ata, dev, proc, inference, chat)
4. **ZFS-model portado**: ARC (Dram) → L2ARC (Nvme) → Pool (Hdd) mapeado para MHI tiers

---

## Pendente Técnico

- **Rede RX**: QEMU SLiRP não roteia sem DHCP. Bloqueia WWW agents.
- **InferenceFsAgent**: `/inference/` precisa de integração CortexAgent + KG
- **HermesFsAgent**: `/chat/send`, `/chat/last_response`
- **RamFsAgent**: `/mnt/ram/` — cache DRAM para tiers inferiores
- **Auto tier migration**: MhiScheduler promovendo/demovendo arquivos por acesso

---

## Estatísticas

| Métrica | Valor |
|---|---|
| Arquivos Rust | 89 → 96 (7 novos) |
| LOC total | 9.377 → 11.077 (+1.700) |
| Erros cargo check | 0 |
| Build size | 735 KB |
| Tags | v0.60.3 → v0.62.1 (5 tags) |
