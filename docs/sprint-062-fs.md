# Sprint 62 — InferênciaFS + MHI + Storage Agents

**v0.62.0** — Fusão entre MHI (Memory Hierarchy Index) e VFS (Virtual Filesystem).
Cada arquivo tem um **tier MHI** (DRAM/VRAM/NVMe/HDD/SDHC). Cada Storage Driver é um **Agent** com skills read/write. Arquivos "frios" migram para tiers lentos, "quentes" para rápidos.

Base: pesquisa cruzada de 10 fontes (arxiv, github, Plan 9, Redox, DAOS, NOVA, KSM).

---

## Arquitetura

```
User: /mnt/hdd/docs/report.txt
           ↓
    VFS Registry (resolve mount → "/mnt/hdd/" → AtaAgent)
           ↓
    MHI Registry (AllocProfile: access_count, tier, suggest_tier)
           ↓
    StorageAgent (ATA|USB-MSC) → Block device
           ↓
    Block I/O (read_sectors / write_sectors)

Virtual mounts bypass StorageAgent:
    /chat/ → HermesFsAgent (conversação)
    /dev/  → DevFsAgent (PCI, hardware)
    /proc/ → ProcFsAgent (agentes, memoria)
    /inference/ → InferenceFsAgent (LLM gera conteudo)
```

---

## Mapeamento

| Mount | Agent | MHI Tier | Driver |
|---|---|---|---|
| `/mnt/hdd/` | AtaAgent | HDD | ATA PIO |
| `/mnt/sdhc/` | UsbMscAgent | USB-MSC | xHCI + BOT |
| `/mnt/nvme/` | NvmeAgent | Nvme | NVMe (futuro) |
| `/mnt/ram/` | RamFsAgent | DRAM | heap alloc |
| `/mnt/vram/` | GpuFsAgent | VRAM | VirtIO-GPU (futuro) |
| `/chat/` | HermesFsAgent | Virtual | Hermes ring buffer |
| `/dev/` | DevFsAgent | Virtual | PCI scan |
| `/proc/` | ProcFsAgent | Virtual | AgentRegistry |
| `/inference/` | InferenceFsAgent | Virtual (LLM) | Cortex + KG |

---

## Sub-Sprints

### 62.1 — VFS Layer (~400 LOC) ✅ PARCIAL
- VfsRegistry (mount, resolve, lookup)
- VfsNode (arvore de diretorios)
- VfsMount (mount point → agent)
- Path utils (canonicalize, split, join)

**Status:** mod/vfs criado. Falta integrar com MHI e main.rs.

### 62.2 — MHI + FS Bridge (~300 LOC)
- `mhi.rs`: estender MhiRegistry com stat por VfsPath
- `mhi.rs`: `suggest_tier_for_path(path, access_profile)` → LLM ou heurística
- `vfs/mhi_bridge.rs`: conectar `resolve(path)` → `mhi.suggest_tier()`
- `profile.rs`: `UserProfile.resource_weights()` influencia suggest_tier

### 62.3 — Storage Agents (~600 LOC)
- `AtaAgent`: wraps `ata.rs` como DriverAgent com `read_sectors`/`write_sectors`
- `UsbMscAgent`: wraps `usb_msc.rs` como DriverAgent com CBW/CSW BOT
- `RamFsAgent`: arquivos em DRAM (heap) para cache de tiers inferiores
- Registra como agentes no boot

### 62.4 — Device + Process FS (~400 LOC)
- `DevFsAgent`: `/dev/pci/`, `/dev/rtl8139/`, `/dev/xhci/`
- `ProcFsAgent`: `/proc/agents`, `/proc/meminfo`, `/proc/uptime`
- `RamFsAgent`: `/mnt/ram/` — arquivos voláteis em DRAM

### 62.5 — InferenceFS (~400 LOC)
- `InferenceFsAgent`: `/inference/<path>` → LLM gera conteúdo sob demanda
- `KnowledgeGraphFileBridge`: writes sync com KG
- TTL-based re-inference: 60s stale → re-gera via LLM

### 62.6 — Auto Tier Migration (~300 LOC)
- MhiScheduler: a cada 1000 ticks, scan AllocProfiles
- Arquivos quentes (access_count > 10, last_access < 100 ticks) → promove p/ DRAM
- Arquivos frios (access_count < 3, last_access > 1000 ticks) → demove p/ HDD
- `mhi_tier as xattr`: cada VFSNode armazena tier atual

---

## Summary

| Sub | Feature | LOC | Prioridade | Depende |
|---|---|---|---|---|
| 62.1 | VFS Layer | ~400 | 🔴 Crítica | Nenhuma |
| 62.2 | MHI + FS Bridge | ~300 | 🔴 Crítica | 62.1, mhi.rs |
| 62.3 | Storage Agents | ~600 | 🟡 Alta | 62.1, ata.rs, usb-msc |
| 62.4 | Dev + Proc FS | ~400 | 🟢 Normal | 62.1 |
| 62.5 | InferenceFS | ~400 | 🟡 Alta | 62.1, Cortex, KG |
| 62.6 | Auto Tier Migration | ~300 | 🟡 Alta | 62.2, 62.3 |
| **Total** | **6** | **~2400** | | |
