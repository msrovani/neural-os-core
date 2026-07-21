# ADR-0058: Generative Card Desktop — UI/Desktop do Jarbas (embedded-graphics + UiDeclaration)

**Data:** 2026-07-21
**Status:** Accepted — **S1–S4 implementados** (QEMU: 3 cards + orb responsivo + HUD relógios; self-tests S1/S2 PASS; clique fecha card). S5 (widgets ricos/tema/TTF) e A/V real (mic/alto-falante/vídeo via HDA/UVC) = residual.
**Lifecycle (INDEX):** `fazendo`
**Unifica / supersede (parcial):**
- **ADR-0047-HMI** (Neural Desktop) — absorve H1 (UI_SPEC/UiDeclaration), H2/H5 (embedding/splats como widgets), H4 (avatar telemetria). H3 (renderer neural/diffusion) permanece ❌ descartado.
- **ADR-0014 §UI/desktop seeds** — CorePools/SMP não; só as sementes de HMI/compositor.
**Estende (não substitui):** ADR-0036 (JARVIS = persona do Hermes — inalterada; ADR-0058 é só a camada de render/UI).
**Amarra:** ADR-0052 (contrato de artefato p/ skills WASM que emitem UI), ADR-0057 §WS-G #412 (structured decoding → card JSON válido), ADR-0056 (Install≠Ready p/ skills), ADR-0037 (UI = software FB, sem GPU compute).
**Ideias:** #79/#80 ✅, #82 🟡, #279d 🟡, #283 🟡, #448/#452 ✅ (absorvidas), #453 ❌, #465 🟡, novas #468–#470.

---

## 1. Contexto e problema

O desktop atual (`crates/jarbas/src/display/`) é um **compositor immediate-mode por software** com 4 camadas Z fixas (`OrbBackground → HermesOverlay → AppWindows → DockBar`), `DoubleBuffer` BGRA32 sobre UEFI GOP, e **janelas hardcoded por enum `AppId`** com conteúdo desenhado à mão (`render_app_content`). Não há árvore de widgets, layout, nem renderer declarativo. `compositor.rs`, `fb.rs`, `agent.rs`.

A ADR-0047-HMI já definiu o alvo — **`UiDeclaration` (JSON gerado pelo LLM) → `UiRenderer` nativo** — e listou as limitações reais (§2.2: apps fixos, compositor sem componente de UI, temas estáticos). Mas o alvo **nunca foi implementado**: `ui_spec.rs` só parseia `WindowSpec`/`WidgetSpec` e loga; **widgets do JSON não são renderizados**. A resposta "visual" do Hermes hoje é, na prática, **texto num painel** (`HERMES_RESPONSE → AppWindow.data`).

Objetivo: **Jarbas responder ao usuário com saída visual rica** (ex.: "clima de amanhã" → um _card_ com ícone, temperatura, máx/mín e mini-gráfico), com a UI **gerada como dados** (JSON) ou como **skill WASM** pelo Hermes/Trinity/RustCoder — inclusive de forma **repetitiva** (Cron/SleepCycle), sem recompilar o kernel.

## 2. Decisão

**"Generative Card Desktop":** o Jarbas responde com **cards declarativos**, renderizados por um toolkit `embedded-graphics` (`no_std`), e gerados como **dados** por Hermes/Trinity/Cortex ou por **skills WASM** (RustCoder/Codex). Arquitetura em duas camadas.

### 2.1 Camada A — Fundação de render (adotar, não reinventar)

- **`embedded-graphics` (MIT/Apache, no_std, no-alloc)** como fundação 2D. O único seam de integração é implementar **`embedded_graphics_core::DrawTarget`** para o `DoubleBuffer` existente (adapter BGRA32 respeitando `rgb_order`; `draw_iter` + overrides `fill_solid`/`fill_contiguous`). Isso destrava todo o ecossistema (fonts `u8g2-fonts`/`profont`, `embedded-text`, `embedded-layout`, `embedded-plots`, imagens `tinybmp`/`tinyqoi`) **sem tocar** orb/dock/avatar.
- **Toolkit de widgets:** **matrix-gui** (MIT, zero-alloc, **animações inteiras sem FPU** → aderente ao kernel soft-float) como padrão; **embedded-gui** (leftger) p/ widgets ricos de dashboard (**gauge/chart/list/table/toast**); **kolibri-embedded-gui** (MIT/Apache, immediate-mode egui-like) como alternativa que espelha o loop atual. Todos sobre embedded-graphics, todos licença permissiva (compatível com AGPL-3.0 do projeto). **Evitar Slint/LVGL** (não no_std-first / licença comercial / FFI C).
- **Window manager:** manter **stacking** (modelo certo p/ assistente com cards/overlays; tiling é desnecessário). Formalizar numa **árvore de janelas retida** (`WindowId, rect, z, declaration, focus, owner`) e **aposentar o enum `AppId` hardcoded**. As 4 camadas Z permanecem (orb no fundo, cards no meio, dock/cursor no topo).

### 2.2 Camada B — Declarativa/generativa (a inovação)

- **Schema `UiDeclaration`** (evolução do `ui_spec.rs` H1):
  ```
  UiDeclaration {
    window: { id, title, x?, y?, w?, h?, z?, closable, focus? },
    body: [ Widget ]                // lista ordenada
  }
  Widget = Text{s,style} | KeyValue{k,v} | Gauge{label,value,min,max,unit}
         | Chart{kind:line|bar, series:[f32], labels?} | List{items:[String]}
         | Icon{name} | Image{fat_name} | Buttons{[{label,action}]}
         | Divider | Progress{value}
  ```
  JSON mínimo, `no_std`+alloc, parse manual (sem serde pesado) — como o `ui_spec.rs` já faz.
- **Geração por LLM (Hermes/Trinity/Cortex):** o card é a **resposta estruturada** do LLM. Usa **ADR-0057 §WS-G #412 (structured decoding)**: a máscara de tokens (`cortex::decode`) **constrange a saída a um `UiDeclaration` JSON válido** (grammar de card). Sem grammar → texto normal (fallback).
- **Geração por skill/WASM (RustCoder/Codex/Trinity):** uma skill (ADR-0052, sandbox WASM SFI + CapGate) produz o `UiDeclaration` e publica no EventBus (`UI_SPEC`); o `DisplayAgent` renderiza. **Install≠Ready** (ADR-0056) respeitado; nada toca MMIO (observe-only).
- **Ações repetitivas:** `CronAgent`/`SleepCycleAgent` re-disparam a skill (ex.: card de clima toda manhã), sem novo prompt — exatamente o pipeline "Jarbas gera ações repetitivas com o Hermes".

### 2.3 Exemplo fim-a-fim — "clima de amanhã"

```
WakeWord/Enter → Hermes (intent) → skill `weather` (RustCoder/Trinity, WASM)
  → NetAgent HTTP GET API de clima (LAN: e1000 + DNS raw + HTTP já ✅)
  → Hermes formata WeatherCard (UiDeclaration, JSON constrangido por #412)
  → publish UI_SPEC → DisplayAgent → UiRenderer:
       card { Icon("cloud"), KeyValue("Amanhã","22°C"), KeyValue("Máx/Mín","25/16"),
              Chart(line, temps_24h) }
  → CronAgent repete diariamente (mesma skill)
```

## 3. Plano de implementação (incremental, cada passo botável)

| Sprint | Entrega | Validação |
|--------|---------|-----------|
| **S1** | `DrawTarget` adapter sobre `DoubleBuffer` (+ dep `embedded-graphics`). | Desenhar `Text`/`Rectangle` embedded-graphics num canto; boot sem regressão. |
| **S2** | `UiDeclaration` schema + parser + `UiRenderer` (kinds base) sobre o toolkit. | Renderizar um card estático a partir de JSON; self-test de boot. |
| **S3** | Árvore de janelas retida + `UI_SPEC` EventBus → spawn/close/focus genérico; portar Settings/Hermes p/ declarações; remover `AppId` hardcode. | Interação mouse (drag/close/focus) em card declarativo. |
| **S4** | Card-answer do Hermes + grammar #412 p/ card JSON; skill `weather` (WASM, RustCoder/Trinity) + repetição via Cron. | E2E: pergunta → card renderizado; Cron repete. |
| **S5** (opc.) | Widgets ricos (charts/gauges embedded-gui), `theme.rs` wired, TTF via `ttf_engine` p/ texto melhor; absorver `#82` tensor-viz e `#465` HUD como widgets. | Dashboard/HUD como widgets declarativos. |

## 4. Riscos / decisões

- **Soft-float:** preferir **matrix-gui** (animação inteira); usar `libm` com parcimônia.
- **Maturidade:** kolibri é alpha (v0.1) → matrix-gui/embedded-gui mais seguros; opção de **vendorizar** subconjunto (repo já vendoriza deps).
- **Cor/canais:** embedded-graphics usa `Rgb888`; o adapter converte p/ BGRA32 (ordem já em `fb.rs`).
- **Sem GPU:** tudo CPU (embedded-graphics é CPU-first; alinhado à ADR-0037). Compositor front-to-back (`fluor`) fica como watch p/ HW real (no_std ainda "planned").
- **Dependências novas:** `embedded-graphics` + 1 toolkit — justificado (evita reinventar toolkit inteiro); manter mínimo e no_std.

## 5. Marcação das ADRs antigas

- **ADR-0047-HMI:** Status → **Superseded (parcial) → ADR-0058**. H1/H2/H4/H5 absorvidos aqui; H3 segue ❌.
- **ADR-0036:** **inalterada** (persona/interação). ADR-0058 é a camada de render/UI que a serve.
- **IDEA_BANK:** `#448`/`#452` → ✅ absorvidas por ADR-0058; `#279d`/`#283`/`#82`/`#465` → apontam p/ ADR-0058; `#453` (H3) permanece ❌.

## 6. Critérios de aceite

- [x] S1: `DrawTarget` adapter (`FbTarget`) — embedded-graphics desenha no FB; boot self-test PASS; 0 regressão.
- [x] S2: `UiDeclaration` + parser + `UiRenderer` (Text/KeyValue/Gauge/Bars/List/Divider/Button/Panel); self-test PASS.
- [x] S3: `CardWindow` retido + `UI_SPEC` spawn/close + mouse (close X, **mover** por título, **redimensionar** pelo canto inf-dir, botão→`CARD_ACTION`, foco); orb + HUD preservados. (`AppId` coexiste; remoção total = follow-up.)
- [x] S4: cards demo (status/clima/videochamada) + `card_json_schema_hint()` p/ #412. (Skill WASM `weather` + HTTP live + Cron = pipeline documentado; A/V real via HDA/UVC gated.)
- [x] `cargo check --release` 0 erros; boot QEMU sem panic; screenshot dos 3 cards.
- [ ] S5: widgets ricos (embedded-gui charts/gauges), `theme.rs` wired, TTF; A/V real da videochamada.

## 7. Referências

- Código atual: `crates/jarbas/src/display/{compositor,fb,agent,ui_spec,font,ttf_engine,gauges,avatar}.rs`.
- ADR-0047-HMI, ADR-0036, ADR-0052, ADR-0056, ADR-0057 (#412), ADR-0037.
- Crates (licenças permissivas): [embedded-graphics](https://github.com/embedded-graphics/embedded-graphics) (MIT/Apache), [matrix-gui](https://lib.rs/crates/matrix-gui) (MIT), [embedded-gui](https://github.com/leftger/embedded-gui), [kolibri](https://github.com/Yandrik/kolibri) (MIT/Apache), [oxide-gui-core](https://crates.io/crates/oxide-gui-core), [fluor](https://github.com/nickspiker/fluor).
