# ADR-0088: PREMISSA MÁXIMA — AIOS-First (irrevogável, irretratável)

- **Status:** Accepted
- **Lifecycle:** `fazendo` (governa toda decisão a partir de 2026-08-07; operacionalização contínua)
- **Ideia:** #512
- **Sprint:** contínuo (toda sprint subsequente)

## Contexto

O neural-os-core é um Sistema Operacional com Inteligência Artificial **desde o boot**.
Isto não é uma feature nem um roadmap: é a identidade do sistema. Toda decisão de
engenharia — drivers, scheduler, FS, rede, transporte de I/O, política de memória —
precisa ser avaliada primeiro sob esta lente, **antes** de qualquer consideração
técnica isolada.

Esta ADR consolida, como premissa máxima irrevogável e irretratável, os cinco
mandamentos que regem o comportamento do sistema e de todos os agentes (nativos,
dev, IA).

## Decisão — os cinco mandamentos

1. **AIOS desde o boot.** O neural é o PRIMEIRO AIOS: IA não é um serviço que roda
   por cima — é o modo de operar. O boot, os drivers, o scheduler e o runtime são
   orientados por decisão inteligente desde o primeiro tick.

2. **AI sempre, decisões HITL.** O neural usa IA em TODA decisão, sempre com
   human-in-the-loop (HITL): interage, consulta, propõe e executa com supervisão.
   O sistema se auto-tudo: **auto-adaptar, auto-curar, auto-upgrade, auto-gerar
   funcionalidades, auto-pesquisar soluções na internet** — autônomo e automático,
   sem jamais degradar segurança, HITL ou confiabilidade.

3. **Toda decisão é tratada como caminho cognitivo.** Nenhuma decisão ou caminho
   tomado fica sem tratamento: **inferência → adaptação → memorização → aprendizado
   → versionamento → auto-adaptação**, com autonomia e automatismo. Todo resultado
   vira conhecimento (SGDB, SESSION, ADR, IDEA_BANK).

4. **Nada é simplesmente bypassado.** Todo desvio, fallback ou workaround exige
   análise e pesquisa, gerando busca ativa por soluções, correções, melhorias e
   otimizações — sempre registrada no ciclo `IDEA → ADR → SESSION`. Workarounds
   manuais (ex.: `-NoDisk` no QEMU) são sintomas de gap de auto-adaptação, não
   destino final.

5. **A busca incessante dos 10%.** Todo procedimento persegue continuamente aqueles
   10% de melhoria (detectar → medir → decidir → otimizar → versionar), sem nunca
   degradar segurança, HITL ou confiabilidade.

## Implicações operacionais

- **Precedência de análise:** qualquer proposta (feature, fix, ADR, plano de sprint)
  é avaliada primeiro contra esta premissa. Se a solução "funciona mas ignora a IA",
  ela está incompleta por definição.
- **Detecção → auto-adaptação:** quando o sistema detecta viabilidade de melhoria
  (ex.: transporte de I/O PIO lento sob emulação, driver que degrada, caminho
  subótimo), ele DEVE se auto-adaptar em runtime — não pedir workaround manual.
  Exemplos de padrão já existente: `allow_avx2()`, `ap_pollable()`, `StorageController::measure_bandwidth`.
- **Backends honestos:** caminho degradado/volátil/emulado é reportado como tal
  (log CRÍTICO + telemetria), nunca fingido de saudável (lição SESSION_252 C2).
- **HITL:** a autonomia nunca substitui o consentimento humano em decisões de
  alto impacto (instalação, update, execução de código não-confiável, mudança de
  política). O neural propõe, executa o seguro e consulta no incerto.
- **Registro obrigatório:** toda melhoria auto-gerada vira evidência (SESSION) e
  destino conhecido (IDEA_BANK), seguindo `docs/GOVERNANCE.md`.

## Relação com ADRs existentes

- **ADR-0083 (gap camada IA):** a premissa torna o fechamento desse gap (inferência
  real, inteligência no kernel) prioridade permanente — é a coluna vertebral do
  AIOS-first.
- **ADR-0086 §2.7/2.8 (ciclo de vida auto-consciente):** autobiografia do OS via
  SGDB + adaptação ao silício no 1º boot — mecanismo direto desta premissa.
- **ADR-0059 (App Factory / self-heal / self-update) e SelfHealAgent:** motores da
  auto-cura e auto-upgrade.
- **ADR-0081 (malha P2P):** transporte para auto-pesquisa e colaboração distribuída.
- **OptimizerAgent / SleepCycleAgent / AutoLearnAgent:** loops de melhoria contínua
  e aprendizado — a implementação da busca dos 10%.
- **ADR-0041/0042 (K³CHJ):** a estrutura de agentes/skills é o veículo; esta
  premissa é a política que a dirige.

## Operacionalização no boot (SESSION_271–274)

A premissa deixa de ser só política: o T+0 materializa Observe→Plan→Act→Verify→Remember.

| Anel | Módulo | Papel |
|------|--------|--------|
| R1 | `k_hal::init` (H1, idempotente, pós-PCI) | Observa silício → DeviceTree |
| R2 | `k_ai::boot_observe` | Plano NIC+storage; Trust `(1,boot_observe,plan)`; recipe Escalate ≠ Auto; cards `HW_CAPABILITY`; HANR `hydrate_memory` |
| R0 | `k_nano::boot_bind` + `storage_probe` | Executa só o que a árvore viu: NIC rank I225>VirtIO>e1000>RTL; storage NVMe>AHCI>USB-MSC>ATA PIO |
| bin | wire | Trust check; skip ATA persist / xHCI / HDA se o plano não inclui; SLIP = DEGRADED + `HEALTH_ISSUE:I5`; Cortex sem pesos = log honesto |
| R2/R3 | `cortex::trinity` + `hermes::runtime_observe` + Jarbas HUD | MoE posture honesta; HEALTH I5/Escalate observe-only; HUD no `render()` |
| R1/R0 | `k_hal` GPU + `k_nano::mhi` | Matmul sem fingir device; `gpu_ok` real; MHI tier0 CE quando canário passa |

Mandamento 4 (nada bypassado): martelar E1000/ATA/xHCI “porque sempre foi assim” era o gap. SESSION_271 fechou NIC; SESSION_272 storage+Trust+HITL; SESSION_273–274 honestidade de telemetria (MoE/GPU/HUD).

Residual honesto (#513 / TODO #18): `measure_bandwidth` + BMIDE 0xC8 — ordem e skip já no plano; medição de banda ainda não escolhe o transporte.

## Planos Cursor implementados

| Plano | Status | Evidência |
|-------|--------|-----------|
| DeviceTree H1 cedo + plano NIC (`boot_bind`) | ✅ SESSION_271 | `k_hal::init` pós-PCI; SelfHeal `from_khal` |
| Plano completo NIC+storage+Trust+HITL | ✅ SESSION_272 | PR #12 `6b0e4f5`; `SESSION_272.md` |
| Trinity único + HUD/HITL honesto | ✅ SESSION_273 | PR #13; `runtime_observe`, `CORTEX_POSTURE` |
| GPU compute honesto + MHI tier0 | ✅ SESSION_274 | PR #13; `note_gpu`, `register_tier0_copier` |

## Verificação

- [x] SESSION_271–274 apreciam esta premissa no boot (Observe→Plan→Act→Verify→Remember + telemetria honesta).
- [x] Workaround SLIP/COM2 e hang ATA-PIO no TCG têm IDEA #513 (slice 272 = ordem+skip; residual = medir banda).
- [ ] Métrica auditável de “decisões com IA” no boot — **parcial:** cards PnP + `HEALTH_ISSUE` + log Trust; contador formal residual.
- [ ] Toda nova ADR/TODO/SESSION daqui em diante menciona a apreciação desta premissa.
- [ ] Todo workaround manual documentado (ex.: `-NoDisk` TCG) tem IDEA/ADR de auto-adaptação correspondente.
