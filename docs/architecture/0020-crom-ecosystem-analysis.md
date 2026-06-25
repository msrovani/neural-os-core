# ADR-0020: Crom Ecosystem Analysis — Ideas Ported to neural-os-core

**Status:** Adopted (2026-06-24)  
**Context:** Analysis of 75 repositories from MrJc01 (Ecossistema Crom) — Go, Python, JS/TS — for ideas viable in bare-metal `no_std` Rust.  
**References:** https://github.com/MrJc01

---

## 1. XOR Delta Reconstruction (Imediata — Sprint 24)

**Origem:** crompressor — Archive mode: armazena resíduo XOR para round-trip lossless bit-exact.

**Análise:** `PackedTernaryTensor` já codifica pesos em 2 bits. O delta XOR entre o original f32 e o reconstruído do codebook cabe em 1 byte extra por peso. Operação é puramente bitwise, zero FPU.

```rust
// Extensão do PackedTernaryTensor para Archive mode
pub struct ArchiveTensor {
    pub shape: (usize, usize),
    pub codebook: PackedTernaryTensor,  // modo Edge (lossy)
    pub delta: Vec<u8>,                 // resíduo XOR (Archive = lossless)
}

impl ArchiveTensor {
    pub fn compress(original: &Tensor, threshold: f32) -> Self {
        let codebook = quantize_to_packed(original, threshold);
        let mut delta = Vec::with_capacity(original.data.len());
        for (i, &orig) in original.data.iter().enumerate() {
            let w = codebook.get_weight(i);
            let reconstructed = match w {
                1 => threshold,
                -1 => -threshold,
                _ => 0.0,
            };
            // XOR do bit pattern do f32 com o reconstruído
            let orig_bits = orig.to_bits();
            let recon_bits = reconstructed.to_bits();
            delta.push((orig_bits ^ recon_bits) as u8);
        }
        ArchiveTensor { shape: original.shape, codebook, delta }
    }

    pub fn decompress(&self) -> Tensor {
        let mut data = Vec::with_capacity(self.delta.len());
        for i in 0..self.delta.len() {
            let w = self.codebook.get_weight(i);
            let reconstructed = match w {
                1 => 1.0, -1 => -1.0, _ => 0.0,
            };
            let recon_bits = reconstructed.to_bits();
            let orig_bits = recon_bits ^ (self.delta[i] as u32);
            data.push(f32::from_bits(orig_bits));
        }
        Tensor::from_row_major(self.shape, data).unwrap()
    }
}
```

**Dependências:** Nenhuma. ~50 LOC, só `Vec<u8>` + `f32::to_bits()`.

---

## 2. CDC Rabin Fingerprint (Imediata — Sprint 24)

**Origem:** crompressor — Content-Defined Chunking via Rabin fingerprint, chunking semântico de tamanho variável.

**Análise:** Rolling hash polinomial em `no_std` é direto. Opera sobre `&[u8]`, sem heap além do output.

```rust
pub struct RabinChunker {
    window: [u8; 48],
    pos: usize,
    hash: u64,
    poly: u64,      // polinômio ex: 0x3DA3358B4DC173
    mod_val: u64,   // define tamanho médio do chunk
}

impl RabinChunker {
    pub fn new(poly: u64, avg_shift: u32) -> Self {
        RabinChunker {
            window: [0u8; 48],
            pos: 0,
            hash: 0,
            poly,
            mod_val: 1u64 << avg_shift,
        }
    }

    pub fn slide(&mut self, byte: u8) -> u64 {
        let out = self.window[self.pos];
        self.window[self.pos] = byte;
        self.pos = (self.pos + 1) % 48;
        // Rolling hash: remove byte de saída, adiciona novo byte
        self.hash = self.hash.wrapping_mul(self.poly)
            .wrapping_add(byte as u64)
            .wrapping_sub((out as u64).wrapping_mul(
                self.poly.wrapping_pow(48)
            ));
        self.hash
    }

    pub fn is_boundary(&self) -> bool {
        (self.hash & (self.mod_val - 1)) == 0
    }
}
```

**Dependências:** Nenhuma. ~80 LOC. `wrapping_mul/add/sub` são intrínsecas do processador.

**Aplicação:** Dividir modelos `.bitnet` > 1 MB em chunks para carregamento sob demanda do SSD (quando NVMe driver existir, Sprint 24+).

---

## 3. Multi-mode Trust (Baixa — Sprint 27)

**Origem:** crom-agente — 3 modos de permissão: total_access, ask_every_time, scoped (com grants salvos).

**Análise:** `trust.rs` já tem TrustCache com allow/deny. Estender para `PermissionMode` enum com grants temporários por escopo.

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum PermissionMode {
    TotalAccess,
    AskEveryTime,
    Scoped,
}

#[derive(Debug, Clone)]
pub struct ScopeGrant {
    pub pattern: String,       // ex: "git *" ou "/src/*"
    pub granted_at_ticks: u64,
    pub ttl_ticks: u64,
}

pub struct MultiModeTrust {
    entries: BTreeMap<(u64, String), TrustEntry>,
    denylist: BTreeMap<(u64, String), ()>,
    mode: PermissionMode,
    scope_grants: BTreeMap<(u64, String), ScopeGrant>,
}

impl MultiModeTrust {
    pub fn new(mode: PermissionMode) -> Self { ... }

    pub fn authorize(&self, token: u64, skill: &str, action: &str, now: u64) -> bool {
        match self.mode {
            PermissionMode::TotalAccess => true,
            PermissionMode::AskEveryTime => false,
            PermissionMode::Scoped => {
                // Check scope grants first
                if let Some(grant) = self.scope_grants.get(&(token, skill.to_string())) {
                    if now - grant.granted_at_ticks <= grant.ttl_ticks {
                        if action_matches_pattern(action, &grant.pattern) {
                            return true;
                        }
                    }
                }
                self.is_trusted(token, skill, now)
            }
        }
    }

    pub fn grant_scope(&mut self, token: u64, skill: &str, pattern: &str, ttl: u64, now: u64) {
        self.scope_grants.insert(
            (token, skill.to_string()),
            ScopeGrant { pattern: pattern.to_string(), granted_at_ticks: now, ttl_ticks: ttl },
        );
    }
}
```

**Dependências:** Estende `trust.rs` (~100 LOC). Nenhuma nova dependência.

---

## 4. TV-DSL Co-processor (Baixa — Sprint 27)

**Origem:** crom-microllm-think-vetor — TV-DSL: tags `<thought>` com comandos estruturados, executados por AST determinístico.

**Análise:** Parser de expressões matemáticas em `no_std` usando `libm`. Hermes gera o comando DSL, co-processador executa deterministicamente (zero alucinação).

```rust
// Comandos DSL que o Hermes pode chamar
pub enum DslCommand {
    Add(Vec<f64>),           // [TV-DSL: add(2.5, 3.7)]
    Multiply(Vec<f64>),      // [TV-DSL: multiply(4.0, 0.3, 0.5)]
    Divide(f64, f64),        // [TV-DSL: divide(10.0, 3.0)]
    Sqrt(f64),               // [TV-DSL: sqrt(144)]
    Pow(f64, f64),           // [TV-DSL: pow(2.0, 8)]
    Volume(f64, f64, f64),   // [TV-DSL: volume(4.0, 0.3, 0.5)]
    Area(f64, f64),          // [TV-DSL: area(5.0, 3.0)]
    Percent(f64, f64),       // [TV-DSL: percent(250, 2000)] = 12.5%
}

impl DslCommand {
    pub fn parse(input: &str) -> Option<DslCommand> {
        // Extrai conteúdo entre [TV-DSL: ...]
        let inner = input.strip_prefix("[TV-DSL: ")?
            .strip_suffix("]")?;
        let (name, args_str) = inner.split_once('(')?;
        let args_str = args_str.strip_suffix(')')?;
        let args: Vec<f64> = args_str.split(',')
            .filter_map(|s| s.trim().parse::<f64>().ok())
            .collect();
        match name.trim() {
            "add" => Some(DslCommand::Add(args)),
            "multiply" => Some(DslCommand::Multiply(args)),
            "volume" => Some(DslCommand::Volume(args[0], args[1], args[2])),
            "percent" if args.len() == 2 => {
                Some(DslCommand::Percent(args[0], args[1]))
            }
            _ => None,
        }
    }

    pub fn execute(&self) -> f64 {
        match self {
            DslCommand::Add(v) => v.iter().sum(),
            DslCommand::Multiply(v) => v.iter().product(),
            DslCommand::Divide(a, b) => a / b,
            DslCommand::Sqrt(x) => libm::sqrt(*x),
            DslCommand::Pow(b, e) => libm::pow(*b, *e),
            DslCommand::Volume(w, h, d) => w * h * d,
            DslCommand::Area(w, h) => w * h,
            DslCommand::Percent(p, t) => (p / t) * 100.0,
        }
    }
}

// Integração no intent_router_daemon
async fn process_with_tvdsl(input: &str, llm_response: &str) -> String {
    if let Some(cmd_start) = llm_response.find("[TV-DSL: ") {
        let segment = &llm_response[cmd_start..];
        if let Some(cmd) = DslCommand::parse(segment) {
            let result = cmd.execute();
            return llm_response.replacen(segment,
                &format!("Resultado exato: {:.4}", result), 1);
        }
    }
    llm_response.to_string() // fallback: resposta textual do LLM
}
```

**Dependências:** `libm` (já existe). ~200 LOC.

---

## 5. PonderNet Dynamic Stop (Baixa — Sprint 27)

**Origem:** crom-microllm-think-vetor — PonderNet: loop recorrente com parada dinâmica baseada em confiança.

**Análise:** Reflex MLP atualmente executa número fixo de camadas. PonderNet permite que o modelo decida quantos ciclos executar.

```rust
pub struct PonderNet {
    pub core: crate::nn::Linear,  // 16→8
    pub halting: crate::nn::Linear, // 8→1 (sigmoid)
    pub max_steps: usize,
    pub lambda: f64,  // penalidade por passo extra
}

impl PonderNet {
    pub fn forward(&self, x: &Tensor, max_steps: usize) -> (Tensor, usize) {
        let mut h = x.clone();
        let mut accumulated = Tensor::new((1, 8));
        let mut remaining_prob = 1.0_f64;

        for step in 0..max_steps {
            h = self.core.forward(&h);
            h.apply(crate::nn::silu);

            // Cabeça de halting: decide se parou
            let halt_logit = self.halting.forward(&h);
            let halt_prob = sigmoid_f64(halt_logit.data[0] as f64);
            let step_prob = halt_prob * remaining_prob;
            remaining_prob -= step_prob;

            // Accumulate weighted by halt probability
            for (i, val) in h.data.iter().enumerate() {
                accumulated.data[i] += val * (step_prob as f32);
            }

            if remaining_prob < 0.01 {
                return (accumulated, step + 1);
            }
        }
        // Residual probability goes to last step
        for (i, val) in h.data.iter().enumerate() {
            accumulated.data[i] += val * (remaining_prob as f32);
        }
        (accumulated, max_steps)
    }
}

fn sigmoid_f64(x: f64) -> f64 {
    1.0 / (1.0 + libm::exp(-x))
}
```

**Dependências:** `libm` (já existe). ~150 LOC.

---

## 6. Codebook Compression VQ (Média — Sprint 28)

**Origem:** crompressor + crompressor-neuronio — Vector Quantization: substitui `quantize_to_packed()` por codebook aprendido + lookup O(1).

**Análise:** K-means em `no_std` para treinar codebook. MatMul ternário vira lookup no codebook.

```rust
pub struct Codebook {
    pub centroids: Vec<f32>,  // [K * D] flattened
    pub k: usize,
    pub dim: usize,
}

impl Codebook {
    pub fn new(k: usize, dim: usize) -> Self {
        Codebook { centroids: vec![0.0; k * dim], k, dim }
    }

    // Treinamento K-means (uma época)
    pub fn train(&mut self, data: &[f32], n: usize, lr: f32) {
        let mut counts = vec![0usize; self.k];
        for i in 0..n {
            let idx = i * self.dim;
            // Encontra centroide mais próximo
            let mut best_dist = f32::MAX;
            let mut best_k = 0;
            for c in 0..self.k {
                let c_idx = c * self.dim;
                let mut dist = 0.0;
                for d in 0..self.dim {
                    let diff = data[idx + d] - self.centroids[c_idx + d];
                    dist += diff * diff;
                }
                if dist < best_dist {
                    best_dist = dist;
                    best_k = c;
                }
            }
            // Move centroide em direção ao ponto
            for d in 0..self.dim {
                let c_idx = best_k * self.dim;
                let err = data[idx + d] - self.centroids[c_idx + d];
                self.centroids[c_idx + d] += err * lr;
            }
            counts[best_k] += 1;
        }
    }

    // Lookup O(1) — retorna índice do centroide
    pub fn lookup(&self, x: &[f32]) -> usize {
        let mut best_dist = f32::MAX;
        let mut best_k = 0;
        for c in 0..self.k {
            let c_idx = c * self.dim;
            let mut dist = 0.0;
            for d in 0..self.dim {
                let diff = x[d] - self.centroids[c_idx + d];
                dist += diff * diff;
            }
            if dist < best_dist {
                best_dist = dist;
                best_k = c;
            }
        }
        best_k
    }
}

// PackedCodebookTensor: pesos ternários via codebook
pub struct PackedCodebookTensor {
    pub shape: (usize, usize),
    pub codebook: Codebook,
    pub indices: Vec<u8>,     // índice no codebook (1 byte = 256 centroides)
}
```

**Dependências:** Nenhuma nova. ~300 LOC kernel + script Python para treinar codebook offline.

---

## 7. KV Cache Codebook (Média — Sprint 28)

**Origem:** crompressor-neuronio Lab06 — 94.2% de redução no KV cache aplicando VQ aos estados de atenção.

**Análise:** Só faz sentido depois do Transformer Engine (#126-131, Sprint 25). O cache K e V de cada camada é quantizado via codebook.

```rust
pub struct KvCacheCodebook {
    k_codebook: Codebook,
    v_codebook: Codebook,
    k_indices: Vec<Vec<u8>>,  // [camada][posicao] -> índice codebook
    v_indices: Vec<Vec<u8>>,
}

impl KvCacheCodebook {
    pub fn compress_kv(&mut self, k: &Tensor, v: &Tensor, layer: usize) {
        // Cada linha da matriz K/V vira um índice no codebook
        let (seq_len, dim) = k.shape;
        for pos in 0..seq_len {
            let k_slice = &k.data[pos * dim .. (pos + 1) * dim];
            let v_slice = &v.data[pos * dim .. (pos + 1) * dim];
            self.k_indices[layer].push(self.k_codebook.lookup(k_slice) as u8);
            self.v_indices[layer].push(self.v_codebook.lookup(v_slice) as u8);
        }
    }
}
```

**Dependências:** Transformer Engine (#126-131) + Codebook VQ (#169). ~200 LOC.

---

## 8. ReAct Loop com Auto-Correção (Média — Sprint 28)

**Origem:** crom-agente — ReAct loop (Reasoning + Acting) com fase de verificação, detecção de loop infinito, retry.

**Análise:** NeuralExecutor atual é cooperativo (poll + hlt). ReAct adiciona: (1) hash das últimas N ações para detectar loop, (2) auto-verificação via lint/test, (3) retry em erro.

```rust
pub struct ReActLoop {
    action_history: [u64; 16],  // hashes das últimas ações
    history_idx: usize,
    max_retries: usize,
}

impl ReActLoop {
    pub fn detect_loop(&mut self, action_hash: u64) -> bool {
        self.action_history[self.history_idx % 16] = action_hash;
        self.history_idx += 1;
        // Se mesma hash aparece 3+ vezes nas últimas 8 ações = loop
        let recent = &self.action_history[
            self.history_idx.saturating_sub(8)..self.history_idx
        ];
        let count = recent.iter().filter(|&&h| h == action_hash).count();
        count >= 3
    }

    pub fn execute_with_retry<F>(&self, mut action: F) -> Result<(), ()>
    where F: FnMut() -> Result<(), ()>
    {
        for attempt in 0..self.max_retries {
            match action() {
                Ok(()) => return Ok(()),
                Err(_) => {
                    serial_println!("[ReAct] Retry {}/{}", attempt + 1, self.max_retries);
                }
            }
        }
        Err(())
    }
}
```

**Integração:** `NeuralExecutor::run()` incorpora `ReActLoop` que envolve o poll de cada `AgentTask`. ~300 LOC.

---

## 9. MCP Server Support (Média — Sprint 28)

**Origem:** crom-agente — Cliente MCP (Model Context Protocol) nativo via JSON-RPC 2.0, suporta servidores MCP da comunidade.

**Análise:** EventBus + SkillRegistry evoluem para protocolo MCP. JSON-RPC 2.0 requer parser JSON em `no_std`. Alternativa: protocolo binário custom.

```rust
// Estrutura MCP simplificada (binária, sem parser JSON)
pub struct McpRequest {
    pub id: u64,
    pub method: [u8; 16],   // nome do método padding
    pub params_len: u16,
    pub params: [u8; 256],  // params serializados
}

pub struct McpResponse {
    pub id: u64,
    pub success: bool,
    pub data_len: u16,
    pub data: [u8; 1024],
}

pub struct McpSkill {
    pub manifest: crate::skill_registry::McpManifest,
    pub transport: McpTransport,
}

pub enum McpTransport {
    StdinStdout,   // subprocesso (quando scheduler existir, Sprint 24+)
}

impl Skill for McpSkill {
    fn execute(&self, context: &[u8]) -> Vec<u8> {
        let req = McpRequest {
            id: 1,
            method: *b"execute_skill\0\0\0\0\0",
            params_len: context.len() as u16,
            params: {
                let mut arr = [0u8; 256];
                let len = context.len().min(256);
                arr[..len].copy_from_slice(&context[..len]);
                arr
            },
        };
        // Enviar via transporte MCP e aguardar resposta
        // (requer comunicação entre processos)
        vec![] // stub
    }
}
```

**Dependências:** Nenhuma nova (protocolo binário). ~400 LOC. Parser JSON p/ compatibilidade total seria crate `serde_json` (~20 KB) — possível em `no_std` com `alloc`.

---

## Summary: Sprint Allocation

| Sprint | Item | Esforço | Arquivos |
|---|---|---|---|
| 24 | XOR Delta | ~50 LOC | `tensor.rs` |
| 24 | CDC Rabin | ~80 LOC | `new file: chunker.rs` |
| 27 | Multi-mode Trust | ~100 LOC | `trust.rs` |
| 27 | TV-DSL Co-processor | ~200 LOC | `new file: tvdsl.rs` |
| 27 | PonderNet | ~150 LOC | `nn.rs` ou `new file: pondernet.rs` |
| 28 | Codebook VQ | ~300 LOC | `tensor.rs` + Python script |
| 28 | KV Cache Codebook | ~200 LOC | `transformer.rs` (quando existir) |
| 28 | ReAct Loop | ~300 LOC | `executor.rs` |
| 28 | MCP Server | ~400 LOC | `skill-registry/` + `event-bus/` |

**Total linhas:** ~1.780 LOC kernel + ~300 LOC Python = ~2.080 LOC para todas as 9 features.
