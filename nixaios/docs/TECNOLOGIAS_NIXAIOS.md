# TECNOLOGIAS — NixAIOS (espelho PORT/ADAPT)

Catálogo vivo do produto paralelo. Bare-metal continua em [`TECNOLOGIAS.md`](../../TECNOLOGIAS.md) §1–15.

**Canónico WASM:** ADR-0059 (Caminho A wasmi). **ADR-0032 = superseded** — não usar como inventário.
**Frugal NixAIOS (0118):** heap **256 KiB = 4 páginas** WASM (página=64 KiB). Fuel **100_000**.
Bare-metal `hermes::wasmi_rt` usa `DEFAULT_FUEL=5_000_000` — perfil diferente de propósito.

Taxonomia ADR-0113: scaffold **IMPLEMENTED** ≠ forward GGUF/CUDA/metal **VERIFIED**.

| # | Tecnologia | Disposicao | Crate / path | Status (honest) |
|---|------------|------------|--------------|-----------------|
| N.1 | nosd daemon (Tokio + notify) | PORT wire | `nosd` | IMPLEMENTED; smoke host OBSERVED |
| N.2 | EventBus std | PORT | `nos-bus` | IMPLEMENTED scaffold |
| N.3 | CapGate IPC | PORT | `nos-ipc` + caps | IMPLEMENTED scaffold |
| N.4 | ModelHub 8 slots + LOO | PORT | `nos-cortex` | IMPLEMENTED scaffold |
| N.5 | HeaderScanner GGUF/Safetensors | PORT | `nos-cortex` | IMPLEMENTED header only |
| N.6 | HotSwapController | ADAPT | `nos-cortex/hotswap` | CpuEcho default; LlamaGguf via **`llama-completion`** (Nix); EXL2/vLLM EngineAbsent |
| N.7 | App Factory path A | PORT | `nos-factory` | IMPLEMENTED scaffold (0059) |
| N.8 | wasmi Frugal 256KiB/100k | PORT política | `nos-wasm` | Host **OBSERVED**: fleet 32+128 identity + fuel trap (`smoke-frugal-fleet.sh`); fuel dinâmico = HYPOTHESIS |
| N.9 | neural-sgdb memory | PORT | `nos-memory` | IMPLEMENTED facade file; recall WSL **OBSERVED** |
| N.10 | Hermes agentic | PORT | `nos-hermes` | chat→GGUF→remember WSL **OBSERVED** (`smoke-cognitive-loop.sh`) |
| N.11 | BEI tick fino | PORT | `nos-bei` | IMPLEMENTED scaffold |
| N.12 | Mesh ADR-0081 | PORT | `nos-mesh` | IMPLEMENTED scaffold |
| N.13 | Distrib assign | PORT | `nos-cortex::distrib` | IMPLEMENTED scaffold |
| N.14 | JARBAS Slint kiosk | ADAPT | `nos-jarbas` | IMPLEMENTED stub UI |
| N.15 | Voice Whisper/Supertonic | ADAPT | `nos-voice` | EngineAbsent stub |
| N.16 | NixOS flake + modules | NOVO | `nixaios/` | IMPLEMENTED scaffold |
| N.17 | ISO offline | NOVO | `images/iso.nix` | QEMU OBSERVED; metal UNKNOWN |
| N.18 | ai-managed.nix HITL | NOVO | `modules/ai-managed.nix` + `nix.propose` IPC | CapGate NIX_WRITE → staging; switch **proibido** no daemon; HITL |
| N.19 | CUDA GGUF `-ngl` | ADAPT | `capacity` nvidia-smi + `NOSD_LLAMA_NGL` + flake `llama-cpp-cuda` | Host GPU WSL **OBSERVED** (RTX 3050); nixpkgs `llama-cpp` = **CUDA_ENGINE_ABSENT**; forward GPU = AWAITING `.#llama-cpp-cuda` |
| N.20 | Metal USB ISO | NOVO | `metal-iso-deep-preflight.sh` + `metal-usb-handoff.ps1` + Rufus DD | Preflight software **OK**; boot metal **AWAITING_HW** |

### Onda 2 — fecho software (2026-10-21)

| Item | Evidência | Status |
|------|-----------|--------|
| Honesty docs 0032/fuel/páginas | TECNOLOGIAS + ADR refs | DONE |
| HotSwap GGUF `llama-completion` | smoke-cognitive-loop | OBSERVED WSL |
| Capacity VRAM | `posture=nvidia_smi` slog | OBSERVED WSL |
| Cognitive loop | tokens + remember/recall | OBSERVED WSL |
| wasmi Frugal fleet | smoke-frugal-fleet 32+128 | OBSERVED host |
| nix.propose HITL | CapGate NIX_WRITE staging | IMPLEMENTED |
| CUDA `-ngl` | smoke-cuda-honesty → ENGINE_ABSENT | OBSERVED honesty |
| Metal USB boot | Rufus DD + copytoram | **AWAITING_HW** |
