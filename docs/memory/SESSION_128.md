# SESSION_128 — ADR-0046 hot-swap Net→FAT→AirLLM

**Data:** 2026-07-16  
**Objetivo:** Completar residual hot-swap (Net HTTP → ATA → `set_model`) de forma viável e honesta.

## Ja existia (SESSION_127)
- `GGUFStreamingModel` + `hot_swap_from_ata` → `load_gguf_streaming` → `set_model`
- `/model <FAT32>` no HermesAgent (`neural-kernel/agents.rs`)
- `net::http_get` via smoltcp/e1000

## Adicionado
| Item | Detalhe |
|------|---------|
| FAT writer 8.3 | `encode_83` em find/create/update (`fat32.rs`) — gap que quebrava write `NAME.EXT` |
| `write_fat_file` | `gguf.rs` — write root FAT32 |
| `hot_swap_from_net` | HTTP GET → FAT → `hot_swap_from_ata`; cap 64MiB staging |
| Comandos | `/model http://…`, `/model-fetch` alias |
| Erro L3.5/RX | Se `http_get` None e RX=0/sem delta → nao finge OK |
| Logs | `[AIRLLM] hot-swap ATA/Net OK|FAIL` |

## Como usar
```text
/model MODEL.GGUF
/model http://10.0.2.2:8080/tiny.gguf
/model-fetch http://10.0.2.2:8080/tiny.gguf HOTSWAP.GGUF
```

## Limites honestos
- e1000 RX ainda pode ser 0 (Sprint Net) → falha L3.5/RX
- Sem DNS hostname / HTTPS
- Body inteiro em RAM (64MiB cap); stream-to-disk residual
- Prefetch ainda soft (nao DMA)
- SLIP nao usado como gate

## Evidencia
```text
cargo nk --target-dir target/check-hotswap → 0 erros
```

## Docs
ADR-0046 §3.2b/c, INDEX, STATE, TODO, IDEA #449 (AirLLM), TECNOLOGIAS 2.10f2
