# SESSION_481 — resolve/informa do hub: severidade honesta, presença real (anti-alias), ausência→fallback + tabela de nichos no README

**Data:** 2026-10-17 · **Estado:** v1.9.99-s481 TEST · **Branch:** `main`
**Contexto:** s480 fechou o crash do bge-m3. Esta sessão entrega o "informa" do hub: quando
um nicho/gaveta não está carregado, o sistema **roteia para um genérico e informa** — em vez de
entregar slot vazio ou mentir. Também corrige duas leituras minhas erradas e escreve a tabela
slot→modelo→tarefa no README (não existia).

---

## 1. Duas correções minhas (mesma causa)

Meu grep anterior cobria `cortex/`, `hermes/`, `jarbas/`, `k_ai/` e **excluiu `neural-kernel/src/`**:

1. **`hub_status()` JÁ TEM caller** — `main.rs:5043` (`slog_bin!("MODEL","ok","{}", hub_status())`).
   Eu afirmei que "não tem nenhum caller" e isso estava **errado**; o gap nunca existiu.
2. **`mark_pro_alias` tem caller VIVO** — `main.rs:5041` marca Pro quando `CURRENT_MODEL_EMBED_DIM >= 2048`
   (o 3B tem 3072). Então o alias **está no ar**: com Active 3B, `slot_loaded(GeneratorPro)` retorna
   `true` sem existir blob Pro. O oracle também errou aqui (viu só o caller na função morta).

## 2. O fix (crates/cortex/src/model_hub.rs, +39/−14)

- **`slot_real(slot)` (novo)** — presença REAL, anti-alias. Para os 5 slots do HUB só conta blob em
  `HUB` (`hub_has_blob`); Active/RustCoder/HwExpert vivem nos Mutex legados, então `slot_loaded` é a
  verdade para eles. Elimina a mentira do alias na decisão.
- **`maybe_fit` agora considera AUSÊNCIA** — antes só caía em fallback por `too_tight` (RAM), e um
  nicho ausente com RAM folgada era devolvido vazio. Agora ausência e RAM caem no mesmo
  `pick_fit_fallback`, sem duplicar a decisão.
- **Severidade honesta** — o log de fallback saiu de `"ok"` para `"warn"` e foi reenquadrado como
  `niche={} -> fallback={} reason={}`. Fallback é degradação, não sucesso (ADR-0113; a lição do repo
  "sucesso com sev fail = dmesg mentiroso"). Motivos: `absent` | `too_tight`.

## 3. Prova de runtime (falcon3-3b, nó único × 7G, 240 s)

Não regrediu: `model 1012764KB` (tamanho **correto** — o `590680KB` fantasma não aparece),
`LLM LOADED llama8b.bin size=1012764KB`, `LOADED - model active in ModelHub`, `T+3658`
(**maior** que os 1106 do s479). `tick=1` é o comportamento conhecido do 3B (a inferência segura o
tick budget), **não** regressão. Log: `logs/boot_mesh_a_FALCON3.txt` (run `20261017_114605`).

## 4. Tabela slot→modelo→tarefa no README (não existia)

O README **não tinha** a tabela de nichos/gavetas do hub (verificado: só havia `FALCON3.V6` e a lista
do Trinity). A única fonte era o código, e ela era inconsistente (colisões de nome, alias). Escrevi a
tabela no README para tornar o binding auditável por humanos.

| Slot | Uso (tarefa) | Nomes canônicos 8.3 | Driver |
|---|---|---|---|
| Active | LLM principal / dia-a-dia | `FALCON3.V7` (fallback `FALCON3.V6`/`FALCON3B.BIN`) | `load_model_v6` → `CURRENT_MODEL` |
| GeneratorPro | Tarefa complexa / 7B-10B | `PRO.V7` (fallback `PRO.v6`/`FALCON7B.v6`) | `HUB` + `select_generator_slot` |
| Agent | Agente / orquestração | `AGENT.V7` (fallback `AGENT.BIN`/`QWEN3B.BIN`) | `HUB` |
| RustCoder | Geração de código | `RUSTCDR.V7` (fallback `RUSTCDR3.BIN`) | Mutex legado |
| HwExpert | Identificação de HW | `HWEXPRT.V7` (fallback `HWEXPRT4.BIN`) | Mutex legado |
| Vision | Encoder de visão | `VISION.V7` (fallback `VISION.v6`/`SIGLIP.BIN`) | `HUB` (`k_ai::vision::vit`) |
| Reranker | Re-rank / memória | `RERANKER.V7` (fallback `RERANKER.BIN`) | `HUB` (`k_ai::memory_systems`) |
| Learner | 1B / auto-aprendizado | `LEARNER.V7` (fallback `FALCON1B.v6`) | `HUB` |

Fora do hub (nomes já únicos, **não** renomear): `BGE.BIN` (embedding, `k_ai::memory_systems`),
`PIPER.BIN`/`PIPER_EN.BIN` (TTS), `STT.BIN` (CTC), `ROUTER.BITNET` (MoE router).

**Nota 8.3:** o kernel não tem LFN (pula entradas `0x0F`, `fat32.rs:499`+), então **todo nome precisa
caber em 8.3 uppercase** (`.V7` = 2 chars de extensão, cabe). `ROUTER.BITNET` (6-char ext) não cabe
→ precisa virar `ROUTER.BIT` (alias legado mantido).

## 5. Lições

- **Grep que exclui um diretório produz conclusões falsas** — "hub_status sem caller" e "alias só em
  código morto" vieram de um grep que não cobria `neural-kernel/src/`. Antes de afirmar ausência,
  grepar o repo inteiro (ou dizer "não achei em X", não "não existe").
- **Fallback por RAM não é fallback por ausência** — `maybe_fit` só caía em `too_tight`; um nicho
  ausente com RAM folgada devolvia slot vazio. Ausência e RAM devem cair no mesmo caminho.
- **Fallback é degradação → warn** — o log saía como `ok`; sucessor que rebaixa capacidade tem que
  dizer. O motivo (`absent`|`too_tight`) entra no log para o diagnóstico.
- **Presença precisa distinguir "blob real" de "alias"** — `slot_loaded` mente para Pro quando o
  Active tem embed_dim alto; `slot_real` separa os casos.
- **Tick baixo em boot de LLM ≠ regressão** — o falcon3 segura o tick budget na inferência; `tick=1`
  com `T+` subindo é o comportamento conhecido (mesmo do s479).

## 6. Pendentes

- Passo 1 `SLOTS=` + nomes canônicos gravados pelo build + guard 8.3 no writer (`mkfat32.py:729`).
- Passo 2 `validate_niche_blob`; Passo 4 validator entre read e load.
- Passo 5-6 podar `fat_names_for`, apagar `slot_from_bitnet_bytes` + `load_models_multi` (sem callers).
- Nichos além do gerador (Vision/Reranker/Learner/BGE/Piper/STT) ainda sem resolução própria (só o
  gerador tem `select_generator_slot`); subsistemas já degradam ad-hoc (ex.: BGE→`emb=pseudo`).
- `mesh peers=0`; metal USB BOOT.LOG (s467); F1 1h 8c/8GB.