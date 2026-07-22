# ADR-0063: RAG DB in-kernel — Vector store TF-IDF + cosine para RAG on-device

**Data:** 2026-07-22
**Status:** Proposed — especificação técnica de um vector database in-kernel para RAG (Retrieval-Augmented Generation) on-device, sem dependência de MemPalace externo. Sem implementação nesta ADR.
**Lifecycle (INDEX):** `por_fazer`
**Estende:** ADR-0019 (Cortex/BitNet), ADR-0033 (On-Device Micro-Learning), ADR-0047 (Latent Space AI-OS), ADR-0060 (BitNet Cognitivo BEI — memória L0-L7)
**Referência:** ClaudioOS `kernel/src/vectordb.rs` (1.062 LOC) — especificação técnica extraída do código-fonte lido integralmente
**IDEA_BANK:** #486 (ver §7)

---

## 1. Contexto e problema

### 1.1 Gap atual

O neural-os-core depende do **MemPalace MCP** (servidor externo) para memória semântica e RAG. Isso viola o princípio bare-metal autossuficiente: se o MCP cai ou não está configurado, Cortex/Trinity não têm contexto de memória para retrieval. ClaudioOS resolve isso com um **vector DB in-kernel** (`vectordb.rs`, 1.062 LOC) que roda inteiramente em `no_std` com `alloc`, sem Postgres, sem serviços externos.

### 1.2 Por que TF-IDF e não embeddings neurais

ClaudioOS usa **TF-IDF (Term Frequency - Inverse Document Frequency)** com cosine similarity, **não** embeddings neurais (BERT/MiniLM). Razões (validadas no código):

1. **Sem dependência de modelo**: TF-IDF é algoritmo puro, não precisa carregar pesos .bin/.gguf. Em bare-metal, carregar um modelo de embeddings (300MB+) consome RAM e VRAM preciosas.
2. **Sem FPU pesada**: TF-IDF é multiplicação de contagens + log. O `ln_f32()` é implementado via IEEE 754 bit tricks (decomposição mantissa/exponente + polinômio Pade) — **zero libm**.
3. **Latência baixa**: indexação e query são O(n × vocab) com n = número de documentos. Para memória de agente (centenas a milhares de entradas), é instantâneo.
4. **Suficiente para RAG de agente**: TF-IDF captura relevância lexical. Para memória de agente (frases, decisões, contexto), relevância lexical é adequada. Embeddings semânticos seriam melhores mas custam 100x mais.
5. **Serialização JSON**: o store serializa para JSON (sem serde — parser JSON próprio em 200 LOC), permitindo persistência em VFS.

**Trade-off honesto:** TF-IDF não captura sinonímia/paráfrase (ex: "gato" vs "felino"). Para RAG de agente, isso é aceitável. Para RAG de documentos grandes com vocabulário variado, embeddings seriam necessários — **residual** (ver §5).

---

## 2. Especificação técnica (extraída de ClaudioOS `vectordb.rs`)

### 2.1 Estrutura de dados

```rust
pub struct VectorEntry {
    pub id: String,           // "vec_1", "vec_2", ... (auto-gerado)
    pub text: String,          // texto original do documento
    pub embedding: Vec<f32>,   // vetor TF-IDF (denso, tamanho = vocab_size)
    pub metadata: BTreeMap<String, String>,  // agent, type, timestamp, etc.
}

pub struct VectorStore {
    vocabulary: BTreeMap<String, usize>,  // word → dimension index
    entries: Vec<VectorEntry>,
    df: Vec<u32>,            // document frequency per term
    doc_count: u32,
    next_id: u64,
    dirty: bool,             // vocab cresceu → rebuild embeddings
}
```

### 2.2 Tokenização

```rust
fn tokenize(text: &str) -> Vec<String>
```

- Lowercase
- Split por não-alfanumérico (exceto `_` e `-`)
- Filtro: tokens com `len >= 2` E não-stopword
- Stopwords: ~80 palavras English comuns (`the`, `is`, `at`, `of`, `on`, `in`, `to`, `for`, `and`, `or`, `an`, `as`, `it`, `be`, `by`, `if`, `no`, `so`, `do`, `he`, `we`, `my`, `up`, `am`, `me`, `us`, `not`, `but`, `are`, `was`, `has`, `had`, `its`, `can`, `may`, `our`, `you`, `all`, `any`, `who`, `how`, `did`, `get`, `got`, `set`, `let`, `new`, `old`, `use`, `now`, `way`, `own`, `see`, `say`, `her`, `him`, `his`, `she`, `they`, `them`, `this`, `that`, `with`, `from`, `have`, `been`, `were`, `will`, `what`, `when`, `your`, `than`, `each`, `just`, `also`, `into`, `over`, `such`, `some`, `very`, `only`, `then`, `more`, `about`, `which`, `would`, `could`, `should`, `there`, `their`, `these`, `those`, `other`)

**Adaptação neural:** adicionar stopwords PT-BR (`o`, `a`, `de`, `que`, `em`, `um`, `para`, `com`, `não`, `uma`, `os`, `as`, `dos`, `das`, `ao`, `aos`, `pelo`, `pela`, `seu`, `sua`, `mais`, `mas`, `ou`, `nem`, `também`, `já`, `quando`, `onde`, `como`, `porque`, `então`).

### 2.3 TF-IDF computation

```rust
fn compute_tfidf(&self, tokens: &[String]) -> Vec<f32>
```

Para cada termo `i` no documento:
- **TF** (term frequency) = `count_i / total_tokens`
- **IDF** (inverse document frequency) = `ln(N / df_i) + 1` (smoothed, evita zero)
- **TF-IDF** = `TF × IDF`

Onde:
- `N` = `doc_count` (total de documentos inseridos)
- `df_i` = document frequency do termo `i` (quantos docs contêm o termo)

Vetor resultado: `Vec<f32>` de tamanho `vocab_size`, denso (zeros explícitos).

### 2.4 ln_f32 sem libm (IEEE 754 bit tricks)

```rust
fn ln_f32(x: f32) -> f32 {
    // Decompor x = m * 2^e onde 1 <= m < 2
    let bits = x.to_bits();
    let e = ((bits >> 23) & 0xFF) as i32 - 127;
    let m_bits = (bits & 0x007F_FFFF) | 0x3F80_0000;  // exponent = 127
    let m = f32::from_bits(m_bits);
    // ln(x) = e * ln(2) + ln(m)
    let t = m - 1.0;
    let ln_m = t * (1.0 - t * (0.5 - t * (1.0/3.0 - t * (0.25 - t * 0.2))));
    let ln2: f32 = 0.693_147_2;
    (e as f32) * ln2 + ln_m
}
```

Acurácia: ~4 dígitos decimais para `x > 0`. Suficiente para TF-IDF (precisamos ordenação relativa, não valores exatos).

### 2.5 sqrt_f32 sem libm (Newton-Raphson)

```rust
fn sqrt_f32(x: f32) -> f32 {
    // Initial guess: halving exponent bits (IEEE 754)
    let bits = x.to_bits();
    let guess_bits = (bits >> 1) + 0x1FC0_0000;
    let mut guess = f32::from_bits(guess_bits);
    // 2 Newton-Raphson iterations: guess = (guess + x/guess) / 2
    guess = 0.5 * (guess + x / guess);
    guess = 0.5 * (guess + x / guess);
    guess
}
```

Acurácia: ~6 dígitos (limite de f32).

### 2.6 Cosine similarity

```rust
fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len().min(b.len());
    let mut dot = 0.0; let mut mag_a = 0.0; let mut mag_b = 0.0;
    for i in 0..len {
        dot += a[i] * b[i];
        mag_a += a[i] * a[i];
        mag_b += b[i] * b[i];
    }
    // Handle remaining elements in longer vector
    for i in len..a.len() { mag_a += a[i] * a[i]; }
    for i in len..b.len() { mag_b += b[i] * b[i]; }
    if mag_a == 0.0 || mag_b == 0.0 { return 0.0; }
    dot / (sqrt_f32(mag_a) * sqrt_f32(mag_b))
}
```

### 2.7 Operações do store

| Operação | Assinatura | Descrição |
|---|---|---|
| `new()` | `VectorStore` | Store vazio |
| `insert(text, metadata)` | `String` (id) | Tokeniza, atualiza vocab + df, computa TF-IDF, armazena entry. Se vocab cresceu, rebuild todos embeddings. |
| `insert_with_id(id, text, metadata)` | `()` | Insert com ID específico (para load de persistência) |
| `search(query, top_k)` | `Vec<(f32, &VectorEntry)>` | Tokeniza query, computa TF-IDF, cosine vs todos, ordena desc, truncate top_k, filtra score > 0.0 |
| `delete(id)` | `bool` | Remove entry (não decrementa df/doc_count — IDF fica levemente off mas funcional) |
| `update(id, new_text)` | `bool` | Remove + re-insert com mesmo ID e metadata |
| `all_entries()` | `&[VectorEntry]` | Dump para debug |
| `len()`, `is_empty()`, `vocab_size()` | `usize` | Metadata |

### 2.8 Serialização JSON (sem serde)

```rust
fn to_json(&self) -> Vec<u8>
fn from_json(data: &[u8]) -> Result<Self, String>
```

Schema JSON:
```json
{
  "vocabulary": {"word": 0, "another": 1},
  "doc_count": 42,
  "next_id": 43,
  "df": [3, 1],
  "entries": [
    {"id": "vec_1", "text": "...", "metadata": {"agent": "cortex", "type": "decision"}}
  ]
}
```

**Embeddings NÃO são serializados** — são rebuildados on load via `compute_tfidf` com a vocabulary carregada. Economiza espaço.

Parser JSON próprio (~200 LOC): `find_matching_brace`, `find_matching_bracket`, `split_json_objects`, `extract_json_string`, `extract_json_object`, `parse_string_int_map`, `parse_string_string_map`, `parse_quoted_string`, `skip_ws`.

### 2.9 Global store

ClaudioOS usa `static mut VECTOR_STORE: Option<VectorStore>` sem Mutex (reconhecem no comentário: "cooperative single-threaded executor, mutex overhead unnecessary").

**Adaptação neural:** neural-os-core tem scheduler multi-agente. Usar `spin::Mutex<VectorStore>` ou `ticket_lock` para thread-safety. O custo do lock é desprezível vs a query cosine O(n × vocab).

### 2.10 Persistência VFS

```rust
pub fn vfs_path() -> &'static str  // "/var/claudio/vectordb.json"
pub fn load_global_store(data: &[u8]) -> Result<(), String>
pub fn serialize_global_store() -> Vec<u8>
```

**Adaptação neural:** path `/var/neural/vectordb.json` (ou via VFS layer — ADR-0062 P2). Persistência em FAT32 (atual) ou ext4 (após ADR-0062 P5).

---

## 3. Decisão

Implementar um **VectorStore in-kernel** no neural-os-core, baseado na especificação do ClaudioOS `vectordb.rs`, com as seguintes adaptações:

### 3.1 Localização no workspace

Nova crate `crates/vector-db` (no_std, alloc), re-exportada por `cortex` (que já é R2 — camada cognitiva). Justificativa: vector DB é infraestrutura cognitiva, não HAL (k_nano) nem orquestração (hermes).

```
crates/vector-db/         # nova crate
  src/
    lib.rs                # VectorStore, VectorEntry
    tfidf.rs              # compute_tfidf, ln_f32, sqrt_f32
    similarity.rs         # cosine_similarity
    tokenize.rs           # tokenize, is_stopword (EN + PT-BR)
    json.rs               # to_json, from_json, parser próprio
    tests.rs              # self-tests (assert-based, sem framework)
```

### 3.2 Adaptações vs ClaudioOS

| Aspecto | ClaudioOS | Neural-os-core | Motivo |
|---|---|---|---|
| Thread-safety | `static mut` sem lock | `spin::Mutex` ou `ticket_lock` | Scheduler multi-agente |
| Stopwords | English apenas | English + **PT-BR** | Piper TTS é PT-BR+EN (ADR-0045) |
| VFS path | `/var/claudio/vectordb.json` | `/var/neural/vectordb.json` | Namespace próprio |
| Crate location | `kernel/src/vectordb.rs` (monólito) | `crates/vector-db` (crate isolada) | K³CHJ workspace por anéis |
| Integração | RAG para agentes Claude | RAG para **Cortex/Trinity MoE** + agentes | LLM on-device, não nuvem |
| Metadata schema | livre (`BTreeMap<String,String>`) | schema com campos canônicos | Ver §3.3 |
| Testes | `#[cfg(test)] mod tests` (não roda no bare-metal) | `demo()` self-check assert-based | Política neural: test = `demo()` que falha se quebrar |

### 3.3 Metadata schema canônico

```rust
pub struct EntryMetadata {
    pub agent: String,        // "cortex", "hermes", "rustcoder", ...
    pub kind: EntryKind,      // Decision, Memory, Skill, Session, Reference
    pub timestamp: u64,        // millis since boot (ou RTC wall clock)
    pub tags: Vec<String>,    // free-form tags
    pub source: Option<String>, // arquivo/URL de origem
}
```

### 3.4 Integração com Cortex/Trinity

```
User input → Hermes → Cortex
  ↓
VectorStore.search(user_input, top_k=5)
  ↓
Retrieved context (top 5 entries) → injected into LLM prompt
  ↓
LLM generates response with RAG context
  ↓
Response + metadata → VectorStore.insert() (novo conhecimento)
```

### 3.5 Integração com ADR-0060 (BitNet Cognitivo BEI — memória L0-L7)

ADR-0060 Onda 4 define **memória L0-L7** (tiers de memória cognitiva). O VectorStore in-kernel é a **camada L1** (memória semântica de curto prazo, TF-IDF). Tiers superiores (L4-L7, embeddings neurais, memória afetiva) são residual — esta ADR cobre apenas L1.

| Tier | ADR-0060 | Esta ADR |
|---|---|---|
| L0 | Memória sensorial (raw) | — |
| L1 | Memória semântica curto prazo | **✅ VectorStore TF-IDF** |
| L2 | Memória episódica | Residual |
| L3 | Memória de procedimentos (skills) | ADR-0059 (WASM) |
| L4-L7 | Embeddings neurais, memória afetiva, Soul Mirror | Residual |

---

## 4. Plano de implementação (fases — cada uma compila 0-erros + boota + testável)

### F1 — Crate `vector-db` fundação
- Criar `crates/vector-db` no_std + alloc
- `VectorStore`, `VectorEntry`, `EntryMetadata`
- `tokenize` com stopwords EN + PT-BR
- `compute_tfidf`, `ln_f32`, `sqrt_f32`
- `cosine_similarity`
- `demo()` self-check: insert 3 docs, search "rust kernel", assert top result contém "Rust" ou "kernel"

### F2 — Serialização JSON + persistência VFS
- `to_json`, `from_json` com parser JSON próprio
- `vfs_path()`, `load_global_store()`, `serialize_global_store()`
- Persistir em FAT32 (`/var/neural/vectordb.json`)
- `demo()` roundtrip: insert → serialize → deserialize → assert igual

### F3 — Integração Cortex/Trinity
- Re-exportar de `cortex`
- Hook em `CortexAgent::tick()`: antes de chamar LLM, `search(user_input, 5)` → inject no prompt
- Hook pós-resposta: `insert(response, metadata)` com agent="cortex", kind=Memory
- Teste: conversa multi-turn, segunda pergunta recupera contexto da primeira

### F4 — Integração Hermes (RAG para skills)
- Hermes consulta VectorStore para recuperar skills relevantes antes de rotear intent
- `kind=Skill` entries indexadas por descrição de skill
- Teste: "preciso de algo para ler arquivos" → recupera skill `file_read`

### F5 — Thread-safety + multi-store
- `spin::Mutex<VectorStore>` global
- Suporte a múltiplos stores nomeados (per-agente, per-domínio)
- `get_store(name) -> &'static Mutex<VectorStore>`

### F6 — Residual: embeddings neurais (L4+)
- Carregar modelo de embeddings (MiniLM 22MB, ou BitNet-derived) via ADR-0046 (AirLLM/GGUF)
- `compute_embedding(text) -> Vec<f32>` substitui `compute_tfidf`
- Cosine similarity permanece
- Trade-off: 100x mais RAM/VRAM, melhor semântica
- **Gated** por disponibilidade de VRAM (GPU) — fallback TF-IDF se sem GPU

---

## 5. Riscos e trade-offs

### 5.1 TF-IDF vs embeddings (honesto)

| | TF-IDF (esta ADR) | Embeddings neurais (F6 residual) |
|---|---|---|
| RAM | ~KB (vocab × docs × 4 bytes) | 22MB+ (modelo) |
| VRAM | 0 | Opcional (GPU) |
| Latência query | O(n × vocab), microssegundos | O(n × dim), milissegundos |
| Semântica | Léxica (sinonímia ❌) | Semântica (sinonímia ✅) |
| Dependência | Nenhuma (algoritmo puro) | Modelo .gguf/.bin |
| Bare-metal | ✅ ideal | 🟡 possível (ADR-0046) |

**Decisão:** TF-IDF primeiro (F1-F5), embeddings como F6 residual gated por VRAM.

### 5.2 Vocabulário crescente

Cada documento novo pode adicionar palavras ao vocabulário, fazendo `vocab_size` crescer. O ClaudioOS faz `rebuild_embeddings()` quando `dirty=true` (vocab cresceu). Para milhares de documentos, o rebuild é O(n × vocab) — aceitável. Para milhões, seria problemático — **residual**: sharding ou vocab fixo com hashing.

### 5.3 df não decrementado no delete

ClaudioOS não decrementa `df`/`doc_count` no `delete()` (reconhecido no código: "IDF values slightly off but still functional"). Para neural-os-core, aceitar o mesmo trade-off (simplicidade > precisão IDF após deletes).

### 5.4 Parser JSON próprio vs serde

ClaudioOS usa parser JSON próprio (~200 LOC) para evitar serde no kernel. Neural-os-core **já tem serde** no workspace (`serde_json` com feature `alloc`). **Decisão:** usar `serde_json` para serialização (já disponível), mantendo o schema JSON compatível com ClaudioOS para interoperabilidade. Reduz código próprio.

### 5.5 Thread-safety

ClaudioOS usa `static mut` sem lock (single-threaded async). Neural-os-core tem scheduler multi-agente (Cortex, Hermes, RustCoder podem acessar o store concorrentemente). **Decisão:** `spin::Mutex<VectorStore>` global. Custo do lock é desprezível vs query O(n × vocab).

---

## 6. Validação

### 6.1 Self-tests (F1)

```rust
fn demo() {
    let mut store = VectorStore::new();
    store.insert("Rust bare metal OS development", metadata("test", Decision));
    store.insert("Python scripting and data science", metadata("test", Decision));
    store.insert("Bare metal kernel development in Rust", metadata("test", Decision));
    let results = store.search("Rust kernel", 2);
    assert!(!results.is_empty());
    assert!(results[0].1.text.contains("Rust") || results[0].1.text.contains("kernel"));
    // cosine identity
    let v = vec![1.0, 2.0, 3.0];
    assert!((cosine_similarity(&v, &v) - 1.0).abs() < 0.001);
    // cosine orthogonal
    assert!(cosine_similarity(&[1.0,0.0], &[0.0,1.0]).abs() < 0.001);
}
```

### 6.2 Validação de integração (F3)

- Boot QEMU, conversa multi-turn com Cortex
- Segunda pergunta recupera contexto da primeira (verificar no log serial: `[vectordb] search "..." → top: "..."`)
- Persistir, rebootar, verificar se memória sobrevive (`load_global_store` no boot)

### 6.3 Performance (F5)

- Insert 1000 documentos, medir tempo
- Query com vocab 5000, medir latência
- Alvo: insert < 100ms, query < 10ms (em QEMU TCG)

---

## 7. IDEA_BANK

| # | Ideia | Destino | Status |
|---|---|---|---|
| #486 | Vector DB in-kernel TF-IDF para RAG on-device | Esta ADR (F1-F5) | ⏳ |
| #487 | Embeddings neurais in-kernel (L4+ residual) | F6 residual | ⏳ |

---

## 8. Conclusão

Um vector DB in-kernel com TF-IDF + cosine similarity é **viável, leve e suficiente** para RAG de agente on-device. A especificação do ClaudioOS `vectordb.rs` é referência técnica sólida (algoritmo puro, sem libm, JSON serialize, global store). As adaptações para neural-os-core são: crate isolada no workspace K³CHJ, thread-safety com `spin::Mutex`, stopwords PT-BR, integração com Cortex/Trinity (não Claude nuvem), e serde_json (já disponível) em vez de parser JSON próprio.

TF-IDF é a camada L1 da memória cognitiva (ADR-0060 Onda 4). Embeddings neurais (L4+) são residual gated por VRAM. Esta ADR desbloqueia RAG on-device sem dependência de MemPalace MCP externo, fortalecendo o princípio bare-metal autossuficiente.

---

## Referências

- ClaudioOS `kernel/src/vectordb.rs` (1.062 LOC) — código-fonte lido integralmente
- ADR-0019: Neural Cortex BitNet LLM
- ADR-0033: On-Device Micro-Learning
- ADR-0045: Sound Voice Stack (Piper TTS PT-BR — justifica stopwords PT-BR)
- ADR-0046: AirLLM GGUF Streaming (embeddings residuais F6)
- ADR-0047: Latent Space AI-OS
- ADR-0060: BitNet Cognitivo BEI (memória L0-L7 — esta ADR cobre L1)
- ADR-0062: ClaudioOS vs Neural-OS (companheira — adoção seletiva de infraestrutura)
