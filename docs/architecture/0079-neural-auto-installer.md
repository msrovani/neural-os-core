# ADR-0079: Neural AutoInstaller — Migração Inteligente Pendrive → HD/SSD/NVMe

**Data:** 2026-07-27
**Status:** Accepted
**Lifecycle:** `fazendo`
**Inspirado por:** AIOS first principles, HW Expert v3, ADR-0040 §SysInstaller, TenonOS (arXiv 2512.00400), seed (Awis13), bootc
**Sprint:** v2.0.0+
**Documentos:** `docs/architecture/0079-neural-auto-installer-plan.md` (plano de implementação)

---

## 1. Problema

O neural-os-core boota de um pendrive USB (FAT32/exFAT com kernel + modelos + firmwares).
O usuário quer **migrar o sistema para o disco interno** (HD/SSD/NVMe) sem perder dados,
sem depender de ferramenta externa, e sem carregar gigas de firmware/modelos que o HW alvo
não precisa.

### Por que NÃO um instalador tradicional?

| Abordagem | Problema |
|-----------|----------|
| **Sector copy** (SysInstaller atual) | Copia 64MB cegos. Não entende partições, não instala bootloader, não copia dados. Bruto. |
| **dd/clonezilla style** | Copia TUDO — inclusive firmwares de HW que o alvo não tem. Pendrive de 8GB vira 8GB no SSD. Desperdício. |
| **Package manager + download** | Precisa de internet no momento da instalação. Nem sempre disponível. |
| **Imagem monolítica** | Cada HW exigiria uma imagem diferente. Inviável. |

### A proposta: AutoInstaller Neural

O pendrive carrega **todas as peças**. O AutoInstaller **detecta o HW alvo**,
seleciona **só o que aquele HW precisa**, particiona e formata o disco,
instala bootloader + kernel + peças selecionadas, e configura o sistema
para boot autônomo. A IA (HW Expert + Trinity MoE) decide:

- **Qual modelo LLM instalar** baseado na RAM detectada
- **Quais firmwares copiar** baseado no PCI scan
- **Qual variante WASM das skills** compilar baseado nas CPU features
- **Qual FS e layout de partição** baseado no tamanho e tipo do disco
- **Se precisa de GPU offload** baseado na VRAM disponível

---

## 2. Decisões Arquiteturais

### 2.1 Modelo LLM por RAM detectada

O pendrive carrega múltiplos modelos .bitnet. O instalador copia só o que
cabe no perfil de memória da máquina alvo:

| RAM detectada | Modelo a instalar | Impacto |
|---------------|-------------------|---------|
| 4-8GB | `BITNET_TINY.BIN` (100M params, ~50MB) | Roda folgado, MHI agressivo |
| 8-16GB | `BITNET2B.BIN` (2B params, ~250MB) | Uso normal, MHI padrão |
| 16-32GB | `BITNET7B.BIN` (7B params, ~875MB) | Respostas mais ricas |
| 32GB+ | `BITNET2B` + `BGE.BIN` + `RERANK.BIN` | Full stack: geração + embedding + rerank |

**Se GPU NVIDIA com 4GB+ VRAM detectada:** instala modelo 7B mesmo com 8GB RAM
sistema — compute vai na GPU.

### 2.2 Rerank — cross-encoder opcional

Rerank é um segundo modelo (cross-encoder) que reordena resultados de busca
vetorial antes de alimentar o LLM:

```
Pergunta → Embedding (BGE) → busca SGDB → 20 candidatos → Rerank → top 3 pro LLM
```

| RAM | Stack de retrieval |
|-----|--------------------|
| 4-8GB | Só embedding (BGE) |
| 8-16GB | Embedding + rerank leve (15M params) |
| 16GB+ | Embedding + rerank completo |

Sem rerank funciona, mas com ele o Cortex recebe contexto mais limpo e alucina menos.

### 2.3 Firmware por HW detectado

PCI scan + HW Expert decidem quais firmwares copiar:

| PCI Class | Detectou | Instala firmware |
|-----------|----------|------------------|
| `0x03.00` (VGA) | Vendor 0x10DE (NVIDIA) | `FW_FECS_BL.BIN` + `FW_GPCCS.BIN` |
| `0x03.00` (VGA) | Vendor 0x8086 (Intel) | `FW_I915_*.BIN` (24 blobs) |
| `0x02.00` (NIC) | Vendor 0x10EC (Realtek) | `FW_RTL_NIC_*.BIN` |
| `0x02.80` (WiFi) | Vendor 0x8086 (Intel) | `FW_IWLWIFI_*.BIN` |
| Nenhum | — | Pula firmware, não instala nada |

Ganho real: não carregar firmware incompatível, não ter falha de init no boot.

### 2.4 WASM compile por HW

O instalador não só copia WASM — **compila/otimiza skills para o HW alvo**:

```
Skill WASM genérica (no_std, scalar)
  ├── AVX2 + FPU     →  skill com SIMD
  ├── soft-float     →  skill sem SSE, usa libm
  ├── GPU NVIDIA     →  skill com shader path
  ├── NPU Intel      →  skill com NPU delegate
  └── RAM < 4GB      →  skill "lite" (fuel + heap limitados)
```

O `app_factory` (ADR-0059) já tem o seletor A/B/C (wasmi / Cranelift / nativo).
O instalador escolhe a variante por skill baseado nas CPU features detectadas.

### 2.5 MHI substitui swap

**Decisão: NÃO criar partição swap.** O neural-os-core tem MHI (Memory Hierarchy
Infrastructure, `k_nano/src/mhi.rs`) que faz migração inteligente por tiers:

```
VRAM (GPU)  ←  DRAM  ←  NVMe  ←  SSD  ←  pendrive
 tier 0       tier 1    tier 2    tier 3    tier 4
```

MHI move **blocos de dados significativos** (tensors, páginas de código pouco
usadas) entre tiers por frequência de acesso. Swap só como residual **extremo**:
se RAM < 4GB **e** único disco for HDD lento (sem NVMe/SSD).

| RAM detectada | Decisão MHI | Configuração |
|---------------|-------------|--------------|
| 4-8GB | MHI com NVMe como tier 2 ativo | Migração agressiva, modelo tiny |
| 8-16GB | MHI com DRAM tier 1 principal | Padrão, modelo 2B |
| 16-32GB | MHI com GPU VRAM como tier extra | GPU offload ativo, modelo 7B |
| 32GB+ | MHI despreocupado | Full stack + rerank |

### 2.6 Bootloader alvo: Limine

**Decisão: Limine** sobre bootloader 0.11.

- Feature `limine-boot` já existe no Cargo.toml
- Limine carrega kernel.elf de qualquer FS (FAT32, ext4, NeuralFS)
- ESP do target só precisa de Limine + Limine.conf
- kernel.elf na partição NeuralFS — upgrade sem regenerar ESP
- ClaudioOS provou funcionar (ADR-0062 §P4)

Fallback: `/EFI/BOOT/BOOTX64.EFI` (default UEFI boot path).

### 2.7 Particionamento

| Partição | FS | Tamanho | Conteúdo |
|----------|----|---------|----------|
| ESP | FAT32 | 512MB | Limine bootloader |
| NeuralFS | NeuralFS | Restante do disco | /boot/, /models/, /firmware/, /skills/, /config/, /data/, /users/ |

Discos < 32GB podem usar FAT32 único (sem NeuralFS) — decidido pelo PartitionPlanner.

---

## 3. Arquitetura

```
┌──────────────────────────────────────────────────────────┐
│                 AutoInstallerAgent                         │
│  (agente EventDriven, acionado por intent `/install`)     │
│                                                            │
│  ┌─────────────┐  ┌──────────────┐  ┌──────────────────┐  │
│  │ HwProfiler  │  │ PartitionMap │  │  SmartFileCopier  │  │
│  │ PCI scan    │  │ GPT layout   │  │  copia peças      │  │
│  │ +HW Expert  │  │ +format      │  │  +compila WASM    │  │
│  │ +RAM detect │  │              │  │                   │  │
│  └──────┬──────┘  └──────┬───────┘  └────────┬─────────┘  │
│         │                │                    │            │
│  ┌──────┴──────┐  ┌──────┴───────┐  ┌────────┴────────┐  │
│  │ModelPlanner │  │ GptWriter    │  │ BootCfg         │  │
│  │tiny/2B/7B/  │  │ +FsFormatter │  │ Limine.conf     │  │
│  │full stack   │  │ +MHI config  │  │ +kernel.elf     │  │
│  └─────────────┘  └──────────────┘  └─────────────────┘  │
│                                                            │
│  ┌──────────────────────────────────────────────────────┐  │
│  │           AI Advisor (Cortex + Hermes intent)        │  │
│  │  HW: NVMe 1TB + Intel UHD + AX201 + 16GB RAM        │  │
│  │  Recomendo: NeuralFS, modelo 7B, rerank on,          │  │
│  │  3 firmwares Intel, skill AVX2. Confirmar?           │  │
│  └──────────────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────────────┘
```

### 3.1 Componentes

| Componente | Função | Arquivo |
|-----------|--------|---------|
| **AutoInstallerAgent** | Agente EventDriven, orquestra instalação | `k_nano/src/installer_agent.rs` |
| **HwProfiler** | PCI scan + HW Expert + RAM detect | `k_nano/src/hw_profiler.rs` |
| **ModelPlanner** | Decide modelo LLM por RAM | `k_nano/src/model_planner.rs` |
| **PartitionPlanner** | Decide layout GPT | `k_nano/src/partition_planner.rs` |
| **GptWriter** | Escreve GPT + partições | Reusa `gpt::gpt_format_single()` |
| **FsFormatter** | Formata FAT32 + NeuralFS | `k_nano/src/fs_formatter.rs` |
| **BootloaderInstaller** | Instala Limine no ESP | `k_nano/src/boot_installer.rs` |
| **SmartFileCopier** | Copia firmware selecionado + compila WASM | `k_nano/src/file_copier.rs` |
| **ConfigGenerator** | Gera CONFIG.TXT + MHI config | `k_nano/src/config_gen.rs` |
| **Cortex InstallAdvisor** | LLM conversa sobre instalação | `cortex/src/install_adviser.rs` (Fase 2) |

### 3.2 Algoritmo de Seleção

```
1. HwProfiler:
   - PCI scan → lista de (vendor, device, class, subclass)
   - RAM detectada via e820 ou ACPI
   - CPU features via CPUID (AVX2, SSE, soft-float)

2. ModelPlanner:
   - RAM ≥ 32GB → full stack (2B + BGE + rerank)
   - RAM ≥ 16GB → 7B
   - RAM ≥ 8GB  → 2B
   - RAM < 8GB  → tiny
   - GPU 4GB+ VRAM → override permite 7B mesmo com 8GB RAM

3. SmartFileCopier:
   para cada firmware no pendrive:
       hw_ids = extrair_hw_ids(firmware_path)
       se any hw_id em PCI scan:
           incluir(manifest, firmware)
   para cada modelo .bitnet no pendrive:
       se modelo está no plano do ModelPlanner:
           incluir(manifest, modelo)
   para cada skill WASM no pendrive:
       hw_req = ler_skill_manifest(skill_path).hardware_requirements
       cpu_feat = HwProfiler.cpu_features
       variant = wasm_compiler.choose_variant(skill, cpu_feat)
       incluir(manifest, {skill, variant})

4. PartitionPlanner:
   - total ≥ 32GB → ESP 512MB + NeuralFS
   - total < 32GB → FAT32 única
   - sem NVMe/SSD (HDD) + RAM < 4GB → criar swap residual
```

---

## 4. Estado Atual do Código

| Componente | Status | Localização |
|-----------|--------|-------------|
| **SysInstaller** | ⚠️ Existe (262 LOC), **não linkado** na lib.rs | `crates/k_nano/src/sys_installer.rs` |
| **BlockDevice write** | ✅ ATA/AHCI/NVMe/USB todos escrevem | `k_nano/src/{ata,ahci,disk_agent/nvme,usb_msc}.rs` |
| **BlockDevice trait** | ✅ Trait unificado | `k_nano/src/block_dev.rs` |
| **StorageBus** | ✅ Registry de devices | `k_nano/src/storage_bus.rs` |
| **GPT format** | ✅ `gpt_format_single()` | `k_nano/src/gpt.rs` |
| **FAT32 write** | ✅ root directory | `k_nano/src/fat32.rs` |
| **exFAT write** | ✅ opt-in `EXFAT_WRITE=1` | `k_nano/src/exfat_write.rs` |
| **NeuralFS** | ✅ format() + write_file() | `k_nano/src/neural_fs/volume.rs` |
| **PCI scan** | ✅ scan + class detect | `k_nano/src/pci.rs` |
| **HW Expert** | ✅ 61.453 HWIDs, modelo treinado | `HWEXPRT.BIN` |
| **MHI** | ✅ tiers + migração | `k_nano/src/mhi.rs` |
| **Limine** | ✅ feature gate + protocol | `k_nano/src/limine.rs` |
| **Cortex LLM** | ✅ BitNet + MoE | `cortex/src/` |
| **app_factory A/B/C** | ✅ wasmi + Cranelift + arena | `hermes/src/app_factory.rs` |
| **Skill Manifest** | ✅ FYY schema | `hermes/src/skill_manifest.rs` |
| **Hermes intents** | ✅ framework de intents | `hermes/src/intent_bus.rs` |
| **Jarbas cards** | ✅ cards UI | `jarbas/src/cards/` |

---

## 5. Riscos e Mitigações

| Risco | Prob. | Impacto | Mitigação |
|-------|-------|---------|-----------|
| Bootloader no target não funciona | Média | Alto | Fallback = pendrive sempre bootável; testar QEMU + disco virtual |
| NeuralFS format corrompe | Baixa | Crítico | Validar format() com readback; fallback FAT32 |
| HW Expert não reconhece HW | Média | Baixo | Fallback: copiar firmware genérico ou pular; usuário adiciona manual |
| UEFI boot entry não persiste | Média | Alto | Fallback `/EFI/BOOT/BOOTX64.EFI` (caminho default UEFI) |
| Instalação interrompida | Baixa | Crítico | Checkpoint por etapa; retomar ou alertar no próximo boot |
| WASM compile falha para skill | Baixa | Baixo | Fallback: instalar skill genérica (wasmi) em vez da otimizada |

---

## 6. Comparação com Ecossistema

| Projeto | Tem self-installer? | Observação |
|---------|-------------------|------------|
| **ClaudioOS** (52 crates, 295K LOC) | ❌ | ROADMAP.md lista "Boot from USB stick" como TODO |
| **FYY** | ❌ | CLI tool de mesh, não OS |
| **Wetware** | ❌ | Daemon sobre SO hospedeiro |
| **WeftOS** | ❌ | Kernel userspace |
| **Oreulius** | ❌ | Unikernel pesquisa |
| **WAeasi** | ❌ | Microkernel pesquisa |
| **coconutOS** | ❌ | Microkernel GPU |
| **ArceOS** | ❌ | Unikernel modular |
| **bootc** (Linux) | ✅ | Roda sob Linux, `bootc install to-disk` |
| **seed** (C) | ⚠️ | Bootloader que aceita firmware via API — conceito similar |
| **neural-os-core** (este) | 🏆 **Primeiro** | AutoInstaller neural em bare-metal no_std |

Nenhum projeto AIOS no_std tem self-installer. O neural-os-core será pioneiro.

---

## 7. Recomendação

**Aceito.** Fazer Fase 0 + Fase 1 (3 sprints) como MVP do Neural AutoInstaller.
Fase 2 (AI dialog) e Fase 3 (HW swap/recovery) como residuais para v2.1+.

O plano detalhado está em `docs/architecture/0079-neural-auto-installer-plan.md`.

---

## 8. Referências

- ADR-0040: Filesystem Architecture — §SysInstaller
- ADR-0059: Runtime App Factory — wasmi + Cranelift + arena
- ADR-0062: ClaudioOS vs Neural-AIOS — Limine bootloader
- `crates/k_nano/src/sys_installer.rs` — SysInstaller atual (262 LOC)
- `crates/k_nano/src/gpt.rs` — GPT format
- `crates/k_nano/src/neural_fs/` — NeuralFS
- `crates/k_nano/src/block_dev.rs` — BlockDevice trait
- `crates/k_nano/src/pci.rs` — PCI scan
- `crates/k_nano/src/mhi.rs` — Memory Hierarchy
- `crates/k_nano/src/limine.rs` — Limine protocol
- `crates/hermes/src/app_factory.rs` — Seletor A/B/C WASM
- arXiv 2512.00400 — TenonOS (self-generating libOS via LLM)
- Awis13/seed — bootloader IA com watchdog + rollback
- bootc-dev/bootc — `bootc install to-disk` em Rust
