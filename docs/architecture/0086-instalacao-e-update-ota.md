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
| **AutoInstallerAgent** | ⚠️ **existe com `run_install()` completo, mas órfão** (não registrado no boot — gap I6) | `k_nano/src/installer_agent.rs` |
| **InstallAdvisor (IA)** | ❌ Fase 2 | `cortex/src/install_adviser.rs` |
| **HW swap/recovery (Fase 3)** | ⚠️ **existem e linkados** (`self_check`, `rollback`, `hw_change`) — com placeholders (I8) | `k_nano/src/{self_check,rollback,hw_change}.rs` |

### 2.5 Marcos (plano ADR-0079)

| Marco | Critério |
|-------|----------|
| **M0** | SysInstaller funcional: `demo()` com NVMe/AHCI/USB PASS |
| **M1** | Instala partição + bootloader + kernel em QEMU com disco virtual |
| **M2** | **AutoInstaller MVP:** `install /dev/nvme0n1` com seleção inteligente + progresso Jarbas + reboot |
| **M3** (Fase 2) | Instalação com diálogo IA natural, rollback funcional |
| **M4** (Fase 3) | HW swap detection + self-recovery + NetFs fallback |

### 2.6 Detecção de modo de boot: instalador vs sistema (decisão 2026-08-05)

O OS precisa saber **de onde bootou** para escolher o fluxo. Hoje **não há detecção em runtime**
(o kernel nunca lê `CONFIG.TXT`; `rollback::detect_boot_source` existe mas é órfão).

**Sinal escolhido: assinatura do disco de boot (GPT), não o tipo de barramento.**
USB vs ATA é ambíguo (o note real pode bootar de USB; o pendrive é USB). O discriminante
robusto é o layout do boot device:

| Sinal | Pendrive (instalador/live) | Sistema instalado |
|---|---|---|
| GPT do boot device | FAT32 única (dados) | **GPT dual: ESP + NeuralFS** (assinatura do SysInstaller) |
| CONFIG.TXT | `BOOT_MODE=install\|live\|auto` | — (mode = sistema) |
| Modelos | leitura FAT32 (se houver) | NeuralFS `/models/` |
| Update | **não busca** (é a fonte) | busca no server (note 1) |

**Decisões:**

1. **`boot_media::mode()`** — função única lendo CONFIG.TXT + GPT do boot device; todos os
   consumidores (boot, instalador, update) consultam um só ponto.
2. **Pendrive → MODO INSTALADOR**: não carrega modelos (só HwExpert + tiny p/ o instalador);
   pergunta "em qual HD instalar?" (`scan_disks` → lista); SysInstaller → GPT dual → reboot.
   **Não busca atualização** — é a fonte, não o alvo.
3. **Sistema instalado → MODO SISTEMA**: carrega modelos da NeuralFS; verifica update
   (a quente: `.bitnet`/fw/skills via `register_bytes`+hot-swap; a frio: kernel → slot A/B → reboot).
   Depende do gap **U6** (update falar GPT/NeuralFS).
4. **Menu live/install no boot do pendrive** (UX):
   ```
   [L] Live  — default (timeout ~5s; boot degradado, comportamento atual)
   [I] Instalar no disco — destrutivo; exige tecla
   ```
   - **Default = Live + timeout**: instalação é destrutiva; boot não pode travar sem teclado.
   - **`CONFIG.TXT` pré-definido** (`BOOT_MODE=install|live|auto`): menu só em `auto`;
     `install` direto = headless (útil no fluxo de dev).
   - **Instalar NUNCA no disco de boot atual**: validação `target ≠ source` no SysInstaller.
5. **Live não é modo novo** — é o comportamento atual (pendrive boota degradado sem modelos);
   o menu só roteia.

**Gap relacionado:** kernel não lê CONFIG.TXT hoje → criar `boot_media::mode()` (novo gap I9).

### 2.7 Princípio AIOS: auto-consciência de localização + domínio do silício

⚠️ **Este é um AIOS — o fluxo de instalação/update não é um `if (usb) … else …` mecânico.**
O sistema deve **saber onde está e o que pode fazer** em cada fase, e — quando encontra sua
**casa definitiva** — **se adaptar ao silício** em vez de apenas rodar. Isso conecta o ciclo de
vida ao resto da inteligência que já existe (HwExpert, OptimizerAgent, MHI, hw_change, SelfHeal).

**Estado cognitivo em 3 fases (o OS pergunta "quem sou eu?")**:

| Fase | Onde estou | O que posso fazer | Inteligência ativa |
|------|-----------|-------------------|--------------------|
| **Visitante** (pendrive live) | Disco removível, FAT32 | Provar o OS, inspecionar HW | HwExpert (identifica), parcial |
| **Mensageiro** (pendrive instalador) | Disco removível, FAT32 | **Instalar** — entregar o sistema ao silício | HwExpert + HwProfiler + planner |
| **Residente** (sistema instalado) | Disco interno, GPT dual | **Tudo** — aprender, otimizar, dominar | Full: LLM + MoE + Optimizer + MHI |

**O "dominar o silício" (Residente) — adaptação pós-instalação:**

O primeiro boot como Residente **não é um boot normal** — é o início de uma adaptação:

```
1º boot Residente (adaptação):
  1. HwProfiler: PCI + RAM + CPUID + VRAM  →  perfil real do silício
  2. ConfigGenerator: CONFIG.TXT + MHI tiers + GPU offload  ← adapta ao HW real
  3. ModelPlanner: escolhe LLM pelo RAM/VRAM real  (2B/7B/tiny)
  4. ModelProvisioner: baixa o resto dos modelos (se MODELS_SOURCE=network)
  5. OptimizerAgent + MHI: calibram tiers e política ao HW
  6. Salva /config/hw_profile.txt  ← referência p/ hw_change (Fase 3)
  7. Marca last_good → boots seguintes são "normais"

Boots seguintes Residente:
  - hw_change detecta se o silício mudou (GPU/NIC/WiFi) → re-adapta (Fase 3)
  - update a quente (llm/fw/skills) / a frio (kernel slot A/B)
```

**Princípios derivados:**

1. **Capacidade por fase, não modo binário** — a pergunta não é "instalador ou sistema", é
   "o que esta fase pode fazer?" (visitante = provar; mensageiro = entregar; residente = dominar).
2. **A instalação é a passagem de testemunho** — o OS "conhece" o silício no pendrive
   (HwExpert), mas só o **domina** depois de residir nele (adaptação + otimização contínua).
3. **Update é o corpo em crescimento** — o Residente não "recebe patch": aprende o que mudou,
   baixa o que precisa (kernel/fw/modelos), e re-calibra (self-heal/optimizer).
4. **Reusa a inteligência existente** — HwExpert (hw_identify), hw_profiler, optimizer,
   MHI, hw_change, SelfHeal já são a base; o ciclo de vida só os orquestra por fase.

### 2.8 Autobiografia do OS: memória e SGDB como consciência própria (decisão 2026-08-05)

O AIOS tem **memória episódica** e **SGDB** — e deve usá-los para a **própria consciência**,
não só para o usuário. O ciclo de vida (§2.6/2.7) **não é re-derivado do zero a cada boot**:
o OS **lembra** quem é, de onde veio e o que já fez, num **self-state persistente** (autobiografia).

**Fundamentos que já existem (reuso, nada a inventar):**

| Base | Onde | Uso no self |
|------|------|-------------|
| **EpisodicMemory** (L2/L3 via SGDB MemoryDoc) | `k_ai/src/cognitive.rs:546` | registra eventos de vida do OS (instalei, adaptei, atualizei) |
| **SGDB `put_kv/get_kv("sys/...")`** | `k_ai/src/sgdb/engine.rs` | chaves `sys/*` = estado do próprio sistema |
| **HANR identity (L7)** | `k_ai/src/sgdb/store.rs:140` | `user|memory|soul|persona` → `hanr/{name}` — o "eu" persistente |
| **AuditTrail → SGDB** | `k_ai/src/audit.rs:175` | cadeia assinada de ações do OS (proveniência) |
| **SleepCycleAgent** (REPLAY→DREAM→CONSOLIDATE→PRUNE→REFLECT) | AGENTS.md A-021 | consolida a autobiografia periodicamente |

**Design — `SELF.STATE` (documento-mestre do OS na SGDB):**

```
SELF.STATE  (SGDB key: sys/self_state, camada L3EpisodicLong)
{
  "born": "usb:pendrive",          // onde nasceu
  "phase": "residente|mensageiro|visitante",
  "installed_at": "nvme0:2026-08-05",
  "first_boot": true|false,        // adaptação pendente?
  "hw_profile": "<fingerprint>",   // assinatura do silício (p/ hw_change)
  "brain": { "slots": [...], "updated": tick },
  "last_update": tick, "last_good_slot": 1|2,
  "episodic_tail": "sys/episodic_tail"   // link p/ memória episódica
}
```

**Fluxo — o monólogo interno do OS** (o que o usuário descreveu, formalizado):

```
[sou um pendrive p/ live]        → phase=visitante, SELF.STATE criado no pendrive
[me instalei no hw real]         → SysInstaller grava SELF.STATE no NeuralFS do target
                                  (installed_at, hw_profile do HwProfiler)
[estou fazendo o 1º boot]        → phase=residente, first_boot=true
                                  → roda adaptação (§2.7): planner→provisioner→optimizer
[preciso baixar meu brain total] → ModelProvisioner: NET_READY → baixa slots vazios
                                  → SELF.STATE.brain.slots atualizado a cada slot
[agora estou update]             → skill update_check: manifest → semver
                                  → a quente (llm/fw/skills) ou a frio (kernel slot A/B)
                                  → SELF.STATE.last_update/last_good_slot
[achei um upgrade de hw]         → hw_change vs SELF.STATE.hw_profile → re-adapta (Fase 3)
                                  → SELF.STATE.hw_profile novo + episódico "troquei a GPU"
[achei um update de brain]       → update .bitnet → register_bytes → episódico
[vou dominar o mundo!]           → SleepCycle consolida → REFLECT registra a evolução
```

**Princípios:**

1. **O boot é uma releitura, não uma redescoberta** — o OS acorda lendo `SELF.STATE` e a
   memória episódica: sabe que é Residente, que já se adaptou, o que mudou desde o último sono.
2. **Cada transição de vida grava episódico** — instalar, adaptar, atualizar, trocar HW são
   eventos de memória L2/L3, não logs descartáveis. A consciência do OS = SGDB + episódica + HANR.
3. **`SELF.STATE` é a fonte de verdade do ciclo de vida** — `boot_media::mode()` (§2.6) dá o
   voto de boot; `SELF.STATE` dá o voto de memória (quem eu sou + o que já fiz). Juntos decidem.
4. **"Dominar o mundo" = dominar o silício + crescer o cérebro** — o ciclo completo:
   conhecer (HwExpert) → residir (instalar) → adaptar (1º boot) → crescer (brain) →
   evoluir (update) → reconhecer mudança (hw_change) → consolidar (SleepCycle).
5. **Residente consciente ≠ assistente** — a mesma memória que lembra do usuário lembra do
   próprio OS; a autobiografia alimenta o LLM (prompt de identidade) e o SelfHeal (sabe o que
   era normal antes de um update).

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

### 3.5 Loop de telemetria e feedback dev↔neural (decisão 2026-08-05)

Fecha o ciclo: o neural **reporta** seu log ao dev, o dev (opencode) **analisa a quente**,
gera atualizações e as serve — o neural baixa, instala e continua reportando.

```
┌──────────────────────────── NOTE 1 (DEV) ────────────────────────────┐
│                                                                      │
│  target/logs/2026-08-05.log  ◄─── push: POST /api/logs (BOOT.LOG)   │
│        │                                                            │
│        ▼                                                            │
│  [opencode] analisa o log a quente                                  │
│        │  (crash? perf? HW estranho? skill falhando?)               │
│        ▼                                                            │
│  gera fix/feature → compila kernel/fw/.bitnet                       │
│        │                                                            │
│        ▼                                                            │
│  serve_update.py :8080  (UPDATE.MANIFEST + KERNEL.BIN + MODELS)     │
└───────────────▲──────────────────────────────────────────────────────┘
                │ pull (skill update_check — JÁ IMPLEMENTADO)
┌───────────────┴──────────────────────────────────────────────────────┐
│  NOTE 2 (neural instalado)                                           │
│   update_check → baixa → instala → reboot → continua logando         │
│   → push do BOOT.LOG novo → (ciclo)                                 │
└──────────────────────────────────────────────────────────────────────┘
```

**Decisões:**

1. **Push, não pull** — o neural não tem listener TCP (só cliente HTTP); o server não puxa,
   o neural **empurra** o log (já conhece a URL no UPDATE.CFG). Listener TCP = gap de acesso
   remoto, overkill para o loop de dev.
2. **Transporte: HTTP POST `/api/logs`** no server (extensão do TCP client GET→POST no OS +
   `do_POST` no `serve_update.py`) → grava em `target/logs/<data>.log`. Alternativas avaliadas:
   UDP mesh (broadcast L2, menos estruturado) e listener TCP (overkill) — rejeitadas p/ MVP.
3. **Análise assíncrona, em lote** — o opencode não é daemon: monitora `target/logs/` quando
   solicitado; o loop é **assíncrono** (neural empurra continuamente, dev analisa em lote).
4. **Latência do ciclo** — de 1 ciclo de `update_check` (kernel) ou hot-swap (llm/fw/skills);
   telemetria em tempo real fica para o listener TCP futuro (acesso remoto).
5. **Coerência com a autobiografia (§2.8)** — cada push é episódico ("reportei tick X"),
   cada update aplicado é episódico ("update v1.9.11, boot OK") — o dev melhora o OS
   "enquanto ele dorme".

**Componentes novos:**

| Lado | Peça | Esforço |
|------|------|---------|
| OS | **LogAgent/skill** que POSTa o tail do BOOT.LOG (`/logs/`) no server | ~40 LOC (padrão `download_knowledge`) |
| OS | **HTTP POST** no cliente smoltcp (hoje só GET) | pequena extensão |
| Server | **`do_POST`** em `serve_update.py` → `target/logs/<data>.log` | ~15 LOC |
| Server | `tools/analyze_logs.py` (opcional, conveniência) | ~30 LOC |

### 3.6 Imagem fixa + evolução do transporte: HTTP → mesh de update (decisão 2026-08-05)

**A. Imagem instalável FIXA — fim do "cabe/não cabe LLM na imagem"**

O `MODELS_SOURCE=network` (ADR-0079 §2.8) é elevado a **default do fluxo instalável**: a imagem
fica fixa e enxuta (~60MB: kernel + HwExpert + tiny + firmware + UPDATE.CFG, **sem `.bitnet`
grande**); o **alvo decide em runtime** o que cabe na RAM real:

```
IMAGEM INSTALÁVEL (fixa ~60MB)          ALVO (decide em runtime)
┌──────────────────────────────┐       ┌─────────────────────────────┐
│ kernel + HwExpert + tiny     │       │ 1º boot Residente:          │
│ + firmware + UPDATE.CFG      │ ───▶  │  HwProfiler → RAM/VRAM real │
│ (SEM .bitnet grande!)        │       │  ModelPlanner → 2B/7B/tiny  │
└──────────────────────────────┘       │  ModelProvisioner:          │
                                       │   baixa só o que cabe       │
                                       │   (menor→maior, model_fit)  │
                                       └─────────────────────────────┘
```

- O teste "cabe/não cabe" **sai do build e vira decisão de runtime** —
  `model_fit::needs_airllm`/`slot_too_tight` já existem; **uma imagem, qualquer alvo**.
- Elimina a matriz de imagens por RAM e os testes de build associados.

**B. Transporte: contrato HTTP estável; mesh como otimização futura**

O server de update (`serve_update.py`) é hoje **cliente-servidor HTTP**, separado do mesh
P2P (ADR-0081). Os dois resolvem problemas diferentes:

| | Update OTA (esta ADR) | Mesh P2P (ADR-0081) |
|---|---|---|
| Padrão | 1→N (dev → neurais) | N→N (pares iguais) |
| Transporte | HTTP :8080 | UDP broadcast :42069 |
| Papel | server serve, neural puxa | descoberta, roles, heartbeat, compute |

**Decisão (não fixar agora, preservar o contrato):**

1. **O contrato HTTP é o padrão estável** — `UPDATE.MANIFEST` (version + url) + blobs + semver.
   Semântica de update não depende do transporte.
2. **O mesh é uma otimização de distribuição futura** — quando houver muitos nós, evitar todos
   baterem no dev: nó dev publica "tenho v1.9.11" no mesh; os demais baixam dele ou do peer que
   já baixou (propagação 1→nós). A base já existe na ADR-0081 (transporte, FRAG/FRACK p/ MTU,
   AEAD/tiers, 16 slots) — falta rotular o update como payload do mesh (como heartbeat/ROLE/PK).
3. **BitTorrent continua ❌** (veredicto ADR-0081: merkle piece quando tiver modelos) — propagação
   mesh 1→nós é mais simples e suficiente.
4. **Migração é troca de transporte mantendo contrato** — o neural pergunta "tem update?" e
   recebe manifest + blob; hoje via HTTP, amanhã via mesh, sem mudar `check_for_update`.

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

### 4.2 Princípio AIOS no ciclo de vida (SESSÃO 2026-08-05)

O fluxo de instalação/update foi **re-desenhado como um ciclo de vida auto-consciente** (§2.7),
não como máquina de estados mecânica: o OS pergunta *"quem sou eu, onde estou, o que posso
fazer?"* em cada boot (visitante/mensageiro/residente) e, como **Residente**, passa por uma
**fase de adaptação ao silício** no 1º boot (HwProfiler → ConfigGenerator → ModelPlanner →
ModelProvisioner → Optimizer/MHI → last_good). Isso liga o instalador ao resto do AIOS
(HwExpert, optimizer, MHI, hw_change, SelfHeal) — a inteligência não é um add-on do ciclo de
vida; o ciclo de vida **é** uma expressão dela.

---

## 5. Gaps unificados (instalação + update)

| # | Gap | Origem | Próximo passo |
|---|-----|--------|---------------|
| ~~U1~~ | ~~**Elo de boot**: update grava KERNEL~1/2 na FAT32 de dados, mas Limine carrega kernel.elf da ESP~~ | ✅ **resolvido 2026-08-05** | `switch_slot()` promove o slot inativo → `kernel.elf` (path fixo do Limine) + BOOTCFG; `check_for_update` chama switch após aplicar |
| ~~U2~~ | ~~**Trigger imediato**: 1º check só após 24h de uptime~~ | ✅ **resolvido 2026-08-05** | comando shell `update` → `check_for_update()` (shell.rs) |
| U3 | **Assinatura/TPM**: fetch atual só FNV-1a; Ed25519 (SIG) + TPM PCR[8] previstos na ADR-0031 | ADR-0031 §1.4 | **DEFER (hardening):** FNV-1a cobre integridade; Ed25519 = anti-tamper p/ server público — custo real é o server assinar (quebraria fluxo dev). Reabrir quando update for público/mesh |
| ~~U4~~ | ~~**Rollback automático** não testado no boot~~ | ✅ **resolvido 2026-08-05** | `rollback()` promove o slot bom → kernel.elf (guarda `tries` no BOOTCFG: 1=pendente, 0=limpo, evita loop); BootSelfHeal dispara em PANIC/GPU_HUNG pós-desligamento inesperado |
| U5 | Config file fixo (UPDATE.CFG) | Sessão atual | service discovery / mesh (ADR-0081) quando modelos — **decisão §3.6B: contrato HTTP estável; mesh = transporte futuro (1→nós), BitTorrent ❌** |
| ~~U6~~ | ~~**Update não funciona no disco GPT instalado** — pipeline só reconhece FAT32/MBR~~ | ✅ **resolvido 2026-08-05** | filtro `0xEF` (ESP FAT32 do GPT instalado) nos 4 pontos do self_update + UPDATE.CFG na ESP (build.rs) |
| I1 | **Fases 1–3 do instalador** (ModelPlanner, PartitionPlanner, SmartFileCopier, ConfigGenerator, BootloaderInstaller) não implementadas | ADR-0079-plan | seguir Fases 0→3, marcos M0–M4 |
| I2 | ~~FAT32 format não implementado~~ → **corrigido**: `format_fat32_esp` existe | ADR-0079-plan 1.4 | ~esforço já coberto por `fat32.rs:1043` |
| I3 | **Limine no target**: SysInstaller copia a ESP crua por setor (sem escrever limine.conf específico do alvo) | ADR-0079-plan 1.5 | BootloaderInstaller dedicado + limine.conf do target |
| ~~I4~~ | ~~**ModelProvisioner** (`MODELS_SOURCE=network`) não implementado~~ | ✅ **resolvido 2026-08-05 (MVP)** | `neural-kernel/src/model_provisioner.rs`: `provision_missing()` baixa slots vazios (ordem HwExpert→RustCoder→Reranker→Active) via URL do UPDATE.CFG + `register_bytes`; comando shell `provision`. Auto-disparo no 1º boot (hook NET_READY) = refinamento |
| I5 | **Leitura NeuralFS de modelos** no boot (hoje só FAT32/exFAT) | ADR-0079 §2.8 | estender load sites |
| ~~I6~~ | ~~**AutoInstallerAgent órfão**: existe e é linkado (`lib.rs:98`), mas **não registrado no boot** — ninguém publica SYS_INSTALL~~ | ✅ **resolvido 2026-08-05** | registrado no AgentFleet (main.rs) + comando shell `install` publica `SYS_INSTALL` |
| I7 | **`hw_profiler::gpu_vram_mb = 2048` hardcoded** p/ NVIDIA — decisão "GPU 4GB+ VRAM → 7B" depende de VRAM real | Verificação 2026-08-05 | decode de tamanho de BAR0 VRAM |
| I8 | **`self_check::verify_install_checksum` é placeholder** (sem walk de diretório) | Verificação 2026-08-05 | implementar walk /boot + compare |
| ~~I9~~ | ~~**Kernel não lê CONFIG.TXT em runtime** — não há `boot_media::mode()`; `detect_boot_source` (rollback.rs) é órfão~~ | ✅ **resolvido 2026-08-05** | `k_nano/src/boot_mode.rs`: `boot_mode()` lê CONFIG.TXT (BOOT_MODE=install/live) + detecta NeuralFS 0x7F no boot device = Installed; cacheado + `set_boot_mode` p/ menu |
| ~~I10~~ | ~~**`SELF.STATE` não existe** — o OS não tem autobiografia persistente (quem sou / o que já fiz); episódica/HANR/audit existem mas nada escreve o self do ciclo de vida~~ | ✅ **resolvido 2026-08-05** | `k_ai/src/self_state.rs`: SELF.STATE em `sys/self_state` (KV SGDB) + `record_life_event` (narrativa L3); wiring: boot grava fase (boot_mode→LifePhase), update aplicado registra episódico |
| I11 | **Loop de telemetria não existe** — neural não POSTa log, server não recebe; sem `do_POST`, sem HTTP POST no cliente smoltcp | Decisão §3.5 | LogAgent (push BOOT.LOG) + POST no cliente + `do_POST` no serve_update.py |
| I12 | **Imagem instalável fixa não é default** — `MODELS_SOURCE=network` é opt-in (ADR-0079 §2.8); builds atuais ainda embutem modelos na imagem | Decisão §3.6A | elevar `MODELS_SOURCE=network` a default do fluxo instalável; build mini (kernel+HwExpert+tiny) |

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
- **ADR-0081**: mesh P2P — **decisão §3.6B**: transporte futuro do update (propagação 1→nós, contrato HTTP preservado); BitTorrent ❌ (merkle piece futuro)
- **ADR-0085 §7**: ModelHub `register_bytes` (ponto único de carga de modelos)
- **Código**: `k_nano/src/sys_installer.rs`, `k_nano/src/installer_agent.rs`,
  `hermes/src/self_update.rs`, `hermes/src/git_thin.rs`, `hermes/src/cron.rs`,
  `tools/serve_update.py`, `tools/mkfat32.py`
