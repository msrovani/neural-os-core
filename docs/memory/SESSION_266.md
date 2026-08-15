# SESSION_266 — Jarbas UI cleanup + suit-boot welcome (MCU-inspired)

**Escopo:** Limpar legado visual do Jarbas, refinar UX do desktop (orb = brand),
e fala de boas-vindas estilo suit-online (Iron Man / JARVIS no MCU) via LLM +
template honesto + banner HUD + TTS (HERMES_RESPONSE).
**Status:** ✅ código + `cargo check --release` (jarbas + neural-kernel) 0 erros.

---

## 1. Análise (estado pré-mudança)

- **Hero visual:** Soul Mirror + grafo mesh (SESSION_261) — correto.
- **Legado:** `JarbasAvatar` (partículas) morto; `Avatar8.render` competia com o
  orb; hub roxo do mesh **duplicava** o orb; demo card ADR-0047 no boot; barra
  de status tipo dashboard (`t:`, `ag:`=windows, LLM via `OVERLAY_TEXT` morto);
  footer "[Modo Ambiente…]"; `ensure_hermes_overlay` no-op; dock com stubs
  Ide/Camera/Audio.
- **Saudação:** já existia em `audio/jarvis.rs` (LLM + template), tom genérico
  "Good day…", sem HUD visual de suit-boot.

## 2. Referência MCU (pesquisa)

Mark II upload (Iron Man 2008): diálogo "At your service, sir" /
"I have indeed been uploaded, sir. We're online and ready" / calibrate HUD /
control surfaces. **Texto do filme não é copiado** — JARBAS usa fala **original**
no mesmo *espírito* (upload → online → HUD → fleet → at your service).

## 3. Mudanças

| Área | Mudança |
|------|---------|
| `avatar.rs` | Só FFT + `AvatarState`; remove `JarbasAvatar`/partículas |
| `console.rs` | Flag `llm_busy` atômico; sem buffer overlay legado |
| `compositor.rs` | HUD: **JARBAS** + mem/LLM/NET; welcome banner; mesh só com peers (sem hub duplicado); sem Avatar8 particles; dock Chat+Power; remove `ensure_hermes_overlay` |
| `agent.rs` | Sem demo UI_SPEC no boot; `mark_ui_ok` |
| `audio/jarvis.rs` | Template + prompt LLM suit-boot; `announce_welcome`; calibração HUD antes do LLM |
| `cortex/bpe.rs` | `text_is_greetingish` aceita léxico suit-boot (upload/HUD/fleet/service) |

## 4. Verificação

- `cargo check -p jarbas -p cortex --release` — 0 erros
- `cargo check -p neural-kernel --release` — 0 erros

## 5. Follow-ups

- TTS formant na welcome (já via HERMES_RESPONSE → synthesize) — validar áudio em HW.
- IDEA #533 click-to-inspect no grafo.
- Dock draw path ainda residual (WM actions usam Dock sem paint no render).
