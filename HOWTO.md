# HOWTO — AIOS K²CHJ Dev Environment Setup

**Para desenvolvedores, entusiastas, e agentes de IA (OpenCode, Claude Code, Codex, etc.)**

Este documento guia a configuração completa do ambiente de desenvolvimento do **AIOS K²CHJ (neural-os-core)** — um sistema operacional bare-metal em Rust (`no_std`, `no_main`) com 247+ agentes, IA nativa, GPU compute, e auto-recuperação.

---

## Índice

1. [Pré-requisitos](#1-pré-requisitos)
2. [Clone e Build](#2-clone-e-build)
3. [Setup do Toolchain Rust](#3-setup-do-toolchain-rust)
4. [QEMU — Teste Local](#4-qemu--teste-local)
5. [HW Real — Pendrive Bootável](#5-hw-real--pendrive-bootável)
6. [Firmware e Modelos](#6-firmware-e-modelos)
7. [Estrutura do Projeto](#7-estrutura-do-projeto)
8. [Para Agentes de IA (OpenCode, Claude Code)](#8-para-agentes-de-ia-opencode-claude-code)
9. [Workflow de Desenvolvimento](#9-workflow-de-desenvolvimento)
10. [Solução de Problemas](#10-solução-de-problemas)

---

## 1. Pré-requisitos

### Hardware
- **CPU:** x86_64 (Intel/AMD), mínimo 2 cores (recomendado 4+)
- **RAM:** Mínimo 4GB (recomendado 8GB+ para QEMU)
- **GPU (opcional):** NVIDIA Pascal+ (GTX 1050+) para GPU compute real
- **Disco:** 10GB livres para toolchain + builds
- **Pendrive (opcional):** 4GB+ para boot em HW real

### Software
- **Sistema:** Windows 10+ (com PowerShell 5.1+) ou Linux
- **Git:** https://git-scm.com/downloads
- **Rust:** nightly toolchain (via rustup)
- **Python:** 3.10+ (para scripts de treino e extração)
- **QEMU:** 8.0+ (para testes locais)
- **7-Zip:** (para extração de DriverPacks SDIO)
- **OVMF:** firmware UEFI para QEMU

---

## 2. Clone e Build

```bash
# Clone
git clone https://github.com/msrovani/neural-os-core.git
cd neural-os-core

# Build (leva ~2-5min na primeira vez)
cargo build --release

# Verificação rápida (0 erros, 0 warnings)
cargo check --release
```

**Esperado:**
```
   Compiling neural-kernel v1.2.0
   Compiling boot v0.1.0
    Finished `release` profile [optimized] target(s) in 0.30s
    0 errors, 0 warnings
```

---

## 3. Setup do Toolchain Rust

### Windows (PowerShell)
```powershell
# Instalar rustup (se nao tiver)
winget install Rustlang.Rustup

# Instalar nightly (obrigatório para bare-metal)
rustup toolchain install nightly
rustup default nightly

# Adicionar target x86_64
rustup target add x86_64-unknown-none

# Verificar
rustc --version  # deve mostrar "nightly"
```

### Linux
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
rustup toolchain install nightly
rustup default nightly
rustup target add x86_64-unknown-none
```

### Dependências Windows (MSYS2/MinGW)
O projeto usa `cc` para linkar o bootloader. No Windows, instale MSYS2:

```powershell
# Instalar MSYS2
winget install MSYS2.MSYS2

# No terminal MSYS2 UCRT64:
pacman -S mingw-w64-ucrt-x86_64-gcc
```

Adicione ao PATH do Windows:
```
C:\msys64\ucrt64\bin
```

---

## 4. QEMU — Teste Local

### Instalar QEMU

**Windows:** Baixe de https://qemu.org ou use winget:
```powershell
winget install QEMU.QEMU
```

**Linux:**
```bash
sudo apt install qemu-system-x86-64 ovmf
```

### Baixar OVMF (firmware UEFI)

O projeto precisa de OVMF para boot UEFI em QEMU:

```powershell
# Windows — copie do MSYS2 ou baixe de:
# https://github.com/retrage/edk2-nightly/raw/master/OVMF-pure-efi.fd
Copy-Item "C:\msys64\usr\share\ovmf\OVMF.fd" "target\ovmf.fd"
```

### Gerar imagem de disco
```powershell
# Cria disk_qemu.raw com modelos .bitnet + firmware (271MB)
python tools\build_image.py
```

### Executar QEMU
```powershell
# Com framebuffer UEFI (janela grafica)
.\run-qemu-uefi.ps1 -Window

# Sem janela (modo texto)
.\run-qemu-uefi.ps1
```

### Com bridge serial (rede via SLIP)
```powershell
# Terminal 1: bridge (deixa rodando)
python tools\serial_bridge.py

# Terminal 2: QEMU
.\run-qemu-uefi.ps1
```

---

## 5. HW Real — Pendrive Bootável

### Gerar imagem para pendrive
```powershell
# Cria disk_hw.raw (64MB) — sem BITNET-2B (modelo muito grande para pendrive menor)
python tools\build_image.py --size 128 --output tools\disk_hw.raw
```

### Gravar no pendrive

**Windows (Rufus):**
```powershell
# Baixe rufus.exe de https://rufus.ie
.\rufus.exe tools\disk_hw.raw
```

**Linux:**
```bash
sudo dd if=tools/disk_hw.raw of=/dev/sdX bs=4M status=progress
sync
```

### Boot
1. Conecte o pendrive no PC alvo (i5-6400 + GTX 1050 recomendado)
2. Ligue o PC e entre na BIOS (F2/Del)
3. Configure: UEFI boot, disable Secure Boot
4. Selecione o pendrive como dispositivo de boot
5. O AIOS K²CHJ inicializará automaticamente

---

## 6. Firmware e Modelos

### Download de Firmware
O projeto já inclui firmware no diretório `firmware/`. Para atualizar:

```powershell
# Baixa firmware NVIDIA + Intel + Realtek do GitLab mirror
python tools\download_firmware.py
```

### Modelos .bitnet
Os modelos já estão incluídos na imagem de disco (`build_image.py`). Para retreinar:

```powershell
# HW Expert v3 (1M params, 259KB, 61K VID/DID)
python tools\train_hw_expert_v3.py --epochs 50

# HDIO HWIDs — extrair de DriverPacks
python tools\extract_sdio_hw.py --dir "Caminho\para\DriverPacks" --extract-only
```

### Firmware do linux-firmware (GitLab mirror)
```bash
git clone --depth 1 https://gitlab.com/kernel-firmware/linux-firmware.git
# Ou via script:
python tools/download_firmware.py
```

---

## 7. Estrutura do Projeto

```
neural-os-core/
├── AGENTS.md              # → REGRAS PARA AGENTES DE IA (leia primeiro!)
├── TECNOLOGIAS.md         # Catálogo de PI e inovações
├── HOWTO.md               # ← Você está aqui
├── README.md              # Visão geral
├── CHANGELOG.md           # Histórico de versões
├── ROADMAP.md             # Roadmap
├── TODO.md                # Tarefas pendentes
├── SUMMARY.md             # Resumo executivo
│
├── crates/
│   ├── neural-kernel/     # → Kernel principal (~16.500 LOC)
│   │   └── src/
│   │       ├── main.rs    # Entry point + boot sequence
│   │       ├── agents.rs  # → 247+ agentes (Hermes, Cortex, etc)
│   │       ├── cortex.rs  # → LLM, Trinity MoE, HW Expert
│   │       ├── hermes.rs  # → Intent router, ReAct, Workflow
│   │       ├── ata.rs     # → ATA PIO driver (fix v1.2.0!)
│   │       ├── e1000.rs   # → NIC driver
│   │       ├── display/   # → Framebuffer + compositor + orb
│   │       ├── audio/     # → HDA driver + TTS + wake word
│   │       ├── gpu/       # → NVIDIA/Intel/AMD + firmware
│   │       └── pci.rs     # → PCI scan
│   ├── agent-core/        # → Agent trait, scheduler
│   ├── skill-registry/    # → Skill registry, MCP, FanOut
│   ├── event-bus/         # → IPC pub/sub + MemoryTree
│   └── ticket-lock/       # → Lock FIFO
│
├── firmware/              # → Firmware blobs git-tracked
│   ├── nvidia/gp108/      #   FECS+GPCCS (39KB)
│   ├── i915/              #   GuC+HuC+DMC SKL/KBL (3.8MB)
│   ├── rtl_nic/           #   Realtek NIC (217KB)
│   ├── rtlwifi/           #   Realtek WiFi (1MB)
│   └── intel/iwlwifi/     #   Intel AX200/AX210 (7.5MB)
│
├── tools/                 # → Scripts Python
│   ├── train_hw_expert_v3.py      # Treino HW Expert
│   ├── extract_sdio_hw.py         # Extração SDIO HWIDs
│   ├── extract_firmware_metadata.py # Extração WHENCE
│   ├── fetch_pci_usb_ids.py       # Download pci/usb-ids
│   ├── download_firmware.py       # Download firmware
│   ├── mkfat32.py / build_image.py # Gerador de imagem
│   ├── serial_bridge.py           # Bridge SLIP/QEMU
│   └── test_firmware.py           # Validação de blobs
│
├── docs/
│   ├── architecture/      # → 40 ADRs (decisões arquiteturais)
│   ├── memory/            # → STATE.md, IDEA_BANK.md, Sessions
│   └── sprint-plan-v1.1.x.md  # Plano de sprints
│
├── run-qemu-uefi.ps1      # → Script QEMU (Windows)
├── run-qemu.ps1           # → Script QEMU BIOS (Windows)
└── .gitignore
```

---

## 8. Para Agentes de IA (OpenCode, Claude Code)

### Se você é um agente de IA lendo este arquivo:

Seu objetivo é ajudar o usuário a configurar, entender e contribuir com o AIOS K²CHJ. Siga estas diretrizes:

#### 8.1 Arquivos Prioritários

| Leia primeiro | Por quê |
|---|---|
| `AGENTS.md` | **Regras mestras** — contém o plano diretor, sprints, lições críticas. É o seu mapa. |
| `TECNOLOGIAS.md` | Catálogo de PI — entenda o que é inovação original vs portado. |
| `docs/memory/STATE.md` | Estado atual do kernel — o que funciona, o que não funciona. |
| `TODO.md` | Tarefas pendentes — veja o que precisa ser feito. |
| `docs/memory/IDEA_BANK.md` | 416+ ideias catalogadas — entenda a visão de longo prazo. |

#### 8.2 Regras de Conduta para Agentes

1. **NUNCA hardcode skills no enum Intent.** Skills são geradas a quente pelo LLM e registradas pelo SkillObserver. O fluxo correto é: usuário → WakeWord → Hermes → Chat → LLM → gera skill → SkillObserver registra → executa.
2. **NUNCA use stubs.** Se algo não pode ser implementado completamente, documente o bloqueio em `docs/dead-ends.md` com a tag CRM-XXX.
3. **SEMPRE execute `cargo check --release` antes de considerar uma mudança completa.**
4. **SEMPRE execute `cargo clean -p neural-kernel && cargo check --release` quando erros somem misteriosamente** — o cache incremental mascara erros.
5. **SEMPRE verifique `TECNOLOGIAS.md` antes de decidir qual tecnologia usar** — evite reinventar a roda.
6. **NUNCA adicione dependências externas** sem justificativa forte. O kernel é no_std. Prefira implementar do que adicionar crates.
7. **Skills a quente:** "grava video" ou "imprime formulario" viram skills gerados pelo LLM sob demanda, não por enum Rust.
8. **Documente no `AGENTS.md`** qualquer lição crítica que você aprender durante a sessão.

#### 8.3 Workflow do Agente

```mermaid
flowchart TD
    A[Ler AGENTS.md] --> B[Ler STATE.md + TODO.md]
    B --> C[Entender sprint atual]
    C --> D{Implementar?}
    D -->|Sim| E[Buscar no TECNOLOGIAS.md referências]
    E --> F[Implementar com #![no_std]]
    F --> G[cargo check --release]
    G --> H{0 erros?}
    H -->|Sim| I[Documentar lição em AGENTS.md]
    H -->|Não| J[Corrigir + cargo clean -p neural-kernel]
    J --> G
    I --> K[Commit + push]
```

#### 8.4 Comandos Úteis para Agentes

```bash
# Verificar compilação
cargo check --release

# Build completo
cargo build --release

# Check com clean (revela erros mascarados pelo cache)
cargo clean -p neural-kernel && cargo check --release

# Gerar imagem de disco
python tools/build_image.py

# Executar QEMU
.\run-qemu-uefi.ps1 -Window

# Extrair HWIDs SDIO
python tools/extract_sdio_hw.py --dir "Caminho\SDIO" --extract-only

# Treinar HW Expert
python tools/train_hw_expert_v3.py --epochs 30
```

---

## 9. Workflow de Desenvolvimento

### 1. Branch
```bash
git checkout -b feature/minha-feature
```

### 2. Implementar
Siga o plano de sprints em `docs/sprint-plan-v1.1.x.md`.

### 3. Compilar
```bash
cargo build --release
# Se erros estranhos aparecerem:
cargo clean -p neural-kernel && cargo build --release
```

### 4. Testar em QEMU
```powershell
# Gerar imagem com modelos + firmware
python tools\build_image.py

# Rodar QEMU
.\run-qemu-uefi.ps1 -Window
```

### 5. Verificar log
```bash
# Último log gerado
Get-ChildItem logs\boot_uefi*.txt | Sort-Object LastWriteTime -Descending | Select-Object -First 1

# Procurar erros
Get-Content logs\boot_uefi_*.txt | Select-String "ERROR|FAIL|PANIC"
```

### 6. Commit
```bash
git add -A
git commit -m "tipo: descricao concisa"
git push
```

---

## 10. Solução de Problemas

| Problema | Causa | Solução |
|----------|-------|---------|
| `error: linker 'cc' not found` | Falta GCC/MSYS2 | Instale MSYS2 com `pacman -S mingw-w64-ucrt-x86_64-gcc` |
| `error: could not compile neural-kernel` + erros estranhos | Cache incremental corrompido | `cargo clean -p neural-kernel && cargo check --release` |
| `[MBR] Signature 55AA nao encontrada` | Boot image nao tem FAT32 | Execute `python tools\build_image.py` para gerar disk_qemu.raw |
| QEMU nao abre | OVMF nao encontrado | Copie OVMF.fd para `target/ovmf.fd` |
| `[DISPLAY] Sem framebuffer UEFI` | QEMU sem UEFI | Use `-bios` com OVMF ou execute `.\run-qemu-uefi.ps1` |
| Bridge serial nao conecta | Porta 4444 ocupada | Mate processos python anteriores: `Stop-Process -Name python*` |
| GPU nao detectada em QEMU | QEMU nao emula NVIDIA | Teste em HW real ou WHPX |
| `error: failed to run custom build command for boot` | Linker nao encontrado | Configure `cc` no PATH ou instale MSYS2 |
| ATA PIO bug | `in al, dx+1` lê FEATURES/ERROR (fix v1.2.0) | Use `in ax, dx` (16-bit) |

---

## Links Rápidos

| Recurso | Link |
|---------|------|
| Repositório | https://github.com/msrovani/neural-os-core |
| HuggingFace Org | https://huggingface.co/aios-k2chj |
| HW Expert v3 Model | https://huggingface.co/aios-k2chj/aios-k2chj-hw-expert-v3 |
| SDIO HWIDs Dataset | https://huggingface.co/datasets/aios-k2chj/aios-k2chj-sdio-hwids |
| PCI/USB IDs Dataset | https://huggingface.co/datasets/aios-k2chj/aios-k2chj-pci-usb-ids |
| Firmware Metadata | https://huggingface.co/datasets/aios-k2chj/aios-k2chj-firmware-metadata |
| linux-firmware (GitLab) | https://gitlab.com/kernel-firmware/linux-firmware |
| Rust nightly | https://rust-lang.github.io/rustup/ |
| QEMU | https://www.qemu.org |

---

> **AIOS K²CHJ — Neural OS Hermes v1.2.0**  
> *"O hardware real não perdoa. O silício obedece."*
