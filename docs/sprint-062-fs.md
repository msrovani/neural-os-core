# Sprint 62 — Inference Filesystem

**v0.62.0** — Files are inferred/predicted rather than stored. Memory-mapped files ↔ Knowledge Graph. FS emulator agents (FAT32, NTFS, ext3, exFAT) with read/write skills. Virtual Filesystem Layer with mount table, path resolution, inode management. The Hermes Chatbot as a filesystem (`/chat/`). Device filesystem (`/dev/`) and process filesystem (`/proc/`).

**Base research:** AIFS (arxiv 2405.13372), NeuralDB (arxiv 2305.13149), vexfs (vectara filesystem), Redox OS SchemeHandler trait, Plan 9 /proc, Linux sysfs.

---

## Legenda
| Símbolo | Significado |
|---|---|
| 🟢 Fácil | < 100 LOC, sem dependências externas |
| 🟡 Médio | 100-300 LOC, depende de 1-2 módulos existentes |
| 🔴 Pesado | 300-700 LOC, módulo novo ou pesquisa |
| ⚫ Bloqueado | Depende de HW ou ambiente externo |

---

## Research References

| Paper/Project | Concept | Applicability |
|---|---|---|
| **AIFS: AI-native File System** (arxiv 2405.13372) | File read = LLM query. File write = training example. Files are "inferred" not stored. | Core concept of InferenceFS |
| **NeuralDB** (arxiv 2305.13149) | Database that learns from queries. Hybrid vector + relational. | Memory-mapped files ↔ Knowledge Graph |
| **vexfs** (github/lspecian/vexfs) | Linux FUSE filesystem with vector search via xattr, ChromaDB-compatible API | Vector search as filesystem operations |
| **Redox OS Scheme** (doc/redox-os.org) | `scheme://` namespace: `tcp://`, `file://`, `debug://`. Every resource is a file. | Architecture: FS emulator as Agent trait |
| **Plan 9 /proc** | Process filesystem — `ps` as `cat /proc/*/status` | ProcFS template |
| **Linux sysfs / devfs** | Hardware as files: `/dev/sda`, `/sys/class/net/eth0` | DevFS template |
| **PAI File System as Context** (#179) | Filesystem as index of knowledge, grep instead of RAG | /chat/ FS concept |
| **IDEA #279c** | Filesystem próprio com permissões (substitui FAT12) | VFS + permission model |

---

## Sub-Sprint 62.1 — Virtual Filesystem Layer (VFS)

**Target:** v0.62.1 | **LOC:** ~500 | **Dependências:** Nenhuma (pure no_std data structures)

### Feature: VFS Mount Table + Path Resolution
- LOC: ~500
- Dependencies: None (core data structures)
- Implementation:
  1. Create `vfs/mod.rs`:
     - `struct VfsNode` — `{ name: [u8; 256], mode: FileMode, inode: u64, size: u64, children: BTreeMap<...>, agent: Option<&'static str> }`
     - `enum FileMode` — `File, Directory, Symlink, AgentMount, Virtual`
     - `struct VfsMount` — `{ mount_point: &'static str, agent_name: &'static str, flags: MountFlags }`
  2. `VfsRegistry`:
     - `mount_table: Vec<VfsMount>` — ordered longest-prefix-first
     - `resolve(path) -> (VfsMount, relative_path)` — match mount point, return owning agent
     - `lookup(path) -> Option<&VfsNode>` — in-memory directory tree
  3. Path resolution: `/foo/bar/baz` → longest prefix match mount → delegate to FS agent
  4. Standard mounts pre-registered at boot:
     - `/` → built-in root (minimal static tree)
     - `/chat/` → HermesAgent
     - `/dev/` → DevFS agent
     - `/proc/` → ProcFS agent
     - `/system/` → SystemFS agent
- Files to create/modify:
  - `crates/neural-kernel/src/vfs/mod.rs` (new)
  - `crates/neural-kernel/src/vfs/node.rs` (new)
  - `crates/neural-kernel/src/vfs/path.rs` (new — path parser, canonicalize)
  - `crates/neural-kernel/src/lib.rs` (add mod vfs)
  - `crates/neural-kernel/src/kernel_main.rs` (init VFS early boot)

### Feature: Inode Management
- LOC: ~150
- Dependencies: VfsRegistry
- Implementation:
  1. `InodeAllocator` — simple bump allocator for inode numbers (u64)
  2. `Inode` struct — `{ id, size, block_count, mode, uid, gid, atime, mtime, ctime }`
  3. Optional: `extended_attrs: BTreeMap<&'static str, Vec<u8>>` for vexfs-style vector search xattr
- Files to create/modify:
  - `crates/neural-kernel/src/vfs/inode.rs` (new)
  - `crates/neural-kernel/src/vfs/mod.rs`

---

## Sub-Sprint 62.2 — InferenceFS (Core Innovation)

**Target:** v0.62.2 | **LOC:** ~350 | **Dependências:** 62.1 (VFS), CortexAgent (LLM)

### Feature: Inference File Read/Write
- LOC: ~350
- Dependencies: VFS, CortexAgent
- Implementation:
  1. Create `vfs/inference_fs.rs`:
     - `InferenceFsAgent` — implements `Agent` trait, registered as mount handler for `/inference/`
     - `read(path)` → `LLM_REQUEST` → LLM generates file content on-the-fly
     - `write(path, data)` → stores as training example in memory ring buffer
  2. Inference semantics:
     - `cat /inference/weather_today` → LLM generates: "Sunny, 32°C, humidity 45%"
     - `cat /inference/network_status` → LLM reads current packet stats and generates summary
     - `echo "correct answer" > /inference/weather_today` → fine-tune hint stored
  3. Training buffer: `VecDeque<(PathBuf, Vec<u8>)>` — last 1000 write operations
  4. Periodically flushed to `MemPalace` Knowledge Graph as training triples
- Files to create/modify:
  - `crates/neural-kernel/src/vfs/inference_fs.rs` (new — InferenceFsAgent)
  - `crates/neural-kernel/src/vfs/mod.rs`
  - `crates/neural-kernel/src/cortex/agent.rs` (add `InferenceGenerate` skill)
  - `crates/neural-kernel/src/agents.rs` (register InferenceFsAgent)

### Feature: Knowledge Graph ↔ File Mapping
- LOC: ~200
- Dependencies: InferenceFS, MemPalace KG (existing)
- Implementation:
  1. When writing to `/inference/<path>`, also add KG fact: `(path, "has_value", value)`
  2. When reading `/inference/<path>`, check KG for existing fact → return if fresh
  3. `KnowledgeGraphFileBridge` — syncs file writes to KG and vice versa
  4. TTL-based re-inference: if file older than 60s, re-generate via LLM
- Files to create/modify:
  - `crates/neural-kernel/src/vfs/kg_bridge.rs` (new)
  - `crates/neural-kernel/src/vfs/mod.rs`

---

## Sub-Sprint 62.3 — FS Emulator Agents (FAT32, NTFS, ext3, exFAT)

**Target:** v0.62.3 | **LOC:** ~800 | **Dependências:** 62.1 (VFS), Storage driver

### Feature: FAT32 DriverAgent
- LOC: ~300
- Dependencies: VirtIO-blk or NVMe or USB-MSC, VFS
- Implementation:
  1. Create `fs/fat32.rs` — FAT32 parser (BPB, FAT table, root directory, cluster chain)
     - `struct Fat32Volume` — `{ bpb, fat_table: Vec<u32>, root_dir: Vec<DirEntry> }`
     - `read_cluster(clus_no) -> Vec<u8>` — sector-based cluster read
     - `walk_path(path) -> Option<DirEntry>` — directory tree traversal
  2. `Fat32Agent` — wraps Fat32Volume as DriverAgent:
     - `tick()` — no-op (polling not needed)
     - `on_request()` — handle `read(path)`, `write(path, data)`, `list_dir(path)` via EventBus
  3. Mount point: `/mnt/fat32/`
- Files to create/modify:
  - `crates/neural-kernel/src/fs/fat32.rs` (new)
  - `crates/neural-kernel/src/fs/mod.rs` (new)
  - `crates/neural-kernel/src/agents/driver/fat32_agent.rs` (new)
  - `crates/neural-kernel/src/lib.rs`

### Feature: NTFS DriverAgent (Stub + Basic Read)
- LOC: ~250
- Dependencies: Storage driver, VFS
- Implementation:
  1. Parse NTFS $MFT (Master File Table) — locate `$AttrDef`, `$Bitmap`, `$UpCase`
  2. Read path: MFT entry → $DATA attribute → runlist → cluster reads
  3. Read-only initially. Write support in v0.62.4+
  4. Mount point: `/mnt/ntfs/`
- Files to create/modify:
  - `crates/neural-kernel/src/fs/ntfs.rs` (new)
  - `crates/neural-kernel/src/fs/mod.rs`

### Feature: ext3 + exFAT DriverAgent Stubs
- LOC: ~250
- Dependencies: Storage driver, VFS
- Implementation:
  1. `ext3.rs` — parse superblock, group descriptors, inode table, directory entries
     - Read-only: `read_inode(inum) -> Inode`, `read_dir(dir_inum) -> Vec<DirEntry>`
     - Mount point: `/mnt/ext3/`
  2. `exfat.rs` — parse exFAT VBR, FAT table, directory entry sets
     - Read-only: cluster-chain traversal, name hashing (upcase table)
     - Mount point: `/mnt/exfat/`
- Files to create/modify:
  - `crates/neural-kernel/src/fs/ext3.rs` (new)
  - `crates/neural-kernel/src/fs/exfat.rs` (new)
  - `crates/neural-kernel/src/fs/mod.rs`

---

## Sub-Sprint 62.4 — Hermes Chat as Filesystem (/chat/)

**Target:** v0.62.4 | **LOC:** ~250 | **Dependências:** 62.1 (VFS), HermesAgent

### Feature: /chat/ Virtual Filesystem
- LOC: ~250
- Dependencies: VFS mount, HermesAgent
- Implementation:
  1. Register mount: `/chat/` → `HermesFsAgent`
  2. File layout:
     - `/chat/send` — write-only: writing a message sends it to LLM
     - `/chat/last_response` — read-only: last LLM response
     - `/chat/history` — read-only: full conversation history (last 50 exchanges)
     - `/chat/history/N` — read-only: Nth exchange
     - `/chat/clear` — write-only: writing anything clears history
     - `/chat/context` — read-write: system prompt context
  3. `HermesFsAgent`:
     - Implements `Agent` trait with `scheme: "chat"`
     - VfsMount calls `read(path)` and `write(path, data)` on this agent
     - Backed by HermesAgent's conversation ring buffer
  4. Enables shell workflows: `echo "hello" > /chat/send ; cat /chat/last_response`
- Files to create/modify:
  - `crates/neural-kernel/src/agents/hermes_fs_agent.rs` (new)
  - `crates/neural-kernel/src/agents/mod.rs`
  - `crates/neural-kernel/src/hermes/ring_buffer.rs` (extract existing buffer to reusable struct)

---

## Sub-Sprint 62.5 — Device Filesystem (/dev/)

**Target:** v0.62.5 | **LOC:** ~250 | **Dependências:** 62.1 (VFS), PCI scan (existing)

### Feature: /dev/ Virtual Filesystem
- LOC: ~250
- Dependencies: VFS mount, PCI scanner, HwRegistry (existing)
- Implementation:
  1. Register mount: `/dev/` → `DevFsAgent`
  2. File layout (dynamic, populated at boot from PCI scan):
     - `/dev/pci/NNNN:NNNN` — one file per device (vendor:device)
     - `/dev/pci/class/XX/` — directory per class code
     - `/dev/rtl8139/` — network device status
     - `/dev/xhci/` — USB controller status
     - `/dev/virtio-gpu/` — GPU registers
     - `/dev/mem` — physical memory window (read-only, restricted)
     - `/dev/port` — I/O port access (future, restricted)
  3. `DevFsAgent`:
     - On boot, iterates `HwRegistry` to build file tree
     - `read("/dev/pci/8086:100E")` → returns JSON-like descriptor
     - Dynamic: re-scans on hotplug event (future)
- Files to create/modify:
  - `crates/neural-kernel/src/agents/dev_fs_agent.rs` (new)
  - `crates/neural-kernel/src/agents/mod.rs`

---

## Sub-Sprint 62.6 — Process Filesystem (/proc/)

**Target:** v0.62.6 | **LOC:** ~200 | **Dependências:** 62.1 (VFS), AgentRegistry

### Feature: /proc/ Virtual Filesystem
- LOC: ~200
- Dependencies: VFS mount, AgentRegistry
- Implementation:
  1. Register mount: `/proc/` → `ProcFsAgent`
  2. File layout:
     - `/proc/agents` — list of all registered agents (name, kind, state, ticks)
     - `/proc/agents/<name>` — per-agent status
     - `/proc/cpuinfo` — CPU topology from PerCpu data
     - `/proc/meminfo` — memory usage (heap, frame allocator, MHI tiers)
     - `/proc/uptime` — ticks since boot
     - `/proc/interrupts` — IRQ counters (when IRQ tracking implemented)
     - `/proc/version` — kernel version string
     - `/proc/self` — agent querying its own info (symlink to /proc/agents/<current>)
  3. `ProcFsAgent`:
     - `read(path)` → serializes kernel data into text
     - All data is live-queried (no caching)
- Files to create/modify:
  - `crates/neural-kernel/src/agents/proc_fs_agent.rs` (new)
  - `crates/neural-kernel/src/agents/mod.rs`

---

## Summary

| Sub-Sprint | Feature | LOC | Prioridade | Dependências |
|---|---|---|---|---|
| 62.1 | VFS Layer + Mount Table | ~650 | 🔴 Crítica | Nenhuma |
| 62.2 | InferenceFS Core | ~550 | 🔴 Crítica | 62.1, CortexAgent |
| 62.3 | FS Emulator Agents (4) | ~800 | 🟡 Alta | 62.1, Storage driver |
| 62.4 | /chat/ Filesystem | ~250 | 🟡 Alta | 62.1, HermesAgent |
| 62.5 | /dev/ Filesystem | ~250 | 🟢 Normal | 62.1, PCI scan |
| 62.6 | /proc/ Filesystem | ~200 | 🟢 Normal | 62.1, AgentRegistry |
| **Total** | **6 features** | **~2700 LOC** | | |

### Implementation Order
```
62.1 (VFS) → 62.2 (InferenceFS) → 62.4 (/chat/) → 62.3 (FS Emulators)
                                    62.5 (/dev/) → 62.6 (/proc/)
```

62.1 is the absolute foundation — all other sub-sprints depend on it. 62.2 (InferenceFS) is the flagship feature and should be the first consumer of VFS. 62.4 (/chat/) and 62.5+62.6 (`/dev/` and `/proc/`) can be developed in parallel after 62.1. 62.3 (FS emulator agents) depends on storage drivers (VirtIO-blk/NVMe) which may be blocked on hardware availability.

### Key Innovations
1. **InferenceFS** — files that generate their content via LLM on read. Write = training feedback.
2. **KG ↔ File Bridge** — every file operation syncs to the Knowledge Graph, making all file data queryable semantically.
3. **Agent as Filesystem** — every mount point is an Agent with read/write skills. The VFS is a router, not a storage layer.
4. **Chat as Filesystem** — `/chat/send` + `/chat/last_response` enables Unix-philosophy message passing via shell.
