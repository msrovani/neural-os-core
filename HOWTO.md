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

# Build kernel (soft-float, alias recomendado)
cargo nk

# Verificação rápida
cargo clean -p neural-kernel && cargo nk
```

**Esperado:** `Finished release profile` com **0 erros** (warnings dead-code são política conhecida).

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
# Cria target\disk_qemu.raw (1024 MB default) com .bitnet + firmware (+ BITNET-2B se existir)
python tools\build_image.py
# Artefato canonico: target\disk_qemu.raw (scripts tambem aceitam tools\ e copiam)
```

### Executar QEMU
```powershell
# Fluxo canônico UEFI + FAT32 (IDE index=1) + log em logs\
.\run-qemu-uefi.ps1 -Window

# Sem janela / gera disco se faltar
.\run-qemu-uefi.ps1 -BuildDisk

# WHPX (fallback: -Tcg)
.\run-qemu-whpx.ps1 -Window
```

### Com bridge serial (rede via SLIP / bypass NIC)
```powershell
# Preferido: WHPX sobe e mata o bridge sozinho (TCP server :4444, QEMU cliente COM2)
.\run-qemu-whpx.ps1 -Window

# Manual (se -NoSerialBridge): Terminal 1
python tools\serial_bridge.py
# Terminal 2
.\run-qemu-whpx.ps1 -NoSerialBridge -Window
```
Topologia: `serial_bridge.py` escuta `127.0.0.1:4444`; QEMU usa `-serial tcp:127.0.0.1:4444` (cliente, sem `server=on`).

---

## 5. HW Real — Pendrive Bootável

Pendrive tipico: **32 GB livres** — use imagem generosa (1024 MB+) e **inclua BITNET-2B**.

### Firmware vs HW Expert (importante)

| Asset | Precisa de `firmware/`? | O que carregar |
|-------|-------------------------|----------------|
| **HW Expert** (`HWEXPRT.BIN` / `hw_expert_v3.bitnet`) | **Não** | Modelo BitNet ~260 KB no FAT. Identifica VID/DID; **não** carrega blobs linux-firmware. |
| **GPU NVIDIA / i915 / Realtek NIC / iwlwifi** | **Sim** | Blobs em `firmware/` copiados pelo `mkfat32.py` como `FW_*.BIN` no FAT. Sem eles, drivers em HW real falham no load de ucode/FECS/etc. |
| **Piper / STT / BPE / RustCoder / BGE / BitNet 2B** | **Não** (firmware) | Modelos `.bin`/`.bitnet` no FAT (ou QEMU-loader só em VM). |

`CONFIG.TXT` na imagem HW inclui `BOOT_MODE=hw`, `LOG_TO_FAT32=1`.

### Gerar imagem de dados (FAT + modelos + firmware)

Para **dois meios** ou disco SATA separado; o fluxo recomendado de **1 stick** é a seção **USB unificado** abaixo.

```powershell
# Soft-float kernel (obrigatório):
$env:CARGO_TARGET_DIR = "$PWD\target"
cargo nk
# Opcional: log B*.LOG no FAT (além do serial) — útil sem cabo COM:
# cargo nk --features fat-boot-log

# Imagem UEFI bootável (bootloader + kernel):
# Preferir bootloader_linker se `cargo build -p boot` travar em nested install.
cargo build --release -p boot

# target\disk_hw.raw (1024 MB default; cabe 2B + Piper + firmware ~12 MB)
python tools\build_image.py --hw
python tools\build_image.py --hw --size 1536   # se 2B+Piper apertarem
```

Conteúdo típico do FAT (`mkfat32.py`): `BITNET2B.BIN`, `HWEXPRT.BIN`, `RUSTCDR.BITNET`, `BGE.BIN`, `PIPER.BIN`, `STT.BIN`, `BPE.BIN`, `MICRO.BITNET`, `FW_*` (todos os blobs de `firmware/`), `CONFIG.TXT`.

### Gerar USB unificado (recomendado — 1 pendrive)

Uma imagem GPT: **ESP** (bootloader+kernel de `uefi.img`) + **FAT32 dados** (modelos/firmware, mesmo conteúdo de `--hw`). MBR híbrido expoe a partição de dados como `0x0C` para o kernel achar `BITNET2B`/`HWEXPRT` no mesmo stick.

```powershell
# Soft-float + uefi.img:
$env:CARGO_TARGET_DIR = "$PWD\target"
cargo nk
cargo build --release -p boot

# target\usb_hw.img (+ alias target\disk_hw_unified.raw)
python tools\build_image.py --hw --unified
# ou: python tools\build_usb_unified.py --size 1536
```

**Windows (Rufus, um stick, modo DD):**
1. Baixe [Rufus](https://rufus.ie)
2. Device = seu pendrive (≥ **2 GB** livres; **8 GB+** recomendado se incluir 2B+Piper)
3. Selecione `target\usb_hw.img`
4. Modo de imagem = **DD** (não ISO)
5. Start → apaga o pendrive

**Linux:**
```bash
# CUIDADO: confira o device (lsblk) — apaga o alvo
sudo dd if=target/usb_hw.img of=/dev/sdX bs=4M status=progress && sync
```

### Dois meios (opcional — igual QEMU ide0/ide1)

Ainda válido para lab / segundo disco SATA:

1. Pendrive A: `target\uefi.img`
2. Pendrive B ou HD: `target\disk_hw.raw` (`python tools\build_image.py --hw`)

```bash
sudo dd if=target/uefi.img of=/dev/sdX bs=4M status=progress && sync
sudo dd if=target/disk_hw.raw of=/dev/sdY bs=4M status=progress && sync
```

### Boot
1. Conecte o pendrive unificado (ou boot USB + disco de dados)
2. BIOS/UEFI: Secure Boot **off**, boot **UEFI** no pendrive
3. No serial (abaixo), procure `[STATUS]`, `[HWEXPERT]`, `[FAT]`, `[PIPER]`, `[STT]`, `[BPE]`, `[GEN]`, `[TTS]`

### Coletar log em HW real (serial COM)

Kernel: UART 16550 em **COM1 = I/O `0x3F8`** (fallback `0x2F8` / `0x3E8` / `0x2E8`), baud típico **115200 8N1** (`uart_16550::SerialPort::init`).

**Windows — cabo USB-Serial / header COM da placa:**
```powershell
# Liste portas
mode
# Ou: Get-CimInstance Win32_SerialPort | Select DeviceID, Name

# PuTTY: Connection → Serial → COMx, Speed 115200, 8N1
# Ou captura crua (ajuste COMx):
$port = New-Object System.IO.Ports.SerialPort COM3,115200,None,8,One
$port.Open()
$log = "logs\boot_hw_$(Get-Date -Format yyyyMMdd_HHmmss).txt"
$sw = [IO.StreamWriter]::new($log)
while ($true) {
  $line = $port.ReadLine()
  $sw.WriteLine($line)
  $sw.Flush()
  Write-Host $line
}
```

**Não use** `tools\serial_bridge.py` em HW real — ele é SLIP/TCP `:4444` para QEMU COM2, não UART físico.

**Log em FAT (opcional):** compile com `--features fat-boot-log`. No volume de dados aparecem `B?????.LOG` (sessão). Sem a feature, o serial imprime `[LOG] SKIP fat session write...` e **mantém** todo `serial_println!`.

### Features de build (QEMU HIT vs HW)
| Feature | Default | Uso |
|---------|---------|-----|
| *(nenhuma)* | HW / release | Boot padrão; sem skinny STT-sim / seed clima |
| `weather-e2e` | off | Path Sprint 107 HIT (`run_weather_e2e.ps1`) |
| `fat-boot-log` | off no kernel | Persiste `B*.LOG` no FAT; serial sempre ativo |

Para reativar HIT clima QEMU: `cargo nk --features weather-e2e` e rebuild boot (`cargo build --release -p boot`).

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
├── AGENTS.md              # Regras para agentes de IA (leia primeiro)
├── TECNOLOGIAS.md         # Catálogo de PI e inovações
├── HOWTO.md               # ← Você está aqui
├── README.md              # Visão geral
├── CHANGELOG.md           # Histórico de versões
├── ROADMAP.md             # Roadmap
├── TODO.md                # Tarefas pendentes
├── SUMMARY.md             # Resumo executivo
│
├── crates/
│   ├── neural-kernel/     # Bin de boot (~integração + residuals)
│   │   └── src/
│   │       ├── main.rs    # Entry + boot 8 fases
│   │       ├── agents.rs  # Fleet nativo + registry
│   │       ├── cortex.rs  # Generate path, EventBus (residual)
│   │       ├── audio/     # Truth path voz (ADR-0045)
│   │       ├── net*       # NETSTACK singleton (residual)
│   │       └── fs/        # VFS monólito (residual)
│   ├── k_nano/            # Ring 0 — HAL, drivers, PCI
│   ├── k_ai/              # SelfHeal, Trust
│   ├── cortex/            # BitNet, Trinity, tensores
│   ├── hermes/            # WASM, rede, skills, cron
│   ├── jarbas/            # Display, GPU, persona
│   ├── agent-core/        # Agent trait, scheduler
│   ├── skill-registry/    # Skills, MCP
│   ├── event-bus/         # IPC pub/sub
│   ├── ticket-lock/       # Lock FIFO
│   └── boot/              # Bootloader 0.11 UEFI/BIOS
│
├── firmware/              # Blobs git-tracked (GPU/WiFi/NIC)
├── tools/                 # Scripts Python (build, treino, bridge)
├── docs/
│   ├── architecture/      # ADRs (0041–0047+)
│   └── memory/            # STATE.md, IDEA_BANK, Sessions
│
├── run-qemu-whpx.ps1      # QEMU WHPX + bridge serial
└── run-qemu-uefi.ps1      # QEMU UEFI
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
3. **SEMPRE execute `cargo clean -p neural-kernel && cargo nk` antes de considerar uma mudança completa.**
4. **SEMPRE execute `cargo clean -p neural-kernel && cargo nk` quando erros somem misteriosamente** — cache incremental mascara erros.
5. **Builds isolados:** usar `--target-dir target/agent-<nome>` (ou `target/check-<nome>`, etc.) — **nunca** `target-*` na raiz do repo.
6. **SEMPRE verifique `TECNOLOGIAS.md` antes de decidir qual tecnologia usar** — evite reinventar a roda.
7. **NUNCA adicione dependências externas** sem justificativa forte. O kernel é no_std. Prefira implementar do que adicionar crates.
8. **Skills a quente:** "grava video" ou "imprime formulario" viram skills gerados pelo LLM sob demanda, não por enum Rust.
9. **Documente no `AGENTS.md`** qualquer lição crítica que você aprender durante a sessão.

#### 8.3 Workflow do Agente

```mermaid
flowchart TD
    A[Ler AGENTS.md] --> B[Ler STATE.md + TODO.md]
    B --> C[Entender sprint atual]
    C --> D{Implementar?}
    D -->|Sim| E[Buscar no TECNOLOGIAS.md referências]
    E --> F[Implementar com #![no_std]]
    F --> G[cargo nk]
    G --> H{0 erros?}
    H -->|Sim| I[Documentar lição em AGENTS.md]
    H -->|Não| J[Corrigir + cargo clean -p neural-kernel]
    J --> G
    I --> K[Commit + push]
```

#### 8.4 Comandos Úteis para Agentes

```bash
# Verificar compilação (canônico)
cargo nk

# Check com clean (revela erros mascarados)
cargo clean -p neural-kernel && cargo nk

# Build isolado (agentes paralelos)
cargo nk --target-dir target/agent-<nome>

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
Consulte o plano histórico em `docs/archive/sprints/sprint-plan-v1.1.x.md`.

### 3. Compilar
```bash
cargo nk
# Se erros estranhos:
cargo clean -p neural-kernel && cargo nk
```

### 4. Testar em QEMU
```powershell
# 1) Build kernel  2) FAT32 em target\  3) UEFI + disco + logs\
cargo build --release
python tools\build_image.py
.\run-qemu-uefi.ps1 -Window
```

### 5. Verificar log
```powershell
# Último log gerado
Get-ChildItem logs\boot_uefi*.txt | Sort-Object LastWriteTime -Descending | Select-Object -First 1

# Procurar erros
Get-Content (Get-ChildItem logs\boot_uefi*.txt | Sort-Object LastWriteTime -Descending | Select-Object -First 1) | Select-String "ERROR|FAIL|PANIC"
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
| `error: could not compile neural-kernel` + erros estranhos | Cache incremental | `cargo clean -p neural-kernel && cargo nk` |
| `[MBR] Signature 55AA nao encontrada` | Boot image nao tem FAT32 | `python tools\build_image.py` → `target\disk_qemu.raw` |
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

> **AIOS K²CHJ — Neural OS Hermes v1.8.0**
> *"O hardware real não perdoa. O silício obedece."*
