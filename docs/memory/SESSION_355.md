# SESSION_355 — k_hal análise + bughunt (R1 honesty)

## Goal
Analisar papel/premissas do `k_hal` (Ring 1) e bughunt aprofundado: falso sucesso, rings Intel, RF fabricado, BAR decode.

## Papel (resumo)
R1 sensório-motor: DeviceTree/`HalOffer`/CapGate, DeviceRecipe+UnlockDAG, USB hub→MSC BE, GPU detect/canary/VRAM, WiFi BE, VirtIO transporte. Premissas: `has_compute` só pós-canary; Ok≠Ready; CapGate deny R3; deps só `k_nano::*`.

## Onda 1 (HIGH)
| Bug | Fix |
|---|---|
| `page_flip_hw` “vblank” = poll `DSPCNTR_ENABLE` após setar ENABLE | readback `DSPSURF` + TSC 50ms; só dword 32-bit GGTT |
| `kv_dma::wait` infinito; `dir as u32`; sem VRAM_READY | `wait()`→bool; match `DmaDir`; gate ready; path=`bar_memcpy` |
| `VramBuddy` ceil → pool > VRAM | `floor_order` |
| `H1_RAN` Relaxed | Acquire/Release |

## Onda 2 (profunda)
| Bug | Fix |
|---|---|
| RCS @ `0x120000` (hex a mais); TAIL no slot START; `CTL=4096` | Gen9 `0x2000` + START GGTT + `CTL=0x3001` |
| `tail` em dwords vs HEAD/TAIL HW em bytes (+ wrap) | bytes + `RING_PTR_MASK` (RCS+BCS) |
| `iwlwifi::scan` fabricava JARVIS-NET/MeuWiFi | 0 APs + slog warn |
| `generic_wifi` `bar0\|bar1<<32` | `pci_bar::decode_bar` |
| ath10k_fw / pipeline_g5 `unwrap` | fail-closed |
| CapGate DENY sub=`info` (TRACE mudo) | sub=`warn` |

## Arquivos
- `crates/k_hal/src/gpu/{intel,intel_display,kv_dma,vram,pipeline_g5}.rs`
- `crates/k_hal/src/net/{wifi_iwlwifi,generic_wifi,ath10k_fw}.rs`
- `crates/k_hal/src/{cap_gate,lib}.rs`

## Verify
- `cargo check -p k-hal --release` — **0 erros**

## Aberto
- Parse real beacon/probe iwlwifi; vblank PIPESTAT; aceite metal blit/RCS pós-fix offsets.

## Lições
1. Mesma classe ADR-0087: hex a mais no base MMIO (0x120000 / 0x220000) + TAIL/START trocados = ring morto.
2. Fabricar SSID no scan é pior que timeout — honesty RF.
3. HEAD/TAIL Gen9 = bytes mascarados, não índice dword cru.
