# SESSION_259 — Keyboard modifiers (Shift/CapsLock/break) + BrokenThorn OSDev mining (2026-08-12)

**Escopo:** Teclado PS/2 com Shift/CapsLock/break codes (fix de gap real) + mineração da série
OSDev do BrokenThorn no mempalace + scan de código (14 tópicos) para referências usáveis.
**Status:** ✅ Fechada — 1 commit — 0 erros — 11 testes k_nano PASS.

---

## 1. BrokenThorn OSDev — Mineração (mempalace)

Pedido: mine `https://brokenthorn.com/Resources/` no mempalace + escaneie o código por referências
que o neural-os pode usar.

- **Fonte:** site dá **403 direto** (brokenthorn.com/Resources/); **Wayback
  `web.archive.org/web/2024/...` funciona** (capturas 2024-2026). Página índice + OSDev19
  (teclado/8042) + OSDevScanCodes (set 1/2, AT, ACPI, multimedia) extraídos verbatim.
- **Mineração:** 5 drawers no room `neural-os-core/brokenthorn-osdev` (índice da série; OSDev19
  teclado — portas 0x60/0x64, status register bits, encoder/controller commands, return codes;
  scan codes set 1 XT; set 2 moderno + ACPI + multimedia; AT set 1). Diary registrado.
- **Conteúdo público domain** (série BrokenThorn é public domain; copyright 2009 BrokenThorn).

## 2. Scan de código — 14 tópicos (explorer)

Mapa BrokenThorn → código neural-os:

| Tópico | Status neural-os | Usável |
|---|---|---|
| Keyboard (T19 + Scan Codes) | **gap real** — `scancode_to_ascii` só-lowercase, sem shift/caps/break | ✅ implementado nesta sessão |
| 8042 self-test/interface (0xAA→0x55, 0xAB) | PS/2 init existe, sem diagnóstico | 🟡 #528 |
| Teclas E0 (setas/Insert/Home/End) + set 2 | prefixo 0xE0 ignorado; tabela set-1 only | 🟡 #529 |
| PIC/PIT (T16, 8259A/8253) | APIC primary; PIC stripped (interrupts.rs:5) | ❌ |
| PMM/VMM (T17/18) | completo (memory.rs, apic.rs) | ❌ |
| VFS (T22) | extenso (FAT32 + NeuralFS + 6 readers) | ❌ |
| DMA (T21) | PCI bus-master; sem 8237 | ❌ |
| FDC (T20) | ausente — storage = ATA/AHCI/NVMe/USB | ❌ (fora do alvo) |
| Process mgmt (T24/25) | Ring3 gated off (ADR-0060) | ❌ (já além) |
| PE parser | ausente no kernel (host Python pefile p/ ML) | ❌ baixa prioridade (#306a) |

## 3. Fix — Keyboard modifiers (commit)

**Causa raiz:** `scancode_to_ascii` era pura `(u8)->Option<char>` só-lowercase e o InputAgent
dropava break codes (`if scancode>=0x80 return`) sem rastrear Shift/CapsLock → sem maiúsculas,
sem shifted symbols (`!@#{}:…`), CapsLock inerte.

**Fix (fonte única em k_nano):**

1. **`crates/k_nano/src/scancode_to_ascii.rs`:** tabela pura `(scancode, shift, caps) -> Option<char>`.
   Letras uppercase iff `shift != caps` (XOR — shift OU caps = maiúscula, padrão PC). Dígitos
   shiftados (`!@#$%^&*()`), símbolos (`_ + { } : " ~ | < > ?`), espaço inalterado. Teclas
   faltantes `[ ] ; ' \` \ ,` (0x1A/0x1B/0x27/0x28/0x29/0x2B/0x33) absorvidas da cópia morta do bin.
2. **`crates/neural-kernel/src/agents.rs` (InputAgent que roda):** campos `shift`/`caps`; match em
   `key = scancode & 0x7F` (cobre make e break juntos): `0x2A | 0x36 => shift = pressed` (breaks
   `0xAA/0xB6` limpam), `0x3A if pressed => caps = !caps` (toggle só no make). `if !pressed return`
   fica APÓS o match (breaks limpam flags e saem).
3. **`crates/hermes/src/agents.rs` (espelho):** mesmo tratamento; já tinha `shift` + KEY_EVENT —
   preservado.
4. **`crates/neural-kernel/src/main.rs:4019`:** cópia morta `pub(crate) fn scancode_to_ascii`
   (uppercase, ~47 linhas, **zero callers** — verificado via grep) **DELETADA**.

**Testes:** 11 host PASS (7 novos: shift upper, caps upper, XOR lower, shifted digits/symbols,
caps≠digits, shift≠space, Enter shiftado None). **`cargo check --release`: 0 erros.**

## 4. Lições

- **Modifiers de teclado: função pura + estado no agente.** Break code = make|0x80 em set 1;
  match em `key & 0x7F` + flag `pressed` cobre make e break num arm só. CapsLock toggla só no
  make. Shift XOR caps para letras (ambos = minúscula). (AGENTS.md)
- **Cópia morta no bin = fonte de teclas "perdidas".** A tabela do bin tinha 7 símbolos que a
  k_nano não tinha — absorvidos antes de deletar.
- **brokenthorn.com dá 403 a bots; Wayback resolve.** Mineração em mempalace room
  `brokenthorn-osdev`.

## 5. Pendências (IDEA_BANK)

- #528: LEDs do teclado (0xED) + self-test 8042 (0xAA→0x55 / 0xAB) — feedback CapsLock + diag.
- #529: teclas E0 (setas/Insert/Home/End) + scancode set 2 — navegação no prompt do Hermes.
