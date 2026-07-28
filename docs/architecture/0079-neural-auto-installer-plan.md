# Plano de Implementação — ADR-0079 Neural AutoInstaller

**Data:** 2026-07-27
**Status:** Approved
**Base:** ADR-0079 (Neural AutoInstaller) + ADR-0040 §SysInstaller
**Estimativa:** 3 fases · ~3.200 LOC · 5 sprints · 1-2 engenheiros
**Gate:** v2.0.0 (Fase 0-1); v2.1.0 (Fases 2-3)

---

## Estrutura de Dependências

```
Fase 0 ─── independente (destravar SysInstaller existente)
  │
Fase 1 ─── depende de Fase 0
  │
  ├── 1.1 HwProfiler ─── independente (só PCI scan + HW Expert, já existem)
  ├── 1.2 ModelPlanner ─── depende de 1.1 (precisa RAM detect)
  ├── 1.3 PartitionPlanner ─── independente
  ├── 1.4 GptWriter ─── reusa gpt.rs existente
  ├── 1.5 FsFormatter ─── independente (reusa NeuralFS + FAT32)
  ├── 1.6 BootloaderInstaller ─── independente (reusa Limine)
  ├── 1.7 SmartFileCopier ─── depende de 1.1 (precisa perfil HW)
  ├── 1.8 ConfigGenerator ─── depende de 1.1 + 1.2
  └── 1.9 Hermes/Jarbas integration ─── depende de tudo acima

Fase 2 ─── depende de Fase 1
  ├── 2.1 Cortex InstallAdvisor ─── depende de 1.1 (perfil HW)
  └── 2.2 Dialog flow ─── depende de 2.1

Fase 3 ─── depende de Fase 1
  ├── 3.1 HW change detection ─── depende de perfil salvo (1.8)
  ├── 3.2 SelfHeal disk migration ─── depende de Fase 1
  └── 3.3 NetFs fallback ─── depende de NetFs existente
```

**Paralelizável:** 1.1 + 1.3 + 1.4 + 1.5 + 1.6 (independentes entre si após Fase 0)

---

## FASE 0: Destravar SysInstaller Existente

**Prioridade:** 🔴 Alta · **LOC:** ~150 · **Dias:** 2-3
**Objetivo:** SysInstaller funcional que copia kernel + GPT para qualquer BlockDevice.
**Código existe mas não compila** — só precisa linkar e adaptar.

### Subitens

| Step | Ação | Arquivo(s) | Esforço | Verificação |
|------|------|-----------|---------|-------------|
| 0.1 | Adicionar `pub mod sys_installer;` em lib.rs | `crates/k_nano/src/lib.rs` | ☆ 1min | `cargo check -p k_nano` = 0 erros |
| 0.2 | Substituir `build_target_ata()` hardcoded por `BlockDevice` genérico — recebe target da StorageBus em vez de fixar ATA 0x170 | `crates/k_nano/src/sys_installer.rs` | ☆☆ | `install()` aceita qualquer BlockDevice da StorageBus |
| 0.3 | Chamar `gpt_format_single()` no target antes de copiar setores — cria GPT com partição NeuralFS | `crates/k_nano/src/sys_installer.rs` → `gpt.rs` | ☆ | GPT criada no target com CRC32C válido |
| 0.4 | Copiar kernel.elf do boot current para /boot/ no target | `crates/k_nano/src/sys_installer.rs` | ☆ | kernel.elf lido e escrito no target |
| 0.5 | demo() expandido: testa com NVMe + AHCI + USB como target | `crates/k_nano/src/sys_installer.rs` | ☆ | demo() passa para cada tipo de BlockDevice |

### Código-chave a modificar

```rust
// Atual: hardcoded ATA secundário
fn build_target_ata(&self) -> Result<AtaDriver, &'static str> {
    // fixa 0x170, A0/B0
}

// Novo: recebe BlockDevice da StorageBus
fn install(&mut self, target: &mut dyn BlockDevice) -> Result<(), &'static str> {
    // 1. Cria GPT
    gpt::gpt_format_single(target, total_lba, GPT_TYPE_NEURALFS, "neural-os-core")?;
    // 2. Formata NeuralFS
    let mut fs = NeuralVolume::format(target, 2048, data_lba)?;
    // 3. Copia kernel.elf
    fs.write_file("/boot/kernel.elf", kernel_data)?;
}
```

### Gate
- ✅ `cargo check --release` = 0 erros
- ✅ `demo()` com NVMe/AHCI/USB target PASS
- ✅ Nenhuma regressão em módulos existentes
- ✅ Commit semântico

---

## FASE 1: Instalador Completo com Inteligência de HW

**Prioridade:** 🔴 Alta · **LOC:** ~1.800 · **Dias:** 12-15 (2 sprints)
**Objetivo:** `install /dev/nvme0n1` funcional, com progresso, seleção inteligente de peças.

### Step 1.1 — HwProfiler

**Arquivo:** `crates/k_nano/src/hw_profiler.rs` (novo)
**Esforço:** ☆☆ ~250 LOC

**Responsabilidade:** Monta perfil completo do HW alvo consultando PCI scan + HW Expert + RAM detection + CPUID.

```rust
pub struct HwProfile {
    pub pci_devices: Vec<PciDevice>,
    pub total_ram_mb: u64,
    pub cpu_features: CpuFeatures,   // AVX2, SSE, soft-float
    pub detected_firmware: Vec<FirmwareEntry>,
    pub has_nvidia_gpu: bool,
    pub has_intel_gpu: bool,
    pub has_intel_wifi: bool,
    pub has_realtek_nic: bool,
    pub vram_mb: u64,                // GPU VRAM se disponível
}

pub fn profile_hardware() -> HwProfile {
    // 1. PCI scan existente
    let devices = pci::scan_pci();
    // 2. Cruza com HW Expert
    let fw = match_hw_to_firmware(&devices);
    // 3. RAM via e820 / ACPI
    let ram = memory::detect_ram_mb();
    // 4. CPUID features
    let cpu = cpuid::detect_features();
    HwProfile { devices, total_ram_mb, cpu_features: cpu, ... }
}
```

| Subtarefa | Ação |
|-----------|------|
| 1.1a | Integrar `pci::scan_pci()` → lista de devices com vendor/device/class |
| 1.1b | Cruzar PCI devices com HW Expert via `match_hw_to_firmware()` |
| 1.1c | Detectar RAM total (reusa `memory::detect_ram_mb()` existente) |
| 1.1d | Detectar VRAM de GPU (NV BAR0 size, Intel stolen memory) |
| 1.1e | Detectar CPU features via CPUID (AVX2, SSE, FPU, soft-float) |

### Step 1.2 — ModelPlanner

**Arquivo:** `crates/k_nano/src/model_planner.rs` (novo)
**Esforço:** ☆ ~100 LOC

**Responsabilidade:** Decide qual(is) modelo(s) LLM instalar baseado no HwProfile.

```rust
pub enum ModelPlan {
    Tiny,       // BITNET_TINY.BIN
    Standard,   // BITNET2B.BIN
    Large,      // BITNET7B.BIN
    FullStack,  // BITNET2B + BGE + RERANK
}

pub fn plan_models(profile: &HwProfile) -> ModelPlan {
    match profile.total_ram_mb {
        ram if ram >= 32_000 => ModelPlan::FullStack,
        ram if ram >= 16_000 => ModelPlan::Large,
        ram if ram >= 8_000  => ModelPlan::Standard,
        _ => ModelPlan::Tiny,
    }
    // Override: GPU 4GB+ VRAM → permite Large mesmo com 8GB RAM
}
```

### Step 1.3 — PartitionPlanner

**Arquivo:** `crates/k_nano/src/partition_planner.rs` (novo)
**Esforço:** ☆ ~80 LOC

**Responsabilidade:** Decide layout de partições baseado no tamanho e tipo do disco alvo.

```rust
pub struct PartitionLayout {
    pub esp_size_lba: u64,
    pub data_start_lba: u64,
    pub data_fs: DataFs,        // NeuralFS ou FAT32
    pub has_swap: bool,         // residual: só se HDD + RAM < 4GB
}

pub fn plan_partitions(disk_size_lba: u64, profile: &HwProfile) -> PartitionLayout {
    let has_ssd = profile.disks.iter().any(|d| d.is_ssd());
    if disk_size_lba * 512 >= 32 * GB {
        // ESP + NeuralFS
    } else {
        // FAT32 única
    }
}
```

### Step 1.4 — FsFormatter

**Arquivo:** `crates/k_nano/src/fs_formatter.rs` (novo)
**Esforço:** ☆☆ ~200 LOC

**Responsabilidade:** Formata partições no target — ESP como FAT32, dados como NeuralFS.

| Subtarefa | Ação |
|-----------|------|
| 1.4a | Implementar `format_fat32(dev, start_lba, total_lba)` — escreve FAT32 boot record + FAT tables + root dir |
| 1.4b | Reusar `NeuralVolume::format(dev, start_lba, total_lba)` — já existe |
| 1.4c | `format_partitions(dev, layout)` — orquestra formatação de todas as partições |

**Nota:** FAT32 format não existe no código atual — precisa ser implementado (ou podemos extrair FAT32 boot record do pendrive e copiar com parâmetros ajustados).

### Step 1.5 — BootloaderInstaller

**Arquivo:** `crates/k_nano/src/boot_installer.rs` (novo)
**Esforço:** ☆☆☆ ~350 LOC

**Responsabilidade:** Instala Limine bootloader no ESP do target e configura Limine.conf.

```rust
pub fn install_bootloader(
    esp_dev: &mut dyn BlockDevice,
    kernel_elf: &[u8],
    limine_conf: &str,
) -> Result<(), &'static str>;

pub fn generate_limine_conf(kernel_path: &str, profile: &HwProfile) -> String;
```

| Subtarefa | Ação |
|-----------|------|
| 1.5a | Copiar Limine EFI binary (BOOTX64.EFI) para `/EFI/BOOT/` no ESP |
| 1.5b | Escrever kernel.elf no caminho configurado (ex: `/neural/boot/kernel.elf` na NeuralFS) |
| 1.5c | Gerar Limine.conf com `protocol=limine`, `kernel_path`, `cmdline` com parâmetros adaptados ao HW |
| 1.5d | Configurar UEFI boot entry via efivars (ou fallback BOOTX64.EFI) |

**Risco:** Limine EFI binary precisa estar disponível em runtime (embutido no kernel ou copiado do pendrive).

### Step 1.6 — SmartFileCopier

**Arquivo:** `crates/k_nano/src/file_copier.rs` (novo)
**Esforço:** ☆☆ ~300 LOC

**Responsabilidade:** Copia arquivos do pendrive para o target, filtrando por HW detectado e compilando variante WASM.

```rust
pub struct InstallManifest {
    pub firmware: Vec<(&'static str, &'static [u8])>,
    pub models: Vec<(&'static str, &'static [u8])>,
    pub skills: Vec<SkillVariant>,    // skill + variante WASM
    pub config: ConfigFile,
}

pub fn build_manifest(profile: &HwProfile, model_plan: &ModelPlan) -> InstallManifest;
pub fn copy_manifest(
    manifest: &InstallManifest,
    target_fs: &mut dyn FilesystemDriver,
    progress: &dyn Fn(u8, &str),
) -> Result<(), &'static str>;
```

| Subtarefa | Ação |
|-----------|------|
| 1.6a | `build_manifest()` — seleciona firmwares por HW, modelos por ModelPlan, skills por compatibilidade |
| 1.6b | `wasm_compile_for_hw(skill, cpu_features)` — se CPU tem AVX2, compila variant com SIMD; senão, usa scalar |
| 1.6c | `copy_manifest()` — copia arquivos em paralelo (quando possível), reporta progresso |
| 1.6d | Integrar com `app_factory::choose_backend()` para decidir wasmi vs Cranelift vs arena |

### Step 1.7 — ConfigGenerator

**Arquivo:** `crates/k_nano/src/config_gen.rs` (novo)
**Esforço:** ☆ ~120 LOC

**Responsabilidade:** Gera CONFIG.TXT e configuração MHI adaptadas ao HW alvo.

```rust
pub fn generate_config(profile: &HwProfile, model_plan: &ModelPlan) -> ConfigFile;
pub fn generate_mhi_config(profile: &HwProfile) -> MhiConfig;
```

| Subtarefa | Ação |
|-----------|------|
| 1.7a | CONFIG.TXT com parâmetros de boot adaptados (memória, GPU, NIC) |
| 1.7b | Configuração MHI com tiers baseados nos dispositivos detectados |
| 1.7c | Configuração de GPU offload se VRAM disponível |

### Step 1.8 — AutoInstallerAgent + Hermes Integration

**Arquivo:** `crates/k_nano/src/installer_agent.rs` (novo) + `hermes/src/intents.rs` + `jarbas/src/cards/`
**Esforço:** ☆☆☆ ~400 LOC

**Responsabilidade:** Agente EventDriven que orquestra todo o fluxo, subscribe `SYS_INSTALL`, publica progresso.

```rust
pub struct AutoInstallerAgent {
    pub status: InstallStatus,
    pub progress: u8,
    pub current_step: String,
    pub manifest: Option<InstallManifest>,
}

impl Agent for AutoInstallerAgent {
    fn handle_event(&mut self, event: &Event) -> AgentTickResult {
        match event.topic.as_str() {
            "SYS_INSTALL" => self.run_installation(),
            "USER_CONFIRM" => self.confirm_and_install(),
            _ => AgentTickResult::Pending,
        }
    }
}
```

| Subtarefa | Ação |
|-----------|------|
| 1.8a | Criar AutoInstallerAgent que orquestra HwProfiler → PartitionPlanner → FsFormatter → BootloaderInstaller → SmartFileCopier |
| 1.8b | Registrar agente no boot (Fase 6 AgentFleet) |
| 1.8c | Registrar intent `/install` no Hermes |
| 1.8d | Criar card de progresso no Jarbas (discover -> partitioned -> formatted -> bootloader -> copying -> complete) |
| 1.8e | Publicar progresso via EventBus para Hermes/Jarbas |

### Diagrama de Sequência (Fase 1)

```
Usuário          Hermes       AutoInstallerAgent    HwProfiler    StorageBus     Target
  │                │                │                   │             │            │
  │ "/install"     │                │                   │             │            │
  │───────────────>│                │                   │             │            │
  │                │ intent_parse   │                   │             │            │
  │                │───────────────>│                   │             │            │
  │                │                │ profile_hw()      │             │            │
  │                │                │──────────────────>│             │            │
  │                │                │                   │ PCI scan    │            │
  │                │                │                   │────────────>│            │
  │                │                │                   │<────────────│            │
  │                │                │<──────────────────│             │            │
  │                │                │                   │             │            │
  │                │  "NVMe 1TB,   │                   │             │            │
  │                │   Intel UHD,  │                   │             │            │
  │                │   16GB RAM.   │                   │             │            │
  │                │   Instalar?"  │                   │             │            │
  │                │<──────────────│                   │             │            │
  │ "sim, NVMe"    │                │                   │             │            │
  │───────────────>│                │                   │             │            │
  │                │ confirm()     │                   │             │            │
  │                │───────────────>│                   │             │            │
  │                │                │ plan() → layout   │             │            │
  │                │                │ format()          │             │────────────>
  │                │                │ install_boot()    │             │────────────>
  │                │                │ copy_files()      │             │────────────>
  │                │                │ gen_config()      │             │────────────>
  │                │                │                   │             │            │
  │                │  "Completo.   │                   │             │            │
  │                │   1.2GB/8GB.  │                   │             │            │
  │                │   Reiniciar?" │                   │             │            │
  │                │<──────────────│                   │             │            │
  │ "reiniciar"    │                │                   │             │            │
  │───────────────>│                │                   │             │            │
  │                │                │ reboot()          │             │            │
```

### Gate Fase 1
- ✅ `cargo check --release` = 0 erros
- ✅ `HwProfiler::profile_hardware()` retorna perfil correto em QEMU + HW real
- ✅ `ModelPlanner::plan_models()` retorna modelo compatível com RAM detectada
- ✅ `FsFormatter` formata FAT32 + NeuralFS corretamente
- ✅ `BootloaderInstaller` instala Limine + boota QEMU do target
- ✅ `SmartFileCopier` copia só firmwares do HW detectado
- ✅ AutoInstallerAgent responde a `/install` e mostra progresso no Jarbas
- ✅ demo() e2e: boot pendrive → `/install` → reboot → boot target
- ✅ STATE.md + CHANGELOG.md atualizados
- ✅ Commit semântico por step

---

## FASE 2: AI-Native Installation

**Prioridade:** 🟡 Média · **LOC:** ~800 · **Dias:** 6-8 (1-2 sprints)
**Objetivo:** Instalação com diálogo IA natural, recomendação inteligente, validação pós-boot.

### Step 2.1 — Cortex InstallAdvisor

**Arquivo:** `crates/cortex/src/install_adviser.rs` (novo)
**Esforço:** ☆☆☆ ~400 LOC

LLM (Trinity MoE) recebe o HwProfile e gera recomendação em linguagem natural:

```
"Recomendo instalar no NVMe Samsung 980 Pro de 1TB.
Detectei 16GB RAM → modelo 7B.
GPU Intel UHD → firmware i915.
WiFi Intel AX201 → firmware iwlwifi.
NeuralFS como FS principal. 3GB/s de leitura.
Confirmar?"
```

| Subtarefa | Ação |
|-----------|------|
| 2.1a | Template de prompt para o Trinity MoE com dados do HwProfile |
| 2.1b | Parse da resposta do LLM para confirmar/rejeitar decisões |
| 2.1c | Fallback para regras hardcoded se LLM não responder |

### Step 2.2 — Diálogo de Instalação

**Arquivo:** `hermes/src/install_dialog.rs` (novo)
**Esforço:** ☆☆ ~250 LOC

Hermes ReAct conduz diálogo com o usuário:

1. "Detectei NVMe 1TB e ATA 256GB. Onde instalar?"
2. "NVMe tem 3GB/s. Recomendo NeuralFS. OK?"
3. "Modelo 7B cabe na RAM. 3 firmwares Intel detectados."
4. "Instalação concluída. Quer reboot agora?"

### Step 2.3 — Self-Install Validation

**Arquivo:** `k_nano/src/self_check.rs`
**Esforço:** ☆☆ ~150 LOC

Pós-instalação: salva hash dos arquivos instalados. No primeiro boot do target, verifica integridade. Se falhar, pendrive bootável ainda funciona.

### Gate Fase 2
- ✅ `cargo check --release` = 0 erros
- ✅ Cortex InstallAdvisor recomenda corretamente para 3 perfis de HW diferentes
- ✅ Diálogo Hermes completo: pergunta → resposta → confirma → executa
- ✅ Self-check no primeiro boot do target PASS

---

## FASE 3: HW Swap & Recovery

**Prioridade:** 🟢 Baixa · **LOC:** ~800 · **Dias:** 8-12 (1-2 sprints)
**Objetivo:** Sistema sobrevive a troca de GPU, NIC ou disco.

### Step 3.1 — HW Change Detection

**Arquivo:** `k_nano/src/hw_change.rs`
**Esforço:** ☆☆ ~250 LOC

No boot, compara HW atual com perfil salvo pela ConfigGenerator. Se mudou:

- GPU diferente → carrega firmware novo, descarrega antigo
- NIC diferente → carrega driver novo
- Disco sumiu → alerta, tenta recovery

### Step 3.2 — SelfHeal Disk Migration

**Arquivo:** `k_ai/src/self_heal_disk.rs`
**Esforço:** ☆☆☆ ~350 LOC

Se disco falha, SelfHealAgent copia sistema para outro disco detectado. Reusa pipeline do AutoInstallerAgent.

### Step 3.3 — NetFs Fallback

**Arquivo:** `hermes/src/net_fallback.rs`
**Esforço:** ☆☆ ~150 LOC

Se HW mudou drasticamente (ex: GPU AMD → NVIDIA) e firmware não está em disco, busca na rede via NetFs + MCP marketplace.

### Gate Fase 3
- ✅ HW change detection detecta GPU/NIC/disk swap
- ✅ SelfHeal migra sistema para outro disco
- ✅ NetFs fallback baixa firmware ausente
- ✅ Boot continua funcional após HW swap

---

## Resumo de Esforço

| Fase | Item | LOC | Sprints | Dias | Depende | Risco |
|------|------|-----|---------|------|---------|-------|
| 0 | Destravar SysInstaller | ~150 | 1 | 2-3 | Nenhuma | Baixo |
| 1.1 | HwProfiler | ~250 | 0.5 | 2-3 | Fase 0 | Baixo |
| 1.2 | ModelPlanner | ~100 | 0.3 | 1 | 1.1 | Baixo |
| 1.3 | PartitionPlanner | ~80 | 0.3 | 1 | Fase 0 | Baixo |
| 1.4 | FsFormatter | ~200 | 0.5 | 2-3 | Fase 0 | Médio |
| 1.5 | BootloaderInstaller | ~350 | 1 | 3-5 | Fase 0 | **Alto** |
| 1.6 | SmartFileCopier | ~300 | 0.5 | 3-4 | 1.1+1.2 | Médio |
| 1.7 | ConfigGenerator | ~120 | 0.3 | 1-2 | 1.1+1.2 | Baixo |
| 1.8 | AutoInstallerAgent | ~400 | 1 | 3-4 | Todos acima | Médio |
| **Total F1** | | **~1.800** | **2** | **12-15** | | |
| 2 | AI-Native | ~800 | 1-2 | 6-8 | Fase 1 | Médio-Alto |
| 3 | HW Swap | ~800 | 1-2 | 8-12 | Fase 1 | Alto |
| **Total** | | **~3.550** | **5-7** | **28-38** | | |

---

## Topologia de Execução

```
Sprint N (Fase 0 + 1.1-1.3) — paralelizável
  Eng único: 0.1-0.5 destravar SysInstaller (2 dias)
  Eng único: 1.1 HwProfiler (2-3 dias)
  Eng único: 1.2 ModelPlanner (1 dia)
  Eng único: 1.3 PartitionPlanner (1 dia)

Sprint N+1 (Fase 1.4-1.7) — parcialmente paralelizável
  Eng 1: 1.4 FsFormatter (2-3 dias)
  Eng 1: 1.5 BootloaderInstaller (3-5 dias) — maior risco
  Eng 2: 1.6 SmartFileCopier + 1.7 ConfigGenerator (3-4 dias)

Sprint N+2 (Fase 1.8 + integração + testes)
  Eng 1-2: 1.8 AutoInstallerAgent + Hermes/Jarbas (3-4 dias)
  Eng 1-2: Teste e2e + correções (2-3 dias)

→ AutoInstaller MVP completo

[Residual] Sprint N+3-4: Fase 2 AI-Native
[Residual] Sprint N+4-5: Fase 3 HW Swap
```

---

## Marcos e Critérios de Sucesso

| Marco | Quando | Critério |
|-------|--------|----------|
| **M0** | Fim Sprint N | SysInstaller funcional: `demo()` com NVMe/AHCI/USB PASS |
| **M1** | Fim Sprint N+1 | Instala partição + bootloader + kernel em QEMU com disco virtual |
| **M2** | Fim Sprint N+2 | **AutoInstaller MVP:** `install /dev/nvme0n1` funcional com seleção inteligente de peças, progresso via Jarbas cards, reboot funcional |
| **M3** | Fase 2 | Instalação com diálogo IA natural, rollback funcional |
| **M4** | Fase 3 | HW swap detection + self-recovery + NetFs fallback |

---

## Entregas por Fase

### Fase 0 — Entrega
- `crates/k_nano/src/sys_installer.rs` modificado (~150 LOC alteradas)
- `crates/k_nano/src/lib.rs` +1 linha

### Fase 1 — Novos Arquivos
- `crates/k_nano/src/hw_profiler.rs` (~250 LOC)
- `crates/k_nano/src/model_planner.rs` (~100 LOC)
- `crates/k_nano/src/partition_planner.rs` (~80 LOC)
- `crates/k_nano/src/fs_formatter.rs` (~200 LOC)
- `crates/k_nano/src/boot_installer.rs` (~350 LOC)
- `crates/k_nano/src/file_copier.rs` (~300 LOC)
- `crates/k_nano/src/config_gen.rs` (~120 LOC)
- `crates/k_nano/src/installer_agent.rs` (~300 LOC)
- `hermes/src/intents.rs` modificado (~50 LOC)
- `jarbas/src/cards/install_card.rs` (~100 LOC)

### Fase 2 — Novos Arquivos
- `crates/cortex/src/install_adviser.rs` (~400 LOC)
- `hermes/src/install_dialog.rs` (~250 LOC)
- `k_nano/src/self_check.rs` (~150 LOC)

### Fase 3 — Novos Arquivos
- `k_nano/src/hw_change.rs` (~250 LOC)
- `k_ai/src/self_heal_disk.rs` (~350 LOC)
- `hermes/src/net_fallback.rs` (~150 LOC)

---

## Riscos por Step

| Step | Risco | Impacto | Mitigação |
|------|-------|---------|-----------|
| **1.5 BootloaderInstaller** | Limine EFI binary não disponível em runtime | Bloqueante | Embutir Limine BOOTX64.EFI como blob no kernel, ou extrair do pendrive |
| **1.4 FsFormatter** | FAT32 format não implementado | Alto | Implementar format FAT32 (boot record + FAT tables); reusar estrutura do pendrive |
| **1.6 SmartFileCopier** | WASM compile por HW não testado no_std | Médio | Fallback: copiar skill genérica (wasmi); compilação otimizada é upgrade |
| **1.1 HwProfiler** | RAM detection não disponível em todos os HW | Médio | Fallback: assumir 4GB (modelo tiny) se não conseguir detectar |
| **1.8 Hermes integration** | Hermes intents não conectadas ao instalador | Médio | MVP sem Hermes: instalador via comando serial/console |
| **2.1 Cortex InstallAdvisor** | LLM não responde ou responde nonsense | Baixo | Fallback para regras hardcoded do ModelPlanner |

---

## Checklist por Fase

### Fase 0
- [ ] `pub mod sys_installer` em lib.rs
- [ ] `build_target_ata()` substituído por BlockDevice genérico
- [ ] `gpt_format_single()` chamado antes da cópia
- [ ] kernel.elf copiado para /boot/ no target
- [ ] demo() com NVMe/AHCI/USB como target
- [ ] `cargo check` 0 erros
- [ ] Commit: `feat(sys-installer): fase 0 — linkar e generalizar SysInstaller`

### Fase 1
- [ ] HwProfiler monta perfil HW corretamente
- [ ] ModelPlanner decide modelo por RAM
- [ ] PartitionPlanner decide layout por tamanho do disco
- [ ] FsFormatter formata FAT32 + NeuralFS
- [ ] BootloaderInstaller instala Limine + boota QEMU do target
- [ ] SmartFileCopier copia só firmwares do HW + compila variante WASM
- [ ] ConfigGenerator gera CONFIG.TXT + MHI config
- [ ] AutoInstallerAgent orquestra fluxo completo
- [ ] Intent `/install` registrada no Hermes
- [ ] Card de progresso no Jarbas
- [ ] Teste e2e: boot pendrive → `/install` → reboot target
- [ ] `cargo check` 0 erros
- [ ] STATE.md + CHANGELOG.md atualizados

### Fase 2
- [ ] Cortex InstallAdvisor recomenda com 3 perfis de HW
- [ ] Hermes dialoga naturalmente com usuário
- [ ] Self-check pós-instalação no primeiro boot
- [ ] Rollback: se boot do target falha, pendrive bootável

### Fase 3
- [ ] HW change detection detecta GPU/NIC/disk swap
- [ ] SelfHeal migra sistema para outro disco
- [ ] NetFs fallback baixa firmware ausente

---

## Referências

- ADR-0079: Neural AutoInstaller (esse documento)
- `crates/k_nano/src/sys_installer.rs` — SysInstaller base
- `crates/k_nano/src/gpt.rs` — `gpt_format_single()`
- `crates/k_nano/src/neural_fs/volume.rs` — `NeuralVolume::format()`
- `crates/k_nano/src/block_dev.rs` — `BlockDevice` trait
- `crates/k_nano/src/storage_bus.rs` — `StorageBus`
- `crates/k_nano/src/pci.rs` — `scan_pci()`
- `crates/k_nano/src/limine.rs` — Limine protocol (feature gate)
- `crates/k_nano/src/mhi.rs` — Memory Hierarchy
- `crates/hermes/src/app_factory.rs` — Seletor A/B/C WASM
- `crates/hermes/src/intent_bus.rs` — Intent framework
- `crates/hermes/src/skill_manifest.rs` — Skill metadata
- `tools/build_usb_unified.py` — Imagem do pendrive
- `crates/cortex/src/` — LLM inference
- `crates/k_ai/src/self_heal.rs` — SelfHeal base
- `docs/architecture/0079-neural-auto-installer.md` — ADR-0079
- `docs/architecture/0040-filesystem-architecture.md` — ADR-0040
