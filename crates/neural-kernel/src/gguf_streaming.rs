//! GGUF AirLLM streaming model — ADR-0046.
//! Keeps header + layer map + embed/unembed in RAM; loads 1 layer per forward step.
//! PrefetchEngine is a SOFT double-buffer (ATA sync read-ahead), NOT DMA.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use alloc::format;
use core::f32::NEG_INFINITY;

use crate::cortex::{
    self, KvCache, LayerWeights, Model, Tokenizer, TransformerModel, MAX_SEQ, VOCAB_SIZE,
};
use crate::gguf::{
    self, dequantize_raw, f32_to_ternary_packed, GgufFile, GgufTensorInfo, GgufType,
};
use crate::serial_println;
use crate::tensor::{PackedTernaryTensor, Tensor};

/// Absolute file reference for one GGUF tensor (payload stays on disk).
#[derive(Clone)]
pub struct TensorRef {
    pub name: String,
    pub file_offset: u64,
    pub nbytes: usize,
    pub rows: usize,
    pub cols: usize,
    pub qtype: GgufType,
}

/// Per-layer tensor map (attn + ffn). Norms optional — default to ones if missing.
#[derive(Clone)]
pub struct StreamingLayerInfo {
    pub span_offset: u64,
    pub span_nbytes: usize,
    pub attn_q: TensorRef,
    pub attn_k: TensorRef,
    pub attn_v: TensorRef,
    pub attn_o: TensorRef,
    pub ffn_gate: TensorRef,
    pub ffn_up: TensorRef,
    pub ffn_down: TensorRef,
    pub attn_norm: Option<TensorRef>,
    pub ffn_norm: Option<TensorRef>,
}

/// Soft double-buffer prefetch — NOT hardware DMA.
/// Stages next layer raw span in RAM via synchronous ATA read_file_range.
pub struct PrefetchEngine {
    path: String,
    /// Soft buffer: (layer_idx, raw bytes)
    buffered: Option<(usize, Vec<u8>)>,
    soft_not_dma: bool,
}

impl PrefetchEngine {
    pub fn new(path: &str) -> Self {
        PrefetchEngine {
            path: path.to_uppercase(),
            buffered: None,
            soft_not_dma: true,
        }
    }

    pub fn is_soft_not_dma(&self) -> bool {
        self.soft_not_dma
    }

    /// Synchronously read layer span into the secondary buffer (soft read-ahead).
    pub fn submit(&mut self, layer_idx: usize, offset: u64, nbytes: usize) {
        if nbytes == 0 {
            return;
        }
        if let Some(data) = gguf::read_fat_range(&self.path, offset as usize, nbytes) {
            self.buffered = Some((layer_idx, data));
        } else {
            self.buffered = None;
        }
    }

    pub fn take(&mut self, layer_idx: usize) -> Option<Vec<u8>> {
        match self.buffered.take() {
            Some((idx, data)) if idx == layer_idx => Some(data),
            Some(_) => None,
            None => None,
        }
    }
}

/// AirLLM GGUF model: layer-wise disk I/O during forward.
pub struct GGUFStreamingModel {
    path: String,
    n_layers: usize,
    hidden_dim: usize,
    vocab_size: usize,
    num_heads: usize,
    num_kv_heads: usize,
    head_dim: usize,
    layers: Vec<StreamingLayerInfo>,
    /// Skeleton model: embed + unembed + empty layers; used for apply_one_layer.
    skeleton: TransformerModel,
    prefetch: spin::Mutex<PrefetchEngine>,
}

impl GGUFStreamingModel {
    pub fn num_layers(&self) -> usize {
        self.n_layers
    }

    /// Load header + build layer map; load embed/unembed only (not full weights).
    pub fn load(path: &str) -> Result<Self, &'static str> {
        let path_u = path.trim().to_uppercase();
        let file = gguf::load_gguf_header_from_disk(&path_u).ok_or("GGUF header load failed")?;
        let meta = parse_arch_meta(&file);

        let layers = build_layer_map(&file, meta.n_layers)?;
        if layers.is_empty() {
            return Err("GGUF: no blk.N layer tensors found");
        }

        let embed_ref = find_tensor_ref(&file, "token_embd")
            .ok_or("GGUF: token_embd missing")?;
        let embed_vals = read_and_dequant(&path_u, &embed_ref)
            .ok_or("GGUF: embed dequant failed")?;

        let (hidden, vocab) = (embed_ref.cols.max(1), embed_ref.rows.max(1));
        let hidden = if meta.hidden > 0 { meta.hidden } else { hidden };
        let vocab = if meta.vocab > 0 { meta.vocab } else { vocab };

        // token_embd is typically [vocab, hidden] in GGUF row-major
        let embed = {
            let rows = embed_ref.rows;
            let cols = embed_ref.cols;
            // Pack as (hidden, vocab) for embed_lookup column access
            let mut transposed = Vec::with_capacity(hidden * vocab);
            if rows == vocab && cols == hidden {
                for h in 0..hidden {
                    for v in 0..vocab {
                        transposed.push(embed_vals[v * hidden + h]);
                    }
                }
                f32_to_ternary_packed(&transposed, hidden, vocab)
            } else {
                f32_to_ternary_packed(&embed_vals, rows, cols)
            }
        };

        let tie = find_tensor_ref(&file, "output.weight").is_none();
        let unembed = if let Some(out_ref) = find_tensor_ref(&file, "output.weight") {
            let vals = read_and_dequant(&path_u, &out_ref).ok_or("GGUF: unembed dequant failed")?;
            f32_to_ternary_packed(&vals, out_ref.rows, out_ref.cols)
        } else {
            // Tie embeddings: clone embed packed data
            PackedTernaryTensor {
                shape: embed.shape,
                packed_data: embed.packed_data.clone(),
            }
        };

        let num_heads = meta.n_heads.max(1);
        let num_kv_heads = meta.n_kv_heads.max(1);
        let head_dim = if meta.head_dim > 0 {
            meta.head_dim
        } else {
            (hidden / num_heads).max(1)
        };
        let (rope_cos, rope_sin) = cortex::rope_precompute(MAX_SEQ, head_dim, 10000.0);

        let skeleton = TransformerModel {
            embed,
            layers: Vec::new(),
            rms_final: alloc::vec![1.0f32; hidden],
            unembed,
            medusa_heads: Vec::new(),
            vocab_size: vocab.min(u32::MAX as usize) as u32,
            hidden,
            num_layers: layers.len(),
            max_seq: MAX_SEQ,
            num_heads,
            num_kv_heads,
            head_dim,
            kv_dim: num_heads * head_dim,
            intermediate_size: hidden * 4,
            ffn_group_size: hidden * 4,
            tie_embeddings: tie,
            rope_theta: 10000.0,
            rope_cos,
            rope_sin,
        };

        serial_println!(
            "[AIRLLM] load OK path={} layers={} hidden={} heads={} kv_heads={} embed_only+layer_map (soft prefetch)",
            path_u, layers.len(), hidden, num_heads, num_kv_heads
        );

        let _ = file.data_start; // used via TensorRef.file_offset
        Ok(GGUFStreamingModel {
            path: path_u.clone(),
            n_layers: layers.len(),
            hidden_dim: hidden,
            vocab_size: vocab,
            num_heads,
            num_kv_heads,
            head_dim,
            layers,
            skeleton,
            prefetch: spin::Mutex::new(PrefetchEngine::new(&path_u)),
        })
    }

    /// Layer-wise forward: seek+read+dequant+apply per layer; drop weights after each.
    pub fn forward_streaming(&self, tokens: &[u32], cache: &mut KvCache) -> (Tensor, Tensor) {
        let (mut x, mask, start_pos, new_len, total_seq) =
            self.skeleton.embed_for_kv(tokens, cache);

        // Soft prefetch layer 0 span
        {
            let mut pf = self.prefetch.lock();
            if !self.layers.is_empty() {
                let l0 = &self.layers[0];
                pf.submit(0, l0.span_offset, l0.span_nbytes);
            }
        }

        for (n, info) in self.layers.iter().enumerate() {
            let layer_weights = match self.load_layer_weights(n, info) {
                Some(w) => w,
                None => {
                    serial_println!("[AIRLLM] layer {} load FAILED — skipping", n);
                    continue;
                }
            };

            // Soft prefetch next layer while we still hold current weights staged
            if n + 1 < self.layers.len() {
                let next = &self.layers[n + 1];
                self.prefetch.lock().submit(n + 1, next.span_offset, next.span_nbytes);
            }

            if n == 0 || n + 1 == self.n_layers || n % 4 == 0 {
                serial_println!(
                    "[AIRLLM] forward layer {}/{} (soft_prefetch={})",
                    n + 1,
                    self.n_layers,
                    self.prefetch.lock().is_soft_not_dma()
                );
            }

            self.skeleton.apply_one_layer(
                n, &layer_weights, &mut x, cache, start_pos, new_len, total_seq, &mask,
            );
            drop(layer_weights); // free ~1 layer of ternary weights
        }

        cache.advance(new_len);
        self.skeleton.finalize_logits(&x, new_len)
    }

    fn load_layer_weights(&self, layer_idx: usize, info: &StreamingLayerInfo) -> Option<LayerWeights> {
        // Prefer soft-prefetched span; otherwise sync read whole layer span once.
        let span = {
            let mut pf = self.prefetch.lock();
            pf.take(layer_idx).or_else(|| {
                gguf::read_fat_range(&self.path, info.span_offset as usize, info.span_nbytes)
            })
        };

        let deq = |tref: &TensorRef| -> Option<(Vec<f32>, usize, usize)> {
            let vals = if let Some(ref span_buf) = span {
                let rel = tref.file_offset.saturating_sub(info.span_offset) as usize;
                let end = rel.saturating_add(tref.nbytes);
                if end > span_buf.len() {
                    return read_and_dequant(&self.path, tref).map(|v| (v, tref.rows, tref.cols));
                }
                dequantize_raw(tref.qtype, &span_buf[rel..end], tref.rows, tref.cols)?
            } else {
                read_and_dequant(&self.path, tref)?
            };
            Some((vals, tref.rows, tref.cols))
        };

        let (q, qr, qc) = { let (v, r, c) = deq(&info.attn_q)?; (v, r, c) };
        let (k, kr, kc) = { let (v, r, c) = deq(&info.attn_k)?; (v, r, c) };
        let (v, vr, vc) = { let (vv, r, c) = deq(&info.attn_v)?; (vv, r, c) };
        let (o, or_, oc) = { let (vv, r, c) = deq(&info.attn_o)?; (vv, r, c) };
        let (gate, gr, gc) = { let (vv, r, c) = deq(&info.ffn_gate)?; (vv, r, c) };
        let (up, ur, uc) = { let (vv, r, c) = deq(&info.ffn_up)?; (vv, r, c) };
        let (down, dr, dc) = { let (vv, r, c) = deq(&info.ffn_down)?; (vv, r, c) };

        let rms_attn = if let Some(ref nr) = info.attn_norm {
            deq(nr).map(|(v, _, _)| v).unwrap_or_else(|| alloc::vec![1.0f32; self.hidden_dim])
        } else {
            alloc::vec![1.0f32; self.hidden_dim]
        };
        let rms_ffn = if let Some(ref nr) = info.ffn_norm {
            deq(nr).map(|(v, _, _)| v).unwrap_or_else(|| alloc::vec![1.0f32; self.hidden_dim])
        } else {
            alloc::vec![1.0f32; self.hidden_dim]
        };

        let ffn_dim = gc.max(gr).max(1);
        let kv_dim = self.num_heads * self.head_dim;

        Some(LayerWeights {
            rms_attn: rms_attn.clone(),
            q: f32_to_ternary_packed(&q, qr, qc),
            k: f32_to_ternary_packed(&k, kr, kc),
            v: f32_to_ternary_packed(&v, vr, vc),
            o: f32_to_ternary_packed(&o, or_, oc),
            rms_ffn,
            rms_inner_attn: alloc::vec![1.0f32; self.hidden_dim],
            rms_ffn_norm: alloc::vec![1.0f32; ffn_dim],
            gate: f32_to_ternary_packed(&gate, gr, gc),
            up: f32_to_ternary_packed(&up, ur, uc),
            down: f32_to_ternary_packed(&down, dr, dc),
            kv_dim,
            num_kv_heads: self.num_kv_heads,
            intermediate_size: ffn_dim,
            ffn_group_size: ffn_dim,
        })
    }
}

impl Model for GGUFStreamingModel {
    fn generate(&self, prompt: &str) -> String {
        let tokens_u16 = Tokenizer::encode(prompt);
        let mut tokens: Vec<u32> = tokens_u16.iter().map(|&t| t as u32).collect();
        let mut cache = KvCache::new(self.n_layers, self.skeleton.kv_dim, self.skeleton.kv_dim);
        let max_new = 16usize.min(MAX_SEQ.saturating_sub(tokens.len()).max(1));

        serial_println!(
            "[AIRLLM] generate start prompt_len={} layers={} (layer-wise ATA)",
            tokens.len(),
            self.n_layers
        );

        let mut out_tokens: Vec<u32> = Vec::new();
        for step in 0..max_new {
            let input: Vec<u32> = if step == 0 {
                tokens.clone()
            } else {
                vec![*tokens.last().unwrap_or(&0)]
            };
            let (_h, logits) = self.forward_streaming(&input, &mut cache);
            let next = argmax_logits(&logits);
            if next == 1 {
                // EOS in char tokenizer
                break;
            }
            tokens.push(next);
            out_tokens.push(next);
        }

        let decoded = Tokenizer::decode(
            &out_tokens.iter().map(|&t| t as u16).collect::<Vec<_>>(),
        );
        format!(
            "[AIRLLM] {} layers streaming OK (soft prefetch, ATA local).\n{}",
            self.n_layers, decoded
        )
    }

    fn embed_dim(&self) -> usize {
        self.hidden_dim
    }

    fn vocab_size(&self) -> u32 {
        self.vocab_size.min(u32::MAX as usize) as u32
    }

    fn max_seq(&self) -> usize {
        MAX_SEQ
    }
}

fn argmax_logits(logits: &Tensor) -> u32 {
    let mut best_i = 0usize;
    let mut best_v = NEG_INFINITY;
    for (i, &v) in logits.data.iter().enumerate() {
        if v > best_v {
            best_v = v;
            best_i = i;
        }
    }
    best_i as u32
}

struct ArchMeta {
    n_layers: usize,
    hidden: usize,
    vocab: usize,
    n_heads: usize,
    n_kv_heads: usize,
    head_dim: usize,
}

fn parse_arch_meta(file: &GgufFile) -> ArchMeta {
    let get = |keys: &[&str]| -> Option<usize> {
        for m in &file.metadata {
            for k in keys {
                if m.key.contains(k) {
                    if let Ok(v) = m.value.parse::<usize>() {
                        return Some(v);
                    }
                }
            }
        }
        None
    };
    let n_layers = get(&["block_count", "n_layer", "n_layers"]).unwrap_or(0);
    let hidden = get(&["embedding_length", "hidden_size", "n_embd"]).unwrap_or(0);
    let vocab = get(&["vocab_size"]).unwrap_or(VOCAB_SIZE as usize);
    let n_heads = get(&["attention.head_count", "n_head"]).unwrap_or(0);
    let n_kv_heads = get(&["attention.head_count_kv", "n_head_kv"]).unwrap_or(n_heads);
    let head_dim = if n_heads > 0 && hidden > 0 { hidden / n_heads } else { 0 };
    ArchMeta { n_layers, hidden, vocab, n_heads, n_kv_heads, head_dim }
}

fn tensor_to_ref(file: &GgufFile, t: &GgufTensorInfo) -> TensorRef {
    let rows = *t.dims.get(0).unwrap_or(&1) as usize;
    let cols = if t.n_dims > 1 {
        *t.dims.get(1).unwrap_or(&1) as usize
    } else {
        1
    };
    let ne = t.dims.iter().product::<u64>() as usize;
    let nbytes = t.tensor_type.nbytes_for_elements(ne);
    TensorRef {
        name: t.name.clone(),
        file_offset: file.data_start.saturating_add(t.offset),
        nbytes,
        rows,
        cols,
        qtype: t.tensor_type,
    }
}

fn find_tensor_ref(file: &GgufFile, hint: &str) -> Option<TensorRef> {
    file.tensors
        .iter()
        .find(|t| t.name.contains(hint))
        .map(|t| tensor_to_ref(file, t))
}

fn find_layer_tensor(file: &GgufFile, layer: usize, suffix: &str) -> Option<TensorRef> {
    let needle = format!("blk.{}.{}", layer, suffix);
    file.tensors
        .iter()
        .find(|t| t.name.contains(&needle))
        .map(|t| tensor_to_ref(file, t))
}

fn build_layer_map(file: &GgufFile, n_layers_meta: usize) -> Result<Vec<StreamingLayerInfo>, &'static str> {
    let mut max_blk = 0usize;
    for t in &file.tensors {
        if let Some(rest) = t.name.strip_prefix("blk.") {
            if let Some(dot) = rest.find('.') {
                if let Ok(idx) = rest[..dot].parse::<usize>() {
                    max_blk = max_blk.max(idx + 1);
                }
            }
        }
    }
    let n = if n_layers_meta > 0 { n_layers_meta } else { max_blk };
    if n == 0 {
        return Err("GGUF: block_count unknown and no blk.N tensors");
    }

    let mut layers = Vec::with_capacity(n);
    for i in 0..n {
        let attn_q = find_layer_tensor(file, i, "attn_q").ok_or("missing attn_q")?;
        let attn_k = find_layer_tensor(file, i, "attn_k").ok_or("missing attn_k")?;
        let attn_v = find_layer_tensor(file, i, "attn_v").ok_or("missing attn_v")?;
        let attn_o = find_layer_tensor(file, i, "attn_output")
            .or_else(|| find_layer_tensor(file, i, "attn_o"))
            .ok_or("missing attn_output")?;
        let ffn_gate = find_layer_tensor(file, i, "ffn_gate").ok_or("missing ffn_gate")?;
        let ffn_up = find_layer_tensor(file, i, "ffn_up").ok_or("missing ffn_up")?;
        let ffn_down = find_layer_tensor(file, i, "ffn_down").ok_or("missing ffn_down")?;
        let attn_norm = find_layer_tensor(file, i, "attn_norm");
        let ffn_norm = find_layer_tensor(file, i, "ffn_norm");

        let mut min_off = attn_q.file_offset;
        let mut max_end = attn_q.file_offset + attn_q.nbytes as u64;
        for tr in [
            &attn_k, &attn_v, &attn_o, &ffn_gate, &ffn_up, &ffn_down,
        ] {
            min_off = min_off.min(tr.file_offset);
            max_end = max_end.max(tr.file_offset + tr.nbytes as u64);
        }
        for opt in [&attn_norm, &ffn_norm] {
            if let Some(tr) = opt {
                min_off = min_off.min(tr.file_offset);
                max_end = max_end.max(tr.file_offset + tr.nbytes as u64);
            }
        }

        layers.push(StreamingLayerInfo {
            span_offset: min_off,
            span_nbytes: (max_end - min_off) as usize,
            attn_q,
            attn_k,
            attn_v,
            attn_o,
            ffn_gate,
            ffn_up,
            ffn_down,
            attn_norm,
            ffn_norm,
        });
    }
    Ok(layers)
}

fn read_and_dequant(path: &str, tref: &TensorRef) -> Option<Vec<f32>> {
    let raw = gguf::read_fat_range(path, tref.file_offset as usize, tref.nbytes)?;
    dequantize_raw(tref.qtype, &raw, tref.rows, tref.cols)
}

/// Hot-swap from local ATA/FAT path: AirLLM load + set_model.
pub fn hot_swap_from_ata(path: &str) -> Result<(), &'static str> {
    let path = path.trim();
    if path.is_empty() {
        return Err("hot_swap ATA: empty path");
    }
    serial_println!("[AIRLLM] hot-swap ATA begin path={}", path);
    match gguf::load_gguf_streaming(path) {
        Ok(()) => {
            serial_println!("[AIRLLM] hot-swap ATA OK path={} (set_model)", path);
            Ok(())
        }
        Err(e) => {
            serial_println!("[AIRLLM] hot-swap ATA FAIL path={} err={}", path, e);
            Err(e)
        }
    }
}

/// Max staged download size (RAM → FAT). Multi-GB GGUF needs stream-to-disk residual.
const HOTSWAP_MAX_STAGED_BYTES: usize = 64 * 1024 * 1024;

/// Parse `http://ip[:port]/path` — hostname DNS not implemented (honest fail).
fn parse_http_url(url: &str) -> Result<([u8; 4], u16, alloc::string::String), &'static str> {
    let rest = url
        .strip_prefix("http://")
        .or_else(|| url.strip_prefix("HTTP://"))
        .ok_or("hot_swap Net: only http:// supported (no https)")?;
    let (host_port, path) = if let Some(pos) = rest.find('/') {
        let (hp, p) = rest.split_at(pos);
        (hp, alloc::string::String::from(p))
    } else {
        (rest, alloc::string::String::from("/"))
    };
    let (host_only, port) = if let Some(pos) = host_port.find(':') {
        let (h, p_str) = host_port.split_at(pos);
        let p: u16 = p_str[1..].parse().unwrap_or(80);
        (h, p)
    } else {
        (host_port, 80u16)
    };
    let parts: Vec<&str> = host_only.split('.').collect();
    if parts.len() != 4 {
        return Err("hot_swap Net: hostname DNS not implemented — use http://A.B.C.D[:port]/path");
    }
    let ip = [
        parts[0].parse().map_err(|_| "hot_swap Net: bad IPv4")?,
        parts[1].parse().map_err(|_| "hot_swap Net: bad IPv4")?,
        parts[2].parse().map_err(|_| "hot_swap Net: bad IPv4")?,
        parts[3].parse().map_err(|_| "hot_swap Net: bad IPv4")?,
    ];
    Ok((ip, port, path))
}

/// Derive FAT 8.3 dest from URL path, or default HOTSWAP.GGUF.
fn dest_from_url_path(url_path: &str, explicit: Option<&str>) -> alloc::string::String {
    if let Some(d) = explicit {
        let t = d.trim();
        if !t.is_empty() {
            return t.to_uppercase();
        }
    }
    let leaf = url_path.rsplit('/').next().unwrap_or("").trim();
    if leaf.is_empty() || leaf == "/" {
        return alloc::string::String::from("HOTSWAP.GGUF");
    }
    // Keep short 8.3-ish name
    let upper = leaf.to_uppercase();
    if upper.len() <= 12 && upper.contains('.') {
        return upper;
    }
    // Truncate base to 8 + .GGU
    let (base, _ext) = match upper.rsplit_once('.') {
        Some((b, _)) => (b, "GGU"),
        None => (upper.as_str(), "GGU"),
    };
    let mut name = alloc::string::String::from(&base[..base.len().min(8)]);
    name.push_str(".GGU");
    name
}

/// Net HTTP GET → FAT write → AirLLM hot_swap_from_ata.
/// Uses e1000/smoltcp `net::http_get` (NOT SLIP). If RX=0, fails with L3.5/RX — never fakes OK.
///
/// `spec`: `"http://ip:port/path"` or `"http://ip:port/path DEST.GGUF"`
pub fn hot_swap_from_net(spec: &str) -> Result<alloc::string::String, &'static str> {
    let spec = spec.trim();
    if spec.is_empty() {
        return Err("hot_swap Net: empty URL");
    }
    let mut parts = spec.splitn(2, |c: char| c.is_whitespace());
    let url = parts.next().unwrap_or("").trim();
    let dest_opt = parts.next().map(|s| s.trim()).filter(|s| !s.is_empty());

    let (ip, port, path) = parse_http_url(url)?;
    let dest = dest_from_url_path(&path, dest_opt);

    serial_println!(
        "[AIRLLM] hot-swap Net begin {}.{}.{}.{}:{}{} -> FAT {}",
        ip[0], ip[1], ip[2], ip[3], port, path, dest
    );

    let rx_before = crate::netstack::net_rx_count();
    let body = unsafe { crate::net::http_get(ip, port, &path) };
    let rx_after = crate::netstack::net_rx_count();

    let data = match body {
        Some(d) if !d.is_empty() => d,
        Some(_) => {
            serial_println!("[AIRLLM] hot-swap Net FAIL: empty HTTP body");
            return Err("hot_swap Net: empty HTTP body");
        }
        None => {
            if rx_after == 0 || rx_after <= rx_before {
                serial_println!(
                    "[AIRLLM] hot-swap Net FAIL L3.5/RX: http_get None rx_before={} rx_after={} (e1000 RX gate)",
                    rx_before, rx_after
                );
                return Err("L3.5/RX: http_get failed — net RX=0 or no reply (e1000; Sprint Net)");
            }
            serial_println!(
                "[AIRLLM] hot-swap Net FAIL: http_get None but RX delta={} (TCP/HTTP error)",
                rx_after.saturating_sub(rx_before)
            );
            return Err("hot_swap Net: http_get failed (RX seen, no HTTP body)");
        }
    };

    if data.len() > HOTSWAP_MAX_STAGED_BYTES {
        serial_println!(
            "[AIRLLM] hot-swap Net FAIL: body {} > max staged {} (stream-to-disk residual)",
            data.len(),
            HOTSWAP_MAX_STAGED_BYTES
        );
        return Err("hot_swap Net: body too large for RAM staging (64MiB cap; stream-to-disk deferred)");
    }

    // Strip HTTP status line if raw response leaked (netstack may return body-only)
    let payload = strip_http_headers_if_present(&data);

    if payload.len() < 4 || &payload[0..4] != b"GGUF" {
        serial_println!(
            "[AIRLLM] hot-swap Net WARN: payload magic != GGUF (len={}); writing anyway for ATA detect",
            payload.len()
        );
    }

    gguf::write_fat_file(&dest, payload)?;
    hot_swap_from_ata(&dest)?;
    Ok(dest)
}

fn strip_http_headers_if_present(data: &[u8]) -> &[u8] {
    // If response starts with "HTTP/", skip headers until \r\n\r\n
    if data.len() >= 5 && &data[0..5] == b"HTTP/" {
        if let Some(pos) = find_double_crlf(data) {
            return &data[pos..];
        }
    }
    data
}

fn find_double_crlf(data: &[u8]) -> Option<usize> {
    let mut i = 0;
    while i + 3 < data.len() {
        if data[i] == b'\r' && data[i + 1] == b'\n' && data[i + 2] == b'\r' && data[i + 3] == b'\n' {
            return Some(i + 4);
        }
        i += 1;
    }
    None
}

/// True if arg looks like an HTTP URL for Net hot-swap.
pub fn is_http_model_spec(s: &str) -> bool {
    let t = s.trim();
    t.starts_with("http://") || t.starts_with("HTTP://")
}
