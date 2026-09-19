# ADR-0106 M4 — Sítios excluídos da migração (OUT)

**Status:** closed (s366)  
**Regra:** braço por **sintaxe/contrato** permanece código — migrar degradaria determinismo.

## Tabela OUT (justificativa)

| Área | Exemplos | Por que OUT |
|------|----------|-------------|
| CLI Hermes | `parse_command`, `Command::*`, slash `/install` | Contrato de superfície; não juízo |
| JSON-RPC / MCP | method names, id parsing | Wire protocol |
| FAT / Limine | 8.3 names, `kernel.elf`, ESP GUID | Layout de mídia (SESSION_252) |
| GPT / MBR | type codes `0xEF`/`0x0C`/`0xEE` | On-disk contract |
| PCI / USB | VID:DID tables, BAR size probe | Hardware identity |
| GGUF / BitNet | ggml type IDs, magic `0xBE11BE11` | File format |
| Scancodes | set-1 make/break, Shift/Caps | Input encoding |
| Host ABI WASM | allowlist `aios::log` exact match | Cap surface, não semântica |
| Theme enum | `Theme::` / palette rails | UI contract |
| Tick/IRQ numbers | PIT vector, LAPIC mode | Platform contract |

## IN já migrados (não listar como OUT)

`intent.think`, `skill.risk`, `compute.tier`, `emotion.hint`, `skill.creation`, `plugin.scan`, marketplace risk merge, wasm PermissionGate→classify.

## Processo

Novo candidato: triar em M0 → se sintaxe, **adicionar linha nesta tabela** com justificativa; se significado, fixture de paridade + `Decision`/`Noul`.
