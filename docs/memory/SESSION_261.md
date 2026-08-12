# SESSION_261 — Mesh graph UI: orb vira hub do grafo P2P; chat Hermes + SysInfo removidos (2026-08-12)

**Escopo:** Análise do tweet @antpalkin (Kimi Agent Swarm — "você não recebe 300
respostas, recebe um mapa") como referência de interface para o orb do Jarbas;
implementação do grafo de mesh como visual central + limpeza dos overlays
(Hermes Console + card SysInfo 9001).
**Status:** ✅ Fechada — 2 commits — 0 erros — `cargo check --release`.

---

## 1. Decisão de interface (tweet → orb)

O tweet apresenta o Agent Swarm da Kimi: 300 agentes / até 4.000 passos
visualizados como **grafo persistente** — nós = entidades tocadas, arestas =
contrapartes compartilhadas, hub = ponto único de falha. Tese central:
*"you don't get 300 answers. you get one map"* + *"it doesn't die when you close
the tab. next launch adds to the same graph"*.

Mapeamento para o Jarbas:
- O orb (Soul Mirror, `soul_mirror.rs`) é nível **"um eu"** (valence/arousal/
  dominance → afeto); o grafo é nível **"sistema"** (topologia de peers/entidades).
- **Substituição é o framing errado** — o orb vira o **nó central do grafo**:
  hub afetivo Jarbas (roxo, glow) + satélites = peers do mesh + arestas
  hub→peer coloridas por p99 RTT. O sistema de cor afetiva transfere direto.
- Roubado do tweet: (1) insight estrutural como resposta (hub/cluster > lista
  de 300 linhas); (2) acumulação entre execuções → IDEA #532 (grafo via SGDB).

## 2. 🔴 Bug descoberto: desenho fora do `render()` é apagado

Os cards de status de mesh (agent.rs:717-760, SESSION_242) eram desenhados no
`tick()` do DisplayAgent **ANTES** do `desktop.render(tick, ...)` (agent.rs:884).
O `render()` apaga o back buffer inteiro (compositor.rs:394) e **só** ele chama
`fb.swap()` (compositor.rs:577) → os cards eram desenhados no back buffer e
apagados no mesmo frame. **Nunca apareceram na tela** — o "dashboard integrado"
do s242 era invisível (o log serial funcionava, a UI não).

Regra extraída: `JarbasDesktop::render()` é o único pintor de frame; qualquer
draw em `desktop.fb` fora dele é apagado. Dados → static compartilhado
(`MESH_GRAPH`), render() desenha.

## 3. Implementação (4 arquivos)

- **`crates/jarbas/src/display/fb.rs`** — `DoubleBuffer::draw_line` (Bresenham
  inteiro, bounds via `set_pixel`).
- **`crates/jarbas/src/display/agent.rs`** — `MeshPeerNode` + static
  `MESH_GRAPH: IrqSafeLock<Vec<MeshPeerNode>>`; o drain de `MESH_HEALTH` agora
  faz snapshot no static (substitui o draw de cards invisível).
- **`crates/jarbas/src/display/compositor.rs`**:
  - Painel "Hermes Console" (CAMADA 2, direita 35%) **deletado**; workspace
    agora full-width (`left_w = w - gap*2`).
  - `ensure_hermes_overlay()` → no-op (janela HermesChat nunca mais spawna;
    OpenChat/ShowLauncher viram no-op — janela nem entra no layout da workspace).
  - `draw_mesh_graph(tick)` chamado dentro do `render()`, após o orb, antes do
    swap: hub = Jarbas (roxo 106,13,173 + glow + core branco), satélites = peers
    (≤12, órbita determinística `2π·i/n`, pulso senoidal), aresta hub→peer por
    p99 RTT (lerp verde→amarelo→vermelho em 0..1500ms; offline cinza).
- **`crates/neural-kernel/src/agents/sysinfo_agent.rs`** — card SysInfo 9001
  (CPU/RAM/agentes/uptime/net/storage) **removido**; agente virou unit struct;
  **BOOT.LOG flush retry mantido** (HW real: USB-MSC demora a enumerar).

Sem mesh ativo o grafo degrada para o hub sozinho (sem peers) — gracioso.

## 4. Verificação

- `cargo check --release`: **0 erros** (49.7s no 1º run; warnings pré-existentes
  do bin = política Known Warnings).
- Working tree tinha mudanças não relacionadas pré-existentes
  (`k_nano/limine.rs` + `main.rs` — reserva de stack SESSION_254/258 e
  `legacy/*.bitnet` untracked) — **intocadas**, ficam fora dos commits desta
  sessão.

## 5. Follow-ups

- IDEA **#532**: grafo persistente via SGDB (acumular entre boots — "a base fica
  mais esperta a cada run").
- IDEA **#533**: click-to-inspect no grafo (mouse PS/2 já integrado → card do
  peer com RTT/TX/ACK/fail via UI_SPEC).
- Force-directed sim quando >30 nós (hoje layout determinístico por índice; com
  r² sem sqrt, barato até em soft-float/TCG).
- Status bar superior (CAMADA 1) **permaneceu** — perguntado ao usuário se remove.
