# TECNOLOGIAS — NixAIOS (espelho PORT/ADAPT)

Catálogo vivo do produto paralelo. Bare-metal continua em [`TECNOLOGIAS.md`](../../TECNOLOGIAS.md) §1–15.

| # | Tecnologia | Disposicao | Crate / path | Status scaffold |
|---|------------|------------|--------------|-----------------|
| N.1 | nosd daemon (Tokio + notify) | PORT wire | `nosd` | IMPLEMENTED scaffold |
| N.2 | EventBus std | PORT | `nos-bus` | IMPLEMENTED scaffold |
| N.3 | CapGate IPC | PORT | `nos-ipc` + caps | IMPLEMENTED scaffold |
| N.4 | ModelHub 8 slots + LOO | PORT | `nos-cortex` | IMPLEMENTED scaffold |
| N.5 | HeaderScanner GGUF/Safetensors | PORT | `nos-cortex` | IMPLEMENTED scaffold |
| N.6 | HotSwapController | ADAPT | `nos-cortex/hotswap` | IMPLEMENTED + EngineAbsent vendors |
| N.7 | App Factory path A | PORT | `nos-factory` | IMPLEMENTED scaffold |
| N.8 | wasmi Frugal 256KiB/100k | PORT política | `nos-wasm` | IMPLEMENTED scaffold |
| N.9 | neural-sgdb memory | PORT | `nos-memory` | IMPLEMENTED facade file |
| N.10 | Hermes agentic | PORT | `nos-hermes` | IMPLEMENTED scaffold |
| N.11 | BEI tick fino | PORT | `nos-bei` | IMPLEMENTED scaffold |
| N.12 | Mesh ADR-0081 | PORT | `nos-mesh` | IMPLEMENTED scaffold |
| N.13 | Distrib assign | PORT | `nos-cortex::distrib` | IMPLEMENTED scaffold |
| N.14 | JARBAS Slint kiosk | ADAPT | `nos-jarbas` | IMPLEMENTED stub UI |
| N.15 | Voice Whisper/Supertonic | ADAPT | `nos-voice` | EngineAbsent stub |
| N.16 | NixOS flake + modules | NOVO | `nixaios/` | IMPLEMENTED scaffold |
| N.17 | ISO offline | NOVO | `images/iso.nix` | IMPLEMENTED receita |
| N.18 | ai-managed.nix HITL | NOVO | `modules/ai-managed.nix` | IMPLEMENTED scaffold |
