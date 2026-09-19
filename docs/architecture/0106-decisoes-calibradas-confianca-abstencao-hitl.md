# ADR-0106: Decisões calibradas — confiança, abstenção e HITL (e fim da decisão por substring)

**Data:** 2026-09-19
**Status:** Proposed
**Lifecycle (INDEX):** `por_fazer`
**IDEA:** **#588**
**Sprint / enquadramento:** **s362**, continuação da **ADR-0088** (Premissa Máxima: toda decisão tratada com inferência, adaptação e versionamento). Empresta a *forma* da **ADR-0104** (medir → sancionar faixa → aplicar dentro da faixa → bin dirige + HITL/pin; `trusted=false → default seguro`). Reusa a disciplina da **ADR-0083** (router treinado vs keyword; LCG seed=42 não roteia).
**Evidência:** auditoria s362 (referência externa: classe "System One"/Jev da TypeSafe, 09/2026 — *forma*, não dependência) confrontada com o código medido:
`think()` = 14 intents em cadeia `if/else if lower.contains(...)`, first-match-wins, **sem confiança e sem `Unknown`**, `else → Chat` (`crates/cortex/src/cortex.rs:4593`); `ApprovalGate::classify` **sem nenhum caller** (`crates/hermes/src/approval.rs:93`) e **três** vocabulários de risco concorrentes (`ApprovalLevel`, `permission_gate::RiskLevel`, `PackageHub::classify`); `trinity::route` **já faz softmax** com gate `best_score > 0.05` e grava a distribuição no `RouteTrace` (`crates/cortex/src/trinity.rs:305-322`); `r3::update_with_replay` + `persist_trained_router` **já** atualizam pesos ternários por reward (`crates/cortex/src/r3.rs`); `tools/train_router.py` declara *"Label semantics = check ORDER of the production keyword classifier that this training distills"*; ~**497** ramos `if/else-if` decidindo por substring (hermes 250, k_nano 107, cortex 51, jarbas 36, k_ai 33, k_hal 20 — **limite superior**, proxy de ramos que testam substring, incluindo parsing).

**Não substitui:** ADR-0088 (premissa; esta a aplica), ADR-0083 (MoE/LCG), ADR-0104 (forma), ADR-0092 (observabilidade), ADR-0055/0100/0101, ADR-0102 (Ring3), ADR-0045 (áudio/voz — fora de escopo).
**Corrige:** a decisão por `contains()` tratada como inferência, e a destilação da heurística apresentada como IA — ambos bypass da premissa máxima (todo desvio exige análise e correção registrada).

---

## 0. Decisão (ler primeiro)

1. **Contrato único de decisão — `Decision<T>`.** Conjunto de opções **fechado** (o enum é o conjunto), **distribuição** completa, `confidence` Q8, `margin`, `source` (`Neural|Heuristic|Cache|Fallback`) e **abstenção explícita** (`Unknown` + `AbstainReason`). Onde a classe "System One" do mercado fala `noul`/`choice`/`score`, o AIOS usa as mesmas três formas sobre enums que **já existem**.
2. **Abster é resultado de primeira classe, não `else`.** Herda a lição do STT (SESSION_346): na dúvida **não se inventa** — abstenção registrada, nunca o braço final por omissão.
3. **Duas classes de sítio — e é o que impede a inundação de HITL:**
   - **conversacional** (ex.: `think() → Chat`): abster **roteia ao LLM** (System 2 escreve), **não** escala;
   - **ação** (ex.: aprovar skill destrutiva): abster **escala para HITL** (`ApprovalGate` + `hitl_ui`).
4. **θ vem da reliableza medida, não de constante.** R2 `cortex` **sanciona a faixa** de θ derivada do reliability diagram; R3 `hermes` **aplica dentro da faixa**; o **bin** é dono do pin/HITL. `trusted=false` (sem medição) → θ conservador + abster.
5. **`Deny` não é negociável por confiança.** Confiança só **sobe** a barra, nunca libera o que a política nega (doutrina "factos medidos não são anuláveis", ADR-0104 §0.6).
6. **A substituição da lógica por inferência é incremental, medida e comprovada.** O braço de substring vira decisão tipada **com paridade comprovada por sítio**; só sobe para `source=Neural` quando existe head treinado — **nunca se declara IA onde ainda é heurística**.
7. **Decisão ≠ parsing.** Ramo escolhido por **sintaxe** é contrato e **não migra** (§7).
8. **Contrato versionado:** `DECISION_CONTRACT_VERSION` no artefato + verificação mecânica Rust↔trainer (`tools/decision_contract_check.py`), porque o próprio `train_router.py` já avisa que a **ordem dos experts deve casar com `init_trinity`**.

---

## 1. Contexto

O sistema declara "IA desde o boot" e "toda decisão tratada com inferência". Na medição de s362, a **forma** existe (contrato de enum + FSM de saída), mas o **juízo** continua sendo tabela de substring: o roteador de intenção é uma cadeia `if/else if` sem confiança, sem `Unknown` e sem escore — first-match-wins, com sombreamento de braço (a palavra "volume" em "por que o **volume** do disco encheu?" cai em `Intent::AudioVolume`). O `else → Chat` é obrigatório, o que reproduz a mesma classe de falha que o `blank-suppress` do STT causava: a decisão por omissão é indistinguível de uma decisão confiante.

Em paralelo, o **oposto** também está no tree: `trinity::route` já faz softmax, já descarta score baixo (`> 0.05`) e já grava a distribuição no `RouteTrace`; e `r3::update_with_replay` já atualiza pesos ternários por reward, com persistência e seam federado. Ou seja: **o System-1 on-device já existe** — ele apenas cobre 1 decisão (escolha entre 7 experts) de ~500 sítios, e é treinado para **reproduzir a heurística** (`train_router.py`), o que faz o teto do "cérebro" ser o acerto das regras.

Esta ADR fixa o contrato que faltava, define como a confiança vira política (θ medido), como o label deixa de ser a própria regra, e **encaminha a migração dos sítios de decisão por substring** com critério e prova de paridade.

---

## 2. Split por anel

| Anel | Responsabilidade | Onde |
|---|---|---|
| **R2 `cortex`** | O **contrato**, o scorer, a distribuição Q8, a **sanção da faixa de θ** e o acumulador de reliableza. **Fonte única** do contrato | `crates/cortex/src/decision.rs` (novo), `trinity.rs`, `difficulty_gate.rs` |
| **R3 `hermes`** | **Política** dentro da faixa: aplica θ, escolhe abster→LLM vs abster→HITL, coleta labels (HITL/resultado) | `crates/hermes/src/{agents,approval,permission_gate}.rs` |
| **R2 `k_ai`** | Store e agregação de labels/reliability (SGDB `ai/decisions/*`) | `crates/k_ai/src/sgdb.rs` |
| **Bin** | Wiring, persistência em disco, pin/HITL, `/decisions` | `crates/neural-kernel/src/{main,shell}.rs` |
| **`k_hal`** | **Não participa** (não há DeviceCap/HalOffer neste eixo) | — |

Não colocar o scorer no caminho do IRQ. Não adicionar campo a `HardwareInfo` (congelado por ADR-0100 T-005).

---

## 3. Contrato `Decision<T>`

```rust
pub const DECISION_CONTRACT_VERSION: u16 = 1;

/// Q8 — sem f32 novo no caminho da decisão (soft-float; SESSION_336/346).
#[derive(Clone, Copy, PartialEq, Eq)] pub struct Confidence(u8);      // 0..=255

#[derive(Clone, Copy)] pub enum DecisionSource { Neural, Heuristic, Cache, Fallback }
#[derive(Clone, Copy)] pub enum AbstainReason { LowConfidence, Tie, NoOptions, ScorerAbsent }

/// T = enum Copy (conjunto FECHADO). Sem string gerada, sem tipo inválido.
#[derive(Clone, Copy)] pub struct Decision<T: Copy> {
    pub choice: Option<T>,        // None = abstenção
    pub confidence: Confidence,   // max(p)
    pub margin: Confidence,       // p1 - p2: separa "1.0 por consenso" de "1.0 por empate"
    pub source: DecisionSource,
    pub abstain: Option<AbstainReason>,
    pub dist: Q8Dist<N>,          // heapless, N fixo por sítio
}
```

Regras não negociáveis:

- **`dist` guarda a distribuição inteira**, não só o argmax — sem isso o item de medição é impossível.
- **`margin` separa confiança de empate** (um escore médio de 1.0 pode ser "todos no nível 1" ou "metade em 0 e metade em 2").
- **`n/a ≠ 0`** (SESSION_331): ausência de medição é `ScorerAbsent`, nunca confiança zero.
- **Q8 na fronteira**; `f32` só onde já existe (o softmax de `trinity::route`), convertido uma vez. Sem transcendentais novos (lição FFT/Goertzel, SESSION_331).
- O **0% de erro de tipo é estrutural** e vem do enum Rust — não é uma métrica a medir, é uma propriedade a não perder.

---

## 4. Threshold por custo do erro

```
SITE_POLICY: site -> (destructive: bool, theta_auto, theta_review, theta_deny)
theta = f(reliableza medida)            // R2 sanciona a faixa
apply(decision, site):
    if policy(site).deny            -> Deny        // confiança NÃO libera
    if d.confidence >= theta_auto   -> Auto        // executa
    if d.confidence >= theta_review -> Confirm     // pede confirmação
    else   ação: Escalate (HITL)  |  conversacional: roteia ao LLM
trusted=false -> theta conservador + abster (default seguro, sem chute)
```

- **Unificação dos três vocabulários de risco:** são **opções** de um mesmo sítio. O `ApprovalGate::classify` (hoje sem caller) é **religado ou removido com justificativa** — não se mantém política órfã.
- **`Escalate ≠ Auto`** (ADR-0104 §0.4): abster em sítio de ação **propõe**, nunca executa.
- θ inicial conservador e explícito; refinado pelo diagrama de reliableza (D0/D4) — não são números mágicos.

---

## 5. Decomposição em perguntas independentes

O ganho não vem de uma cadeia grande, e sim de **perguntas independentes com probabilidade fina** sobre o mesmo estado. Mapa direto no que já existe:

| Forma | Sítio atual | Vira |
|---|---|---|
| `Choice` (14 opções) | `think()` | `decide::<Intent>` com `Unknown`, **score por braço** em vez de first-match-wins (corrige o sombreamento) |
| `Score` (3 níveis) | `ComputeTier` | escala ordenada com **critérios descritos por situação** ("prompt curto/greeting"), nunca por grau ("moderadamente longo") |
| `Noul` | destrutividade / aprovação | "isto é destrutivo?" como **probabilidade**, não `contains("shutdown"\|"format")` |

**Escape hatch obrigatório** em todo `Choice` (`Unknown`/`other`): sem ele o modelo é forçado a escolher algo. Critérios por **situação**, não por grau — rótulo vago espalha a probabilidade e colapsa a confiança.

---

## 6. Labels: parar de destilar a heurística

1. **Fontes honestas:** (a) `ApprovalGate::resolve(id, approve)` — a decisão escalada vira label; (b) **resultado verificado** da skill (`SkillMarket` success_rate / execução); (c) correção explícita do operador (`/decisions correct <site> <opt>`).
2. **Viés declarado:** só escalação gera label ⇒ **ausência de escalação não é confirmação**. Mitigação: **auditoria amostral** de N% das decisões `Auto` re-confirmadas pelo operador, e relatório de reliableza **por proveniência** de label.
3. **Treino:** `r3::update_with_replay` (reward = correto?) → `persist_trained_router` (o seam federado já existe). `tools/train_router.py` deixa de rotular pela tabela de keywords e consome `data/decision_labels.jsonl` exportado do SGDB, com proveniência no header.

---

## 7. Escopo da migração (onda M)

**Critério de inclusão — o teste é o *motivo* do braço, não a forma do `if`:**

- **IN (migra):** braço escolhido pelo **significado** de texto livre — intenção, risco, tópico, emoção, pedido de skill, destrutividade.
- **OUT (permanece código):** braço escolhido por **sintaxe** — extensão de arquivo, prefixo de protocolo, nome de campo/chave, delimitador, tabela de comando (`parse_command`). Isso é **contrato, não juízo** (mesma lição SESSION_252: hardcoded de *layout* é contrato; IP de servidor era dado). Migrar isso degradaria código correto em modelo probabilístico.

**Sequência por sítio:** (1) inventariar → (2) **gravar fixture de paridade** (entrada → opção esperada, comportamento atual) → (3) trocar o braço pelo contrato com `source=Heuristic` (escorado) → (4) **paridade verde** → (5) promover para `source=Neural` só com head treinado.

**Paridade é obrigatória porque migração em massa esconde drift:** sem fixture por sítio, ~500 trocas de braço viram regressão invisível — a mesma classe de bug do `apply_one_layer` duplicado (SESSION_353) e do port FAT sem os bug-fixes upstream (SESSION_252). O precedente de forma no repositório é parity contra artefato real (s255) e FFT↔DFT (s346).

**Teto honesto:** os 497 são **limite superior** (proxy de ramos que testam substring, incluindo parsing). A contagem canônica de "decisão" sai da triagem **M0**.

---

## 8. Governança / observabilidade

- **Log visível** (`ok|warn`, nunca `trace` — SESSION_289/299): `site= source= conf= margin= choice= abstain= theta=`.
- **Persistência** no padrão ADR-0104: `k_nano::storage::put_blob("ai/decisions/reliability", json)` + `k_ai::sgdb::put_kv("ai/decisions/labels/<site>", …)`, reidratado no boot (detectar→medir→decidir→versionar).
- **Postura no HUD:** `source` (neural/heuristic) + contadores de abstenção — "instrumento invisível = instrumento inexistente" (SESSION_330).
- **Rollback:** θ anterior guardado; degradação de reliableza na janela → reverter com `warn`, uma vez.
- **HITL:** pin por `/decisions theta <site> <v>` e `CONFIG.TXT`, sempre clampado à faixa sancionada.

---

## 9. Fallback (degradação graciosa)

| Condição | Comportamento |
|---|---|
| Scorer ausente (`router_trained=false`) | `source=Heuristic` honesto; **sem LCG** (ADR-0083/SESSION_273) |
| OOM no matmul/emb | `warn` + caminho keyword atual (`trinity.rs`) |
| Sem labels / sem medição | `trusted=false` → θ conservador, abster; **nada declarado calibrado** |
| Conversacional incerto | roteia ao LLM (não escala) — evita HITL spam |
| Ação incerta | `Confirm`/`Escalate` — **nunca executa** |

---

## 10. Verificação

**Host (aceite por onda):** `cargo clean -p neural-kernel` + `cargo check --release` (0 erros; cache incremental mascara erro — SESSION_346) · `cargo test -p cortex --lib` / `-p hermes --lib` / `-p k_ai --lib` · `cargo test --workspace --exclude neural-kernel --exclude boot --no-fail-fast` · testes do contrato: abstenção; `dist` Q8 soma ≈ 255; `Deny` imune a confiança; "conversacional não escala, ação escala"; nenhuma opção fora do conjunto · **paridade por sítio** na migração · `python tools/decision_contract_check.py` (ordem/opções Rust↔trainer).

**Runtime — declaradamente AWAITING:** PHASE 6/7 **não roda** no boot 8c (stall pré-existente da frente pins/FAT/tickv). QEMU valida **não-regressão** (comparar com o log anterior até a mesma linha, método SESSION_346) e a presença da linha de postura. Contadores reais, abstenção em campo e θ calibrado **não** são aceites aqui — prometê-los repetiria o erro s346 ("aceite que não pode ser medido").

**Metal:** único lugar onde o loop fecha (θ saindo do diagrama) — horizonte longo, registrado como tal, não vendido como pronto.

---

## 11. YAGNI (não construir agora)

- **Não** adotar TypeSafe/Jev como dependência: nuvem, texto-only, 32k de contexto, sem ferramentas e sem loop — contraria "IA desde o boot" e a operação offline. Adota-se a **forma**, não o vendor.
- **Não** colocar LLM no caminho da decisão (System 2 escreve, System 1 decide).
- **Não** novo campo em `HardwareInfo` (ADR-0100 T-005); telemetria em SGDB.
- **Não** PID/controlador contínuo sobre θ (adaptação quantizada, como a ADR-0104).
- **Não** tocar áudio/STT/wake-word (frentes s346/s352).
- **Não** migrar parsing (§7).
- **Não** migrar os ~497 de uma vez: por sítio, com paridade.

---

## 12. Ondas

**D0** contrato `Decision<T>` + telemetria/reliableza + persistência + postura no HUD *(pré-requisito de tudo)* → **D1** `decide()` canônico + `Intent::Unknown` + `think()` como wrapper (1 caller: `agents.rs:1706`) → **D2** política de θ + unificação dos riscos (religar/remover o `classify` morto) → **D3** decomposição (`Score`/`Noul`, escape hatch) → **D4** labels honestos + treino + `decision_contract_check.py` *(θ calibrado = `por_fazer`, horizonte longo)*.

**M0** triagem canônica (site × classe IN/OUT × anel; corrige a contagem de 497) → **M1** roteamento conversacional (cortex `think`; hermes tópico/keyword/skill-creation) → **M2** risco/ação (`approval`, `permission_gate`, `marketplace`, `plugin_hub`, `package_hub`) → **M3** significado em observabilidade/HUD/labels (jarbas tema/gauge, subs de `slog`, net diag) → **M4** cauda + **lista dos excluídos com justificativa**.

**Ordem recomendada:** D0 → M0 → (D1 ∥ D2) → M1 → D3 → M2 → M3 → M4 → D4.

---

## 13. Riscos

1. **Inundação de HITL** se abster escalar em sítio conversacional → mitigado pela classe de sítio (§0.3).
2. **Reliableza enviesada** (só escalação gera label) → auditoria amostral + proveniência no relatório; declarado, não escondido.
3. **Drift silencioso na migração** → fixture de paridade obrigatória por sítio (§7).
4. **Árvore compartilhada:** `crates/cortex/src/cortex.rs`, `crates/hermes/src/agents.rs`, `crates/k_hal/gpu/*` e `TECNOLOGIAS.md` estão modificados por outra pista (s361) — **patcher, nunca sobrescrever**; commits só com arquivos próprios.
5. **Colisão de numeração** de ADR/IDEA (lição SESSION_264/346) → confirmar 0106 e o IDEA livre no momento da escrita.

---

## 14. Baseline Medido (s362 — antes e depois do alinhamento)

**Artefato:** `ROUTER.BITNET` v6 (magic `0xBE11BE11`, vocab=99, hidden=64, n_experts=7, 25818 bytes). Treinado com `tools/train_router.py` (encode `(b-32)+3`, truncate 32, ASCII 32..=126).

### 14.1 Encode desalinhado (pré-s362)

O kernel usava `b+2` para todo byte, truncate 64, clamp na const `VOCAB=256`. Medido com `tools/measure_router_calibration.py`:

| Métrica | Valor |
|---|---|
| Acurácia (CURATED inteiro, n=111) | **17.1%** |
| ECE (Expected Calibration Error) | **39.7pp** |
| Tokens fora da tabela (idx ≥ 99, contribuição 0) | **75.3%** |
| MoE router (R3) = decisão neural | **0 em 12 boots** |
| classify matmul fail | **468 em 6 boots mesh** |

### 14.2 Encode alinhado (pós-s362)

`encode()` espelha o trainer; `router_vocab` derivado do header (99); truncate 32. Teste host: `trinity::tests::router_encode_and_decision_match_python_reference` (fixture de 12 casos com artefato real).

| Métrica | Valor |
|---|---|
| Acurácia (CURATED inteiro) | **82.9%** |
| ECE | **5.4pp** |
| Holdout (seed 7, n=31) | **93.5% / ECE 14.1pp** |
| Tokens fora da tabela | **0%** |
| Teste host | **3/3 pass (paridade token-a-token + decisão)** |

### 14.3 Corrupção SSE2 sret (impede runtime)

Mesmo com encode alinhado, o roteador **não decide em bare-metal**: `sse2_ternary_matmul_add_sub_skip` (kernel `#[target_feature(enable="sse2")]`) retorna um `Tensor` com `shape.0 = usize::MAX` (mascarado 65535) e `data.len()=7` — dentro do kernel a shape é `(1,7)` correta; o corruption acontece na **fronteira de retorno** (ABI sret no alvo soft-float). Medido em boot WHPX (`kernel-irqchip=off`, `smp=2`, `cpu=host`) com instrumentação `MatmulDiag` + registro por chamada.

| Campo | Valor |
|---|---|
| `kernel.shape` (dentro do fn) | `(1, 7)`, len=7 |
| `invalid.shape` (chamador) | `(65535, 7)`, len=7 |
| `sse2_ok` + `sse_invalid` | **1 + 1 (mesma chamada)** |
| Caminho: dispatcher → avx512(none) → bare-metal SSE2 | ✓ |
| Classe de defeito | SESSION_336: `#[target_feature]` + XMM intrinsics no soft-float |

### 14.4 Sondas OOD

| Sonda | Resultado (pré-s362 encode) |
|---|---|
| "por que o volume do disco encheu" | ❌ disk_diag (palavra "volume" sombreia hw_control) |
| "aumenta o volume do alto-falante" | ✅ hw_control |
| "me explica como funciona o roteador" | ✅ generator |
| "tem algum ataque no log de rede" | ❌ security vs disk_diag |
| "o ssd esta com problema" | ❌ disk_diag vs disk_diag (correto mas confiança baixa) |
| "fala mais alto por favor" | ❌ hw_control (esperado, mas confiança baixa) |

### 14.5 O que falta para fechar

1. **Corrupção SSE2 sret** → corrigir retorno do kernel (out-param ou rebuild na wrapper).
2. **Encode alinhado** → ✅ feito (s362, fixture + teste host).
3. **Tensor::is_valid** → ✅ feito (s362+, `(0,0)` = invalid).
4. **θ da reliableza** → `por_fazer` (D0 da ADR-0106).
5. **Labels honestos** → `por_fazer` (D4).
