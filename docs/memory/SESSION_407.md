# SESSION_407 — Revisão e correção da ADR-0110 (Laya-Zero)

**Motivo:** revisão da ADR-0110 a pedido do maintainer. Duas lanes independentes: @explorer (verificação claim-a-código) + @oracle (julgamento arquitetônico/governança). Veredito convergente: **ACCEPTED ilegítimo** — decisão central contradizia evidência medida do próprio repo.

## Achados (blockers)
- **B1 erro de categoria**: ADR aceitava reusar `hw_expert_v6` como classificador de intenção de rede. Real: input `pack_vid_did(vid,did)`→4 tokens vocab-64; output 5 cabeças família-de-device (`cortex.rs:399-496`); **265.620 bytes, h=128/6L/4heads/q_dim=32** — não "128 heads/hidden 3072" (3072 é Falcon3-3B; loss 0.389 é v3/Sprint102, não S255). S248 registrado: transformer ferramenta errada, fp32 60,58% ≈ majoritário, teto 59-63%; gate #492 ≥65%. ADR omitia S248.
- **B2 fail-open**: "classifier ausente → `Intent::Normal`" viola ADR-0106 (`ScorerAbsent`, `decision.rs:253`).
- **B3 contradição**: "Malicious→skip detectores" torna "consenso antes de drop" inalcançável; SecurityAgent **não tem caminho de drop** (observa resumos de texto, `hermes/security.rs:216-253`).
- **B4 governança**: sem IDEA, sem SESSION, sem `docs/evidence/laya-zero/`; INDEX dizia PROPOSED/`por_fazer` vs header auto-declarado ACCEPTED/`fazendo`; "6 sessions reconciliadas" = labels de lanes (exp/lib/fix-N) dentro de outras sessions.

## Achados (maiores)
- `HEAP_FREE_MB` inexistente (real: `heap_observe().headroom_mb`); gate 8GB/200MB sem sentido para artefato 265KB; `embed_model` vs `fall_back_to_scalar` conflui residência com ISA.
- TickvLite = append-log persistente, GC watermark 256KB, **sem TTL** — efêmeros pertencem a ring RAM (S352).
- NoProto header **37B medidos** (36 só em p2p_sim morto); BOOT_AI **já existe** (`boot_report.rs:32`, ADR-0100 T-001..T-004, não 0092).
- Citações erradas: TRINITY=273 (não 293); mesh=242 (não 280); SecurityAgent↔NET_EVENT=310 (não 152); SESSION_233.md não existe (lição em AGENTS.md).

## Correções aplicadas
- `docs/architecture/0110-laya-zero-implementation-adr.md` reescrita: status PROPOSED/`por_fazer` + aviso de revisão; B1→REJEITADO com evidência; triagem reescrita fail-closed/aditiva/sem-drop; TickV→ring RAM; gate RAM removido; §5 tabela corrigida +S248/#492/#496; §9.4 BOOT_AI→"estender"; Fase 1 condicionada a #611.
- `docs/architecture/INDEX.md`: linha 0110 atualizada (decisões revisadas, citações corrigidas).
- `docs/memory/IDEA_BANK.md`: **#611** criada (dataset pacote→intenção não-circular + holdout disjunto + gate ≥65% + evidência; único liberado trivial: `TaskType::LayaIntent=8`).

## Verificação
Docs-only; zero código tocado. `TaskType` slot 8 livre confirmado (`noproto.rs:42-59`); `Decision<T>`/`ScorerAbsent` confirmados (`decision.rs:54-59,149-156`); shapes hw_expert_v6 medidos do header real.

## Complemento (mesma sessão): estudo do white paper-fonte + fatos verificados
User enviou o texto-fonte do paper "Laya-Zero" na íntegra. Duas lanes: @librarian (modelo real + agent-jev) + @explorer (superfície System-1 no repo).

**Laya real confirmado e corrigido**: Convai Innovations (não lab chinês), ModernBERT-large 421M encoder não-autorregressivo, Apache-2.0, primitivas boolean/choice/score+margin+abstain — premissa conceitual do paper VERDADEIRA (model card usa "System 1"). Números do paper FALSOS: 842MB fp16 (não 150-300MB); 39,5ms p50 T4 / **193-464ms CPU** (não <30ms — no metal cooperativo ~4-9 ticks congelariam o scheduler, S328); **zero-shot quase ao acaso** (0,362 vs 0,318 random; ECE 0,466; "fast base to specialise, not a zero-shot decision engine"). → **rodar o Laya real no Ring 0 REJEITADO** (exigiria motor denso novo; repo só tem BitNet/W2A8).

**agent-jev**: o paper fabricou a análise ("provavelmente Python sobre Linux") sem acesso — real: **AgentJev-0.6B** (Qwen3-0.6B decoder sem LM head → classifier permutação-equivariante, 79,25% typed-decisions vs 77,00% Laya especialista, mesmo split). Lições absorvíveis: recusa over-length honesta (nunca truncar); o fail-open deles (serviço cai→deixa passar) é o oposto do nosso padrão `abstain(ScorerAbsent)`.

**O repo já tem o "reflexo"**: `cortex::intent_decide::decide_intent` (15 arms, Q8, θ_conf/θ_margin, abstain→LLM, wired no tick Hermes `agents.rs:1949`) + Trinity `classify_keywords`/ROUTER.BITNET; `Decision<T>` ≡ contrato typed-decision do Laya. Ausente de verdade: classifier **aprendido**, LLM→op-IR (stub `evolve.rs:133`), fast-path sub-tick (tudo ~55ms; "mesh <5ms" do paper incompatível com heartbeat real ~6s), lifecycle de agente WASM efêmero.

**Rota aprovada (ADR §10.4 / #611 refinada)**: **destilação, não execução** — promover `decide_intent` de hand-weighted → trained BitNet pequeno (~1M params, classe hw_expert, loader v6 existente, zero motor novo), treino host forward Rust-exato + validação do artefato (S247/248), labels não-circulares + holdout disjunto, gate ≥65% + ECE calibrado, evidência `docs/evidence/laya-zero/`.

**Edições do complemento**: ADR-0110 ganhou §10 (adendo) + notas em §7/§9.1; IDEA #611 reescrita com a rota de destilação; linha INDEX 0110 atualizada.
