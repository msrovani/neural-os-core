# SESSION_306 — Mesh 4c Master/Worker + slog visível + Jarbas cursor/orb

**Data:** 2026-09-03 | **Sprint:** v1.9.99-s306 TEST | **Status:** ✅ PASS parcial (mesh eleição); Falcon dual ❌ RAM host

---

## Objetivo

Duas instâncias QEMU 4c em socket P2P mesh, 5 min, arbitrar Master/Worker e monitorar log + NSGDB; também estabilizar cursor/orb (trails/freeze) que estavam no working tree.

## Causa raiz — mesh “invisível”

1. **`slog` P2P/Net com `sub=info|tick`** → `Sev::Trace` (mudo na serial). Eleição podia existir sem evidência.
2. **`detect_qemu_net_mode`** só varria `ram_end` 1 MiB a 1 MiB; flag em `0x13E000000` (script) podia falhar / demorar.
3. **STATIC sem gateway slirp** ainda tentava L3.5 ARP/DNS/HTTP → smoke mentiroso no socket mesh.
4. **`target1/uefi.img` stale** vs `target/uefi.img` fresco — `run-qemu-p2p-mesh.ps1` prefere `target1/` → kernel velho.

## Fix mesh / netmode

| Item | Mudança |
|---|---|
| Visibilidade | P2P `MESH_ENGINE` / `TX heartbeat` / `TOFU bind` / `mesh role=` → `sub=ok`; Net bootstrap `tick`→`ok` |
| Detect | Candidatos canônicos ≥4 GiB (`0x13E000000`, …) dentro de `ram_end`, depois scan |
| STATIC | Trata como dev; **skip L3.5/L4/L5** após static IP |
| Imagem | Sync `target/uefi.img` → `target1/uefi.img` pós-rebuild |

## Evidência QEMU (2026-09-03)

```text
2× QEMU 4c / 5G / TCG / NoDisk / NoModels / socket 127.0.0.1:12345
A=10.0.3.2  B=10.0.3.3  netmode loader @0x13E000000
PIDs 6996 / 13120 — vivos após ≥5 min

A: netmode=STATIC 10.0.3.2; MESH_ENGINE node_id=2; TOFU bind node=3; mesh role=Master
B: netmode=STATIC 10.0.3.3; MESH_ENGINE node_id=3; TOFU bind node=2; mesh role=Worker→Memory
GOAL2_election=true (mesh_log_parser)
TICKV backend=RAM (VOLATIL) — NoDisk honesto
llm degraded / Falcon3 NÃO carregado — FreeGB host ~5 (2× Falcon inviável)
```

## Jarbas / mouse (mesmo working tree)

- Cursor: paint só no compositor + underlay; IRQ não pinta trails; `MOUSE_PORT_LOCK` (um consumidor 8042).
- `swap`: sem spin infinito em `CURSOR_LOCK`; dirty-rect `swap_rect` (HUD/orb/cursor).
- Orb: soul_mirror mais leve; TTS boot TCG formant (evita Piper/republish starvation).

## Arquivos

- `crates/hermes`: `net.rs`, `network_agent.rs`, `netstack.rs`, `agents/mouse_agent.rs`
- `crates/k_nano`: `net/mesh.rs`, `interrupts.rs`
- `crates/jarbas`: `display/{compositor,fb,soul_mirror}.rs`, `audio/jarvis.rs`

## Lições

1. Evidência mesh exige `ok|warn|fail` — mesma classe ADR-0092 / SESSION_289.
2. Socket P2P ≠ user/slirp: STATIC sem gw; não prove ARP `10.0.3.1`.
3. Preferência `target1/uefi.img` no script mesh exige sync pós-`cargo build -p boot`.
4. Dual Falcon3 + 2×4c precisa host com folga >>5 GB livres (honestidade AIOS).

## Validação

```powershell
cargo build --release -p boot
Copy-Item target\uefi.img target1\uefi.img -Force
# launch 4c mesh NoModels NoDisk; grepar mesh role= / TOFU / netmode=STATIC
```
