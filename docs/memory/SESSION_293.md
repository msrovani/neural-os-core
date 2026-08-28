# SESSION_293 — QEMU UEFI boot debugging, Falcon3 GGUF/1.58-bit, OVMF pflash, RAM converter fix

**Objetivo:** Rodar QEMU 4 cores com Falcon3 como LLM, monitorar NSGDB e Jarbas greeting, validar re-boot cross-session com inferências NSGDB.

## Problemas encontrados

### 1. OVMF pflash dual-file (crítico)
`target/ovmf.fd` estava zerado/corrompido. QEMU com `-bios ovmf.fd` combinado (CODE+VARS) sem NVRAM entries → EFI shell em vez de Limine.

**Fix:** Separar em `target/ovmf_code.fd` (readonly) + `target/ovmf_vars.fd` (leitura/escrita):
```
-drive if=pflash,format=raw,file=target/ovmf_code.fd,readonly=on
-drive if=pflash,format=raw,file=target/ovmf_vars.fd
```
O VARS precisa de NVRAM entries (Boot0001→Limine). Template correto: `edk2-x86_64-code.fd` do QEMU como CODE, VARS zerado aceita no 1º boot mas o UEFI deve encontrar a ESP.

**Lição:** OVMF combinado CODE+VARS só funciona se o VARS já tem boot entries salvas. Para boot fresh, pflash dual é mandatório.

### 2. QEMU serial 0 bytes via Git Bash/MSYS
Comando QEMU com paths MSYS (`/c/Users/...`) → QEMU não encontra arquivos ou serial não conecta.

**Fix:** Python `subprocess` com `os.path.normpath()` para paths Windows nativos (`C:\Users\...`). `qemu_boot_stdio.py` — launcher funcional que:
- Descobre modelos em `target/` + `E:\modelos` + `D:\modelos`
- Monta loaders `-device loader,file=...,addr=0x100000000`
- Usa `-serial stdio -nographic` com captura stdout→log
- **Resultado:** O boot UEFI + Limine + kernel serial funciona via esse script

### 3. Falcon3 GGUF i2_s não suportado
O GGUF `Falcon3-7B-Instruct-GGUF` (4.1GB, i2_s quantization = ternário) baixado de `bartowski/Falcon3-7B-Instruct-GGUF` usa type=25 (i2_s). O kernel `cortex::gguf` só suporta Q4_0 até Q6_K — não carrega i2_s.

**Decisão:** Falcon3-7B 1.58-bit NÃO é GGUF. O modelo nativo é `tiiuae/Falcon3-7B-Instruct-1.58bit` em safetensors. Conversão via `convert_falcon3_bitnet.py` → `.v6` (BitNet format, magic `0xBE11BE11`).

### 4. RAM insuficiente para conversão 7B
Host: 7.5GB total, 1.3GB livres. Conversão Falcon3-7B (3.27GB safetensors + overhead ~1.5GB) → OOM.

**Fix aplicado (commit `ac3d3ef`):**
- `t.float()` → `t.half()` — 50% RAM menos para bf16/f32 tensors
- `gc.collect()` entre etapas de load — libera memória incremental
- In-place quantize (`out=x`) — zero array intermediário

**Resultado:** Conversão 3B funciona (custa ~1.5GB). 7B ainda precisa de ≥16GB RAM no host.

### 5. neural-sgdb como dependência externa
`neural-sgdb` não é builtin — precisa ser clonado de `github.com/msrovani/neural-sgdb` em `crates/neural-sgdb/`. Sem ele, `k_ai` não compila.

### 6. run-qemu-uefi.ps1 encoding
O script PS1 tem UTF-8 com em-dashes (`—`) que PowerShell pode mal interpretar. Python launcher (`qemu_boot.py` / `qemu_boot_stdio.py`) é alternativa mais robusta.

## Estado do PR `fix/scheduler-stack-overflow-pf`
4 commits pushados (3 anteriores + DNS fix `0e26a16`):
1. `e853393` — Scheduler stack 2MB→8MB
2. `83e8916` — Model paths D:\modelos
3. `37f2b96` — chore: remove .freebuff/ e .bitnet
4. `0e26a16` — DNS via gateway (bypass ARP SLIRP virtual)

## Scripts debug (untracked — descartáveis)
- `tools/check_esp.py`, `tools/check_esp2.py` — debug GPT/ESP
- `tools/find_falcon.py`, `tools/find_falcon2.py`, `tools/find_models_deep.py` — busca modelos
- `tools/list_models.py` — lista modelos
- `tools/qemu_boot.py`, `tools/qemu_boot_stdio.py`, `tools/run_qemu_boot.py` — launchers QEMU
- `tools/run_qemu_models.py` — QEMU com loaders

## Aprendizado consolidado
1. **OVMF pflash dual** é obrigatório para boot fresh (não existe VARS salvo)
2. **Paths MSYS→Windows** são a causa #1 de QEMU silencioso — usar Python subprocess
3. **Falcon3 1.58-bit** = safetensors TII → `.v6` BitNet, NÃO GGUF
4. **GGUF i2_s** = type 25, kernel só suporta Q4_0–Q6_K (type 2–8)
5. **float16 converter** reduz RAM 50% — viabiliza 3B em 8GB, 7B precisa 16GB+
6. **neural-sgdb** é repo externo, precisa clone manual em `crates/`

## Failures
- QEMU 4 cores serial 0 bytes (OVMF pflash issue)
- Falcon3-7B OOM no host (7.5GB insuficiente)
- GGUF i2_s not supported in kernel loader
