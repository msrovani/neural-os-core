# SESSION_480 — bge-m3 deixa de entrar em panic: `resize_heap_to_mb` fail-closed + `NotLlm` propaga (mata o "590680KB")

**Data:** 2026-10-17 · **Estado:** v1.9.99-s480 TEST · **Branch:** `main`
**Contexto:** s479 deixou aberto o `bge-m3` — que carregava um modelo de embedding de
135 MB como LLM generativo e quebrava o boot. Esta sessão fecha os dois passos que
matam a causa raiz, com prova de runtime.

---

## 1. Passo 0 — `resize_heap_to_mb` fail-closed pós-claim

**Causa mecânica (VERIFIED no código, allocator.rs:1641-1685):** `CURRENT_HEAP_MB`
(`:1173`, default 512) é o teto do **BUMP** (setado em `:455`), mas `resize_heap_to_mb`
o usava como se fosse o tamanho do **TALC** — cujo span de boot é `budget` (~75% RAM,
`:2089`). Um delta pequeno passava o gate `can_alloc_bytes` (`:1648`) e chamava
`guard.extend(old, req)` com `req` **menor** que `old` → assert do talc
(`new_heap must contain old_heap`). Pior: `start_virt = HEAP_START + current*1MB`
(`:1654`) mapeava páginas **dentro** do span TALC já reivindicado.

Medido no bge-m3: `current=512MB`, `target=1795MB` → `req`=1,75 GB contra `old`=~5 GB.

**Correção:** recusa fail-closed quando `TALC_READY` — pós-claim o span já cobre
`budget` e é demand-paged. Único caller é `cortex.rs:2630` (caminho legado do
`load_model`); `memory_agent.rs:214` **já tinha removido** o dele com o comentário
*"resize_heap_to_mb mirava a pool errada"*.

## 2. Passo 3 — `NotLlm` propaga e a constante fantasma morre

**Causa raiz do "590680KB":** `main.rs:3953` tinha
`const BITNET_2B_V4_BYTES: usize = 604_856_373` → `/1024` = **590680 KB**. Esse é
exatamente o número que **nenhum artefato do repo tem** e que aparecia em 5 de 8 logs
da matriz do s479. Ele existia como *fallback* de `qemu_loader_llm_len`, cujo `_ => break`
(`:291`) **descartava o veredito `NotLlm`** que `probe_llm_file_size` já calculava
corretamente (`model.rs:164-173`: version ∉ {6,7} ou `model_type` ≠ 0).

Resultado: um `BGE_M3.BIN` (embedding, 135 MB) ou `ROUTER.BITNET` (25 KB) passava pelo
magic `0xBE11BE11` e era tratado como LLM com shapes fabricados (`layers=24`).

**Correção:**
- `qemu_loader_llm_len` passa a devolver `Option<usize>`; `NotLlm` e probe esgotado → `None`.
- A constante foi removida dos **dois** call sites (`:3953` e `:4062`, o fallback de `0x120000000`).
- O comprimento do blob no loader vem **só** do probe. `fat_sz` descreve o arquivo no
  disco e pode ser **outro modelo** — não pode virar comprimento da região do loader.
- Rejeição **informa e segue**: `blob @0x100000000 tem magic BitNet mas nao e LLM (v6/v7 + model_type=0) - loader ignorado`.

## 3. Prova de runtime (bge-m3, nó único × 7G, 240 s)

| Métrica | Antes (s479) | Depois (s480) |
|---|---|---|
| `tick` | 65 | **8448** |
| `[PANIC]` literal | `cortex.rs:534 unwrap None` + talc | **NENHUM** |
| carregamento | BGE como LLM `layers=24` | recusado como LLM |
| subsistema | — | `[BGE] ausente - emb=pseudo (hash/TF bits)` |
| `PF` | 10 | 8 (só demos Ring3 P6) |

Log: `logs/boot_mesh_a_BGE_M3.txt` (run `20261017_113653`).

## 4. Achado de harness: meu próprio script mentiu

O `run-mesh-model.ps1` reportou `Panic=1` — mas `[PANIC]` **literal, case-sensitive**,
é **NENHUM**. O script conta `panic` case-insensitive e casa com o registro
cross-boot do k_ai SELF (`boot_failure ... panic=true`), que lê o ramlog do boot
**anterior**. Mesmo padrão já visto no s477: `panic=1` = registro cross-boot.

**Regra:** veredito de panic = `[PANIC]` literal case-sensitive; nunca regex
case-insensitive de "panic".

## 5. Lições

- **TALC span pós-claim é `budget`, não `CURRENT_HEAP_MB`** — misturar o teto do BUMP com o
  do TALC faz `extend` receber um `req` menor que o `old` e estourar o assert do talc.
- **Veredito calculado certo e descartado é bug silencioso** — `probe_llm_file_size` já
  classificava `NotLlm` corretamente; o `_ => break` jogava fora e caía num fallback que
  era uma **constante de 604 MB**. Fallback que é número fixo = tamanho que nenhum
  artefato tem, e todo arquivo com o mesmo magic vira LLM fantasma.
- **Comprimento do blob no loader não pode vir de `fat_sz`** — `fat_sz` descreve o disco
  e pode ser outro modelo; usá-lo como comprimento da região do loader fabrica um LLM
  fantasma a partir de um BGE.
- **Harness com regex case-insensitive mente** — `panic` casa `panic=true` do registro
  cross-boot. Veredito = `[PANIC]` literal.
- **Recusar honestamente é melhor que carregar errado** — o bge-m3 agora não carrega como
  LLM, o subsistema BGEAssume fallback (`emb=pseudo`) e o boot segue. Isso é o princípio
  "não cai, roteia/fallback e informa".

## 6. Pendentes (plano do hub, ora-3)

- **resolve/informa**: `escalate slot=pro → active reason=too_tight` hoje sai com severidade
  `ok` (fallback é degradação → `warn`); `hub_status()` não tem **nenhum caller**; o fallback
  só dispara por RAM (`too_tight`), não por ausência; nichos além do gerador (Vision,
  Reranker, Learner, BGE, Piper, STT) não têm resolução de nicho.
- Passo 2 `validate_niche_blob`; Passo 4 validator entre read e load; Passo 1 `SLOTS=` +
  nomes canônicos 8.3 no build; Passo 5-6 podar `fat_names_for`, apagar
  `slot_from_bitnet_bytes` + `load_models_multi` (sem callers).
- `PK / mesh peers=0`; metal USB BOOT.LOG (s467); F1 1h 8c/8GB.