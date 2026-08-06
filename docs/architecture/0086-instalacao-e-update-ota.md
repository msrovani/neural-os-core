# ADR-0086: Instalação e Update OTA — Processo Unificado (Consolidação ADR-0079 + ADR-0031 §1 + ADR-0074 + #308)

**Data:** 2026-08-05
**Status:** Accepted
**Lifecycle:** `fazendo`
**Inspirado por:** ADR-0079 (AutoInstaller) + ADR-0079-plan, ADR-0031 §1 (Self-Update A/B), ADR-0074 (git thin — referência, sem arquivo), IDEA_BANK #176, #306–310, #421, #417–423
**Sprint:** v1.9.9 TEST → v2.0.0
**Documentos fonte (canônicos, não substituídos):** `0079-neural-auto-installer.md`, `0079-neural-auto-installer-plan.md`, `0031-aios-self-update-wasm-jarvis.md`
**Nota de consolidação:** esta ADR **unifica e normaliza** o processo completo de instalação e update. As ADRs 0079/0031/0074 permanecem como histórico; este documento é o **processo canônico** (single source of truth do fluxo instalador + atualizador).
**Cenário-alvo de dev (2026-08-05):** note 1 (Windows, ICS, opencode, `serve_update.py`) compila e serve kernel/fw/`.bitnet`/skills; **note 2 (HW real) roda o OS instalado, monitora a rede e se auto-atualiza** (a quente p/ skills/fw/modelos; reboot p/ kernel via slot A/B). QEMU valida o pipeline; o note real valida HW (§3.4).

---

## 0. Inventário de fontes coletadas

| Fonte | Conteúdo | Estado |
|-------|----------|--------|
| **ADR-0079** | AutoInstaller Neural: migração pendrive→HD/SSD/NVMe com seleção por HW (modelo por RAM, firmware por PCI, WASM por CPU), MHI no lugar de swap, Limine, particionamento, `MODELS_SOURCE=network` | Accepted, `fazendo` |
| **ADR-0079-plan** | Plano de implementação: Fases 0–3 (~3.550 LOC), marcos M0–M4, topologia de execução, checklist por fase | Approved |
| **ADR-0031 §1** | Self-Update A/B dual-slot: referências ChromeOS/Android/CoreOS, design proposto (KERNEL~1/2 + BOOTCFG.JSON + Ed25519 + SHA-256 + tries/rollback), canais stable/nightly/security, fluxo UpdateAgent | **Superseded (parcial, WASM)** → §1 Self-Update permanece válido |
| **ADR-0074** | **Sem arquivo próprio** — existe apenas como referência: `crates/hermes/src/git_thin.rs` "git-over-HTTPS thin client (ADR-0074)" (codemap) + SESSION_241 (`git_thin.rs: Git refs fetch → tls::fetch_url`) + `apply_pack_bytes` bridge | Referência de código |
| **IDEA_BANK #421** | Instalador Neural com IA — SysInstaller pendrive→HD/SSD/NVMe | ✅ ADR-0079 M0–M4 |
| **IDEA_BANK #308a/b/c** | Update/Upgrade Agent (A/B slot + HTTP + Ed25519/SHA-256 + reboot); canais (stable/nightly/security, poll 3600/600/60s); rollback automático (3 tries → last_good) | ✅ parcial HTTP / 🟡 stub / ✅ parcial SelfHeal |
| **IDEA_BANK #176** | Ed25519 Cryptographic Identity (assinatura de kernel/atualização) | ✅ v0.50.0 |
| **IDEA_BANK #306a–d, #307** | Compat cross-OS (PE/ELF/Mach-O/APK) + syscall→skill — **fora do escopo** de instalação; documentado como contexto | ✅ parcial PE/ELF |
| **IDEA_BANK #309a–c, #310a/b** | WASM Runtime (wasmi), IDE, híbrido kernel/WASM; J.A.R.V.I.S. — relação via app_factory/instalador | ✅/⏳ |
| **IDEA_BANK #417–423** | Base de escrita (exFAT write, BlockDevice write, DiskIntelligence, FilesystemDriver, MHI, NeuralFS, GPU Direct Storage) — pré-requisitos de storage do instalador | ✅ parcial |

---

## 1. Problema (unificado)

O neural-os-core é um OS bare-metal que boota de pendrive USB (Limine UEFI + FAT32 de dados).
Dois fluxos de ciclo de vida:

1. **Instalar** — migrar do pendrive para o disco interno (HD/SSD/NVMe), ou replicar o OS entre
   máquinas, **sem ferramenta externa** e **sem copiar gigas** de firmware/modelos que o HW alvo não usa.
2. **Atualizar (OTA)** — entregar um kernel novo a uma máquina rodando o OS (ex: segundo notebook
   ligado por cabo ethernet), com **verificação de integridade** (hash/assinatura), **atomicidade**
   (slot inativo) e **rollback** (tries/last_good).

Cenário concreto de update: **note 1** (dev, Windows) compila o kernel novo e serve via HTTP;
**note 2** roda o OS instalado; ligados por cabo (ICS do Windows compartilha internet);
o note 2 **verifica 1×/dia** (skill) se há versão nova e se auto-atualiza.

---

## 2. Processo de INSTALAÇÃO (consolidado da ADR-0079 + plano)

### 2.1 Visão

O pendrive carrega **todas as peças** (kernel, modelos, firmwares, skills, configs). O instalador
**detecta o HW alvo**, seleciona **só o que aquele HW precisa**, particiona/formata, instala
bootloader + kernel + peças, e configura o sistema. A IA (HW Expert + Trinity MoE) decide:
modelo LLM por RAM, firmwares por PCI, variante WASM por CPU features, layout por tamanho do disco,
GPU offload por VRAM.

| Abordagem rejeitada | Problema |
|---------------------|----------|
| Sector copy (SysInstaller antigo) | Copia 64MB cegos; não entende partições nem instala bootloader |
| dd/clonezilla | Copia TUDO — inclusive firmware de HW ausente; desperdício |
| Package manager + download | Precisa de internet na instalação |
| Imagem monolítica | Cada HW exigiria imagem diferente |

### 2.2 Decisões de instalação (normalizadas)

| Decisão | Escolha | Justificativa |
|---------|---------|---------------|
| **Modelo por RAM** | 4-8GB→tiny; 8-16GB→2B; 16-32GB→7B; 32GB+→full stack (2B+BGE+rerank) | Override: GPU 4GB+ VRAM permite 7B com 8GB RAM |
| **Rerank** | Cross-encoder opcional: 4-8GB sem; 8-16GB leve; 16GB+ completo | Contexto mais limpo pro Cortex |
| **Firmware por PCI** | VGA NVIDIA→`FW_FECS_BL`+`FW_GPCCS`; VGA Intel→`FW_I915_*`; NIC Realtek→`FW_RTL_NIC_*`; WiFi Intel→`FW_IWLWIFI_*`; nenhum→pula | Sem firmware incompatível, sem falha de init |
| **WASM por HW** | Skill genérica → variante SIMD/soft-float/GPU/NPU/lite conforme CPUID + `app_factory` A/B/C (ADR-0059) | Otimização por máquina |
| **MHI substitui swap** | `VRAM ← DRAM ← NVMe ← SSD ← pendrive` (tiers 0-4). Swap só residual: RAM<4GB **e** HDD lento | MHI já existe (`k_nano/src/mhi.rs`) |
| **Bootloader: Limine** | Limine carrega kernel.elf de qualquer FS (FAT32/ext4/NeuralFS); fallback `/EFI/BOOT/BOOTX64.EFI` | Feature `limine-boot` existe; ClaudioOS provou (ADR-0062 §P4) |
| **Particionamento** | ≥32GB: ESP 512MB (FAT32) + NeuralFS (resto). <32GB: FAT32 única. HDD+RAM<4GB: swap residual | Decidido pelo PartitionPlanner |
| **Variante `MODELS_SOURCE=network`** | Opt-in: pendrive só kernel+firmware; 1º boot baixa os 8 modelos do ModelHub (ordem menor→maior: HwExpert→RustCoder→Reranker→Active 2B por último) | Pendrive menor; modelo cloud-init |

### 2.3 Arquitetura do instalador

```
AutoInstallerAgent (EventDriven, intent /install)
  ├── HwProfiler       PCI scan + HW Expert + RAM + CPUID
  ├── ModelPlanner     tiny/2B/7B/full stack por RAM
  ├── PartitionPlanner ESP+NeuralFS | FAT32 única | swap residual
  ├── GptWriter        reusa gpt::gpt_format_single()
  ├── FsFormatter      FAT32 + NeuralFS (reusa NeuralVolume::format)
  ├── BootloaderInstaller  Limine no ESP + limine.conf + kernel.elf
  ├── SmartFileCopier  firmware por HW + modelo por plano + WASM por CPU
  ├── ConfigGenerator  CONFIG.TXT + MHI config + GPU offload
  └── AI Advisor       (Fase 2) Cortex recomenda em linguagem natural
```

### 2.4 Estado atual do instalador (código)

| Componente | Status | Localização |
|-----------|--------|-------------|
| **SysInstaller M1** | ✅ existe, `scan_disks()` + `install(source,target,kernel_elf)` (GPT dual ESP+NeuralFS, copia kernel.elf → `/boot/kernel.elf`) | `crates/k_nano/src/sys_installer.rs` |
| **BlockDevice write** | ✅ ATA/AHCI/NVMe/USB escrevem | `k_nano/src/{ata,ahci,disk_agent/nvme,usb_msc}.rs` |
| **GPT format** | ✅ `gpt_format_single()` / `gpt_format_multi` | `k_nano/src/gpt.rs` |
| **FAT32 write / exFAT write** | ✅ root dir / opt-in `EXFAT_WRITE=1` | `fat32.rs` / `exfat_write.rs` |
| **NeuralFS** | ✅ `format()` + `write_file()` | `k_nano/src/neural_fs/volume.rs` |
| **ModelHub 8 slots** | ✅ `ModelSlot` + `register_bytes()` (ponto único de carga, ADR-0085 §7) | `cortex/src/model_hub.rs` |
| **HwProfiler/ModelPlanner/PartitionPlanner/FsFormatter/BootloaderInstaller/SmartFileCopier/ConfigGenerator** | ❌ novos (Fase 1 do plano) | `k_nano/src/hw_profiler.rs` etc. |
| **ModelProvisioner** | ❌ novo (variante §2.8) | `cortex/src/model_provisioner.rs` |
| **AutoInstallerAgent** | ⚠️ esqueleto existe | `k_nano/src/installer_agent.rs` |
| **InstallAdvisor (IA)** | ❌ Fase 2 | `cortex/src/install_adviser.rs` |
| **HW swap/recovery** | ❌ Fase 3 | `hw_change.rs`, `self_heal_disk.rs`, `net_fallback.rs` |

### 2.5 Marcos (plano ADR-0079)

| Marco | Critério |
|-------|----------|
| **M0** | SysInstaller funcional: `demo()` com NVMe/AHCI/USB PASS |
| **M1** | Instala partição + bootloader + kernel em QEMU com disco virtual |
| **M2** | **AutoInstaller MVP:** `install /dev/nvme0n1` com seleção inteligente + progresso Jarbas + reboot |
| **M3** (Fase 2) | Instalação com diálogo IA natural, rollback funcional |
| **M4** (Fase 3) | HW swap detection + self-recovery + NetFs fallback |

---

## 3. Processo de UPDATE OTA (consolidado da ADR-0031 §1 + #308 + ADR-0074)

### 3.1 Design A/B dual-slot (decisões normalizadas)

```
FAT32 Boot Partition (type 0x1C):
  KERNEL~1          ← slot ativo (bootloader carrega este)
  KERNEL~1.SIG      ← assinatura Ed25519 (futuro)
  KERNEL~2          ← slot inativo (alvo do update)
  KERNEL~2.SIG      ← assinatura Ed25519 (futuro)
  BOOTCFG~1         ← JSON { "boot_slot":"1|2", "kernel":"KERNEL~1|2", tries, last_good }
  BOOT.LOG          ← log de boot p/ BootSelfHealAgent
  UPDATE.MANIFEST   ← { "channel":"stable", "version":"1.9.10", "url":".../KERNEL.BIN" }
```

Decisões-chave (herdadas da ADR-0031 §1.4, normalizadas):

| Decisão | Escolha | Justificativa |
|---------|---------|---------------|
| **Verificação** | FNV-1a no fetch atual; **evolução**: SHA-256 + Ed25519 (SIG) + TPM PCR[8] | Kernel ~1MB (não 4GB rootfs) — hash completo é prático; sem dm-verity |
| **Sem partição KERN separada** | KERNEL~1/2 coexistem na FAT32 | FAT32 aguenta (~2MB) |
| **Sem recovery partition** | Bootloader é imutável e tiny; probabilidade de 2 slots Ed25519-assinados falharem ≈ 0 | Simplicidade |
| **Atomicidade** | Escreve SÓ no slot inativo; switch via BOOTCFG~1 (escrita de 512B) | Power-loss não corrompe o slot ativo |
| **Canais** | stable (poll 3600s), nightly (600s), security (60s), none (manual) | Risco crescente |
| **Rollback** | `tries` (default 3): boot falha → decrementa; 0 → cai p/ `last_good`; BootSelfHealAgent detecta crash e publica UPDATE_FAILED | Padrão ChromeOS/Android |

### 3.2 Fluxo UpdateAgent (normalizado da ADR-0031 §1.4 + código atual)

```
1. Check (skill diária / cron):
   a. Lê UPDATE.CFG na FAT32 → URL do servidor          (read_update_cfg)
   b. GET UPDATE.MANIFEST → { version, url }            (fetch_url → net_bridge/TLS)
   c. Compara semver com local (CARGO_PKG_VERSION)      (parse_version)
2. Se nova:
   a. GET KERNEL.BIN (chunked com hash por chunk — evolução)
   b. FNV-1a (atual) / SHA-256 + Ed25519 (evolução)      (fetch_update)
   c. Grava slot INATIVO via Fat32Writer                 (apply_update → KERNEL~2)
   d. Escreve BOOTCFG~1 apontando o slot novo            (switch_slot)
   e. Loga "reboot p/ ativar slot"
3. Reboot → bootloader lê BOOTCFG~1 → carrega slot novo
4. Boot OK   → BootSelfHealAgent marca last_good = slot novo
5. Boot FALHOU (3 tries) → fallback last_good + UPDATE_FAILED → rollback
```

### 3.3 ADR-0074 — git thin client (update via git-over-HTTPS)

Sem arquivo ADR; implementação em `crates/hermes/src/git_thin.rs` + bridge `apply_pack_bytes`
(`hermes/src/self_update.rs`):

- `fetch_refs(repo_https)` → `GET {base}/info/refs?service=git-upload-pack` via `tls::fetch_url`
- `build_want_done(sha40)` → upload-pack want×1 + done (smart HTTP)
- `find_pack()` / `pack_object_count()` → localiza PACK na resposta sideband
- `apply_pack_bytes(pack_or_blob)` → se PACK, `apply_thin_pack`; senão blob cru → slot inativo
- **Verificação:** FNV-1a do blob; inflate via `miniz_oxide` (no_std)
- **MVP:** info/refs + 1 pack shallow (want×1) → inflate 1º blob. Sem push/index pleno/delta resolve

### 3.4 Topologias de teste (QEMU e HW real como testador)

O ciclo de dev usa **duas topologias**:

**A. QEMU (pipeline)** — um único QEMU simula os dois "computadores" como discos virtuais:

```
index=0  uefi.img              ← "pendrive" (ESP Limine + kernel.elf)
index=1  disk_qemu.raw         ← "dados pendrive" (FAT32: modelos, UPDATE.CFG)
index=2  install_target.raw    ← "HD do PC alvo" (vazio) ← SysInstaller instala AQUI
-netdev user (10.0.2.2 = host serve_update.py)
```

- Ato 1: boot do pendrive → `/install` → SysInstaller escreve GPT dual no `target.raw`
- Ato 2: reiniciar QEMU com `target.raw` no index=0 → rede → ModelProvisioner baixa modelos
- Ato 3: reboot → intelligence completa
- **Nota:** boot do alvo exige reordenar drives (OVMF boota do 1º disco EFI); não há hot-swap de disco no mesmo QEMU

**B. HW real (note 2 como testador)** — topologia-alvo do dev:

```
NOTE 1 — DEV (Windows)                          NOTE 2 — TARGET REAL (HW)
┌──────────────────────────────┐  cabo ICS    ┌────────────────────────────────┐
│ opencode + repo + cargo      │  ethernet    │ neural-os-core INSTALADO no HD │
│ compila kernel/fw/.bin/.bitnet│────────────▶│  [instalado via SysInstaller]  │
│ serve_update.py :8080        │              │  │                             │
│  /KERNEL.BIN /FW_* /MODEL.BIN│              │  ├─ monitora a rede (poll)     │
│  /UPDATE.MANIFEST            │              │  ├─ detecta atualização →      │
└──────────────────────────────┘              │  │  skill update_check         │
                                              │  │  a quente (skills/fw) ou    │
                                              │  │  reboot (kernel: slot A/B)  │
                                              └────────────────────────────────┘
```

- Note 1 serve todos os artefatos (kernel, firmware, `.bitnet`, skills) via `serve_update.py`
- Note 2 roda o OS instalado, monitora a rede, e aplica:
  - **a quente** (sem reboot): skills WASM, firmware, modelos → `register_bytes()` + hot-swap
  - **com reboot**: kernel → slot inativo → `switch_slot` → reboot
- **Bloqueio (U6):** o pipeline de update hoje só reconhece FAT32/MBR — não funciona no disco GPT instalado (ver §5 U6)

---

## 4. Implementação desta sessão (SESSÃO 2026-08-05 — disparo diário)

| Peça | Arquivo | Estado |
|------|---------|--------|
| Servidor OTA on-demand | `tools/serve_update.py` (novo): `UPDATE.MANIFEST` + `KERNEL.BIN` (:8080, `--version/--kernel/--base-url`) | ✅ testado (200/200/404) |
| Config na imagem | `tools/mkfat32.py` — grava `UPDATE.CFG` (env `UPDATE_URL` override, default `10.0.2.2:8080`) | ✅ |
| Leitura config | `hermes/src/self_update.rs::read_update_cfg()` (FAT32, mesmo padrão do active_slot) | ✅ |
| Rotina diária | `check_for_update()` — config → manifest → semver → fetch slot | ✅ |
| Parse semver/JSON | `json_field()` + `parse_version()` (no_std, sem serde) | ✅ + 2 testes host |
| Skill | `UpdateCheckSkill` (registrada no bin) | ✅ |
| Cron diário | `hermes/src/cron.rs` job `update_check` (86400×TIMER_HZ ticks, inline) | ✅ |
| Build/Teste | `cargo check --release` 0 erros; `cargo test -p hermes --lib self_update` 2/2 PASS | ✅ |

**Integração com o legado:** o `check_for_update` usa o `SelfUpdate` existente (`fetch_update`,
`apply_update`, `switch_slot`, `active_slot`) — o mecanismo A/B da ADR-0031/#308 não foi reescrito,
foi **disparado** pela skill.

### 4.1 Verificação estrutural do instalador (SESSÃO 2026-08-05)

Cruzamento ADR × código real revelou estado **diferente do que a ADR-0079 §4 e o plano previam**:

| Componente | ADR-0079 §4 dizia | Realidade no código | Conclusão |
|-----------|-------------------|---------------------|-----------|
| SysInstaller | "não linkado, 262 LOC" | **linkado** (`lib.rs:97`), **304 LOC**, `demo()` M1 PASS | ✅ além do previsto |
| HwProfiler | "novo (Fase 1)" | **existe e linkado** (`hw_profiler.rs`, `profile_hardware()`) | ✅ Fase 1 adiantada |
| AutoInstallerAgent | "esqueleto" | **`run_install()` completo** (profile→GPT→install→skills catalog→progresso) | ✅ mas órfão (I6) |
| self_check / rollback / hw_change (Fase 3) | "❌ Fase 3" | **existem e linkados** (`self_check.rs`, `rollback.rs`, `hw_change.rs`) | ✅ Fase 3 adiantada |
| `format_fat32_esp` | "FAT32 format não existe" | **existe** (`fat32.rs:1043`) | ✅ I2 corrigido |
| ModelPlanner / PartitionPlanner / SmartFileCopier / ConfigGenerator / BootloaderInstaller | "Fase 1" | **não existem** | ❌ pendente (I1) |

**Descoberta bloqueante (U6):** o pipeline de update (`active_slot`, `write_kernel`, `write_bootcfg`,
`read_update_cfg`) filtra partições FAT32 `0x0B/0x0C/0x1C`, mas o disco **instalado pelo SysInstaller**
é GPT dual: ESP → `0xEF` + NeuralFS → `0x7F` (mapeamento em `fat32.rs:127-134`). **No note 2 real
instalado, o update não encontra UPDATE.CFG nem slots** — o fluxo note1→note2 (§3.4B) está bloqueado
até o filtro aceitar `0x7F`/`0xEF` e os slots viverem na NeuralFS.

---

## 5. Gaps unificados (instalação + update)

| # | Gap | Origem | Próximo passo |
|---|-----|--------|---------------|
| U1 | **Elo de boot**: update grava KERNEL~1/2 na FAT32 de dados, mas Limine carrega kernel.elf da ESP | Sessão atual | limine.conf/boot path ler slot apontado por BOOTCFG~1 |
| U2 | **Trigger imediato**: 1º check só após 24h de uptime | Sessão atual | comando shell `update` chamando `check_for_update()` |
| U3 | **Assinatura/TPM**: fetch atual só FNV-1a; Ed25519 (SIG) + TPM PCR[8] previstos na ADR-0031 | ADR-0031 §1.4 | add SHA-256/Ed25519 no fetch + TPM extend |
| U4 | **Rollback automático** não testado no boot | ADR-0031 / #308c | watchdog de boot (3 tries → last_good) |
| U5 | Config file fixo (UPDATE.CFG) | Sessão atual | service discovery / mesh (ADR-0081) quando modelos |
| **U6** | **Update não funciona no disco GPT instalado** — pipeline só reconhece FAT32/MBR | Verificação 2026-08-05 | estender filtro p/ NeuralFS `0x7F` (+ESP `0xEF` FAT32); slots na NeuralFS |
| I1 | **Fases 1–3 do instalador** (ModelPlanner, PartitionPlanner, SmartFileCopier, ConfigGenerator, BootloaderInstaller) não implementadas | ADR-0079-plan | seguir Fases 0→3, marcos M0–M4 |
| I2 | ~~FAT32 format não implementado~~ → **corrigido**: `format_fat32_esp` existe | ADR-0079-plan 1.4 | ~esforço já coberto por `fat32.rs:1043` |
| I3 | **Limine no target**: SysInstaller copia a ESP crua por setor (sem escrever limine.conf específico do alvo) | ADR-0079-plan 1.5 | BootloaderInstaller dedicado + limine.conf do target |
| I4 | **ModelProvisioner** (`MODELS_SOURCE=network`) não implementado | ADR-0079 §2.8 | `cortex/src/model_provisioner.rs` |
| I5 | **Leitura NeuralFS de modelos** no boot (hoje só FAT32/exFAT) | ADR-0079 §2.8 | estender load sites |
| I6 | **AutoInstallerAgent órfão**: existe e é linkado (`lib.rs:98`), mas **não registrado no boot** — ninguém publica SYS_INSTALL | Verificação 2026-08-05 | registrar no AgentFleet + intent `/install` |
| I7 | **`hw_profiler::gpu_vram_mb = 2048` hardcoded** p/ NVIDIA — decisão "GPU 4GB+ VRAM → 7B" depende de VRAM real | Verificação 2026-08-05 | decode de tamanho de BAR0 VRAM |
| I8 | **`self_check::verify_install_checksum` é placeholder** (sem walk de diretório) | Verificação 2026-08-05 | implementar walk /boot + compare |

---

## 6. Verificação

- `cargo check --release` — 0 erros.
- `cargo test -p hermes --lib self_update` — 2/2 PASS (extração de campos do manifest;
  ordenação semver incluindo pré-release e major bump).
- `tools/serve_update.py` smoke — `GET /UPDATE.MANIFEST` 200, `GET /KERNEL.BIN` 200
  (17.159.896 bytes do kernel.elf real), path desconhecido 404.
- SysInstaller (instalação): demo self-test com MemoryDisk — ver ADR-0079 §4.
- **Verificação estrutural** (SESSÃO 2026-08-05): 6 módulos da ADR-0079 §4 foram conferidos
  contra o código real — 4 existem e estão linkados (SysInstaller, HwProfiler,
  AutoInstallerAgent, self_check/rollback/hw_change), 5 não existem (ModelPlanner,
  PartitionPlanner, SmartFileCopier, ConfigGenerator, BootloaderInstaller); **U6 descoberto**
  (update só fala FAT32/MBR, não GPT/NeuralFS do disco instalado).
- Próximo: smoke QEMU do fluxo OTA completo (servidor host :8080 → guest `10.0.2.2:8080` →
  `check_for_update` → slot) — depende do gap U2 (trigger imediato) e, para o note real, de U6.

---

## 7. Referências cruzadas

- **ADR-0079** + **0079-plan**: **deprecadas 2026-08-05 (processo → esta ADR §2)**; mantidas como referência de design/riscos e plano de trabalho detalhado (Fases 0–3, marcos M0–M4)
- **ADR-0031 §1**: **deprecado 2026-08-05 (§1 Self-Update → esta ADR §3)**; mantido como histórico/referência de design (também superseded no tema WASM → ADR-0059)
- **ADR-0074**: git thin client — referência de código (`git_thin.rs`), sem arquivo; consolidado na §3.3
- **IDEA_BANK**: #176, #308a/b/c, #421 (deprecados → esta ADR); #417–423 (pré-requisitos storage); #306a–d/#307/#309a–c/#310a/b (contexto cross-OS/WASM/J.A.R.V.I.S.)
- **ADR-0059**: app_factory A/B/C (WASM por HW no instalador)
- **ADR-0081**: mesh P2P (evolução futura de discovery do update server)
- **ADR-0085 §7**: ModelHub `register_bytes` (ponto único de carga de modelos)
- **Código**: `k_nano/src/sys_installer.rs`, `k_nano/src/installer_agent.rs`,
  `hermes/src/self_update.rs`, `hermes/src/git_thin.rs`, `hermes/src/cron.rs`,
  `tools/serve_update.py`, `tools/mkfat32.py`
