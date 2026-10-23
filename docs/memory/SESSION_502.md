# SESSION_502 — NixAIOS Onda 3: MoE S1/S2 + Cage kiosk minimal

**Data:** 2026-10-22  
**Escopo:** `nixaios/` (bancada paralela ADR-0118; não fork bare-metal)

## Objectivo

Continuar migração neural-os-core → NixAIOS alinhada ao manifesto Cortex/Hermes MoE Hub: appliance **minimal** (sem GNOME), JARBAS via `cage` na iGPU, roteamento Sistema 1 / Sistema 2, Factory A spawn.

## Feito

1. **`nos-cortex::route`** — `route_intent` → `RouteDecision{slot,lane,reason}`; S1 Active/HwExpert/RustCoder; S2 GeneratorPro; testes 3/3.
2. **`nos-hermes`** — ciclo Observe→Plan→Act→Verify→Remember com LOO por slot; S2 `sync_marked` (async queue = residual, honesty); `spawn_wasm_agent` Factory A; `cognitive_tick`; `route.explain`.
3. **IPC / nosctl** — métodos `route.explain`, `cognitive.tick`, `agent.spawn`; CLI `route` / `spawn` / `tick`.
4. **ISO minimal** — `services.jarbasCage.enable=true` + `nosdAiManaged.enable=true`; `graphical.target`; getty@tty1 off; mesa/vulkan/cage; dirs models+nsgdb+XDG.
5. **`jarbas-cage.nix`** — seatd, tmpfiles `/run/user/nosd`, groups video/render/seat, `cage -s -- nos-jarbas`.

## Prova host

`cargo test --workspace` em `nixaios/packages/nosd` (`CARGO_TARGET_DIR=target/nixaios-onda3`) — **PASS** (cortex 17, hermes 2, factory 2, wasm 5, …).

## Honesty (ADR-0113)

| Claim | Status |
|-------|--------|
| MoE keyword routing | VERIFIED host tests |
| S2 async queue dedicada | UNKNOWN / residual (`sync_marked`) |
| Cage DRM + HUD Slint metal | UNKNOWN (módulo ON na receita; boot metal AWAITING) |
| llama.cpp / CUDA engines | EngineAbsent / opt-in (Onda 2) |
| neural-sgdb crate real | facade file |
| ISO Downloads rebuild com Onda 3 | UNKNOWN até `nix build .#iso` de novo |

## Próximo

1. Rebuild ISO Onda 3 + QEMU/WSL smoke (nosd + route + spawn).  
2. Fila S2 async (não bloquear tick).  
3. Slint HUD real no `nos-jarbas`.  
4. Metal USB Rufus DD + copytoram.
