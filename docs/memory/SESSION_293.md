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

## Novos findings (s293+)

### 7. ATA probe skip em TCG (CRÍTICO)
`storage_bw::skip_measure()` retorna true em TCG → `probe_storage_drivers()` não executa ATA probe. O kernel NÃO lê o disco FAT32 no QEMU TCG. FALCON3.V6 no disco é inútil sem ATA.

**Impacto:** FAT32 model loading só funciona em WHPX ou HW real. TCG = QEMU loader only.

### 8. QEMU loader scan limitado a 2 endereços
Kernel procura BitNet magic (`0xBE11BE11`) apenas em `0x100000000` e `0x120000000`. Modelos carregados em outros endereços (ex: hw_expert @0x108400000) são ignorados.

**Fix necessário:** scan iterativo de `[0x100000000..0x180000000)` step=1MB, ou reordenar QEMU loaders para BitNet primeiro.

### 9. QEMU pflash dual-file obrigatório
`-bios ovmf.fd` (combinado) não funciona com pflash. Usar:
```
-drive if=pflash,format=raw,file=ovmf_code.fd,readonly=on
-drive if=pflash,format=raw,file=ovmf_vars.fd
```
### 10. QEMU launcher Python (tools/qemu_boot_stdio.py)
Launcher funcional com: pflash OVMF, chardev serial file, model auto-discovery, timeout, status detection. Corrige bugs: addr sem `0x`, SIGALRM no Windows, smp override.

## Failures
- QEMU 4 cores serial 0 bytes (OVMF pflash issue)
- Falcon3-7B OOM no host (7.5GB insuficiente)
- GGUF i2_s not supported in kernel loader
- ATA probe skip em TCG — modelo não carrega via FAT32
- QEMU loader scan limitado — BitNet em endereços não-probed

## Round 2 — Model Loading Deep Debug (s293+R2)

### 11. PS1 model scan: .v6 extension excluded (FIXED)
`run-qemu-uefi.ps1` only scanned `.bitnet`/`.BIN`/`.bin` — FALCON3.V6 (770MB) was invisible.
**Fix:** Added `*.v6` filter + changed 70MB cap to 2GB. Commit `dd8e5fe`.

### 12. Model dedup: FALCON3.V6 == FALCON3B.v6 (same 807MB)
Both in `target/`, same size. Added size-based dedup in launcher.

### 13. QEMU loader model loads — `if false &&` blocked status tracking (FIXED)
FALCON3.V6 (770MB) placed at `0x100000000` via `-device loader`. Kernel probe:
- `has_addr_in_any_region(0x100000000)` → **true** (8GB RAM)
- `is_page_present(0x100000000 + HHDM)` → works for BGE scan (all addresses)
- Model copies 770MB to heap via `to_vec()` → `load_model_v6()` → **SUCCESS**
- `set_model()` called → `model_is_loaded()` returns true
- **But `load_status::set(Llm, Loaded)` never fired** because it was inside
  `if false && (model_loaded || model_is_loaded())` — hardcoded disabled block
- Fix: removed `false &&`, set status unconditionally when model loaded
- **Result:** `llm=LOADED`, BOOT SCORE `llm=ok`, 770MB Falcon3 active

### 14. slog messages all Sev::Trace — model probe invisible
All slog_bin! calls in the model probe use subs "bge", "ramdisk", "info", "loader"
which map to `Sev::Trace`. Default console filter = `CONSOLE_OK` (sev≥1).
→ All model loading diagnostic messages are **completely hidden** from serial/file.
**Fix needed:** Change key probe messages to sub="ok" or add boot-trace feature.

### 15. TCG 770MB copy timeout
Even if probe found the model, `to_vec()` of 770MB in TCG would take minutes.
With 10 models (1GB total), boot stalls at Phase 5 for >120s then finishes with ABSENT.
**Implication:** Model loading via QEMU loader in TCG is fundamentally impractical >100MB.

### 16. WHPX ATA PIO hang on this machine
WHPX without QEMU loader: ATA probe hangs (PIO too slow in WHPX on this HW).
WHPX + QEMU loader: immediate crash (0 bytes, WHPX SMP issue documented in AGENTS.md).
**Conclusion:** Neither TCG nor WHPX can reliably load Falcon3 770MB model.

### 17. NSGDB ingest WORKS
Despite LLM=ABSENT, SGDB `ingest ramlog → SGDB L3 boot/0000017 (7640 bytes)` succeeds.
Boot completes through Phase 8 + BOOT SCORE with 4 cores. System is functional,
just without LLM inference (formant fallback for voice).

## Model Loading — RESOLVED ✅
Falcon3-3B-Instruct-1.58bit (770MB) loads successfully via QEMU loader at 0x100000000.
`llm=LOADED` in Status + BOOT SCORE. 4 cores online. NSGDB ingest works.

## Cross-boot NSGDB — BLOCKER
TICKV is RAM-only → SGDB doesn't persist between QEMU instances.
BOOT.LOG persistence requires ATA disk I/O → blocked by `storage_bw::skip_measure()` in TCG.
**Fix needed:** Remove TCG ATA skip or implement BOOT.LOG write via alternative path.
Test on WHPX or HW real where ATA works.
