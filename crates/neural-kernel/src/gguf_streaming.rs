// E1a: gguf_streaming stays in bin (net-dependent). Crate re-export removed.
// ADR-0046 item 8: hot-swap AirLLM real (ATA + Net) — conecta os callers do
// /model aos helpers reais de cortex::gguf (header-only streaming + stream-to-disk).

/// True if path looks like http://host:port/path
pub fn is_http_model_spec(path: &str) -> bool {
    path.starts_with("http://") || path.starts_with("https://") || path.starts_with("tcp://")
}

/// Net hot-swap: GET Range em chunks (4MB) → append FAT (stream-to-disk) →
/// load header-only (AirLLM). ponytail: sem DMA/parallel — soft double-buffer,
/// honesto no log. Retorna o nome FAT destino.
pub fn hot_swap_from_net(path: &str) -> Result<alloc::string::String, alloc::string::String> {
    // Destino FAT 8.3: extrai do path (ou default DEST.GGUF)
    let dest = alloc::string::String::from("DEST.GGUF");
    let (host, port, url_path) = crate::net::parse_http_url(path)
        .map_err(|e| alloc::format!("url invalida: {}", e))?;
    // Resolve host → IP (mesmo padrão do https_get)
    let ip = unsafe { crate::net::dns_resolve_host(&host) }.ok_or("dns_failed")?;

    // Descobre o total via Range: bytes=0-0 (206 → Content-Range total)
    let probe = unsafe { crate::net::http_get_range_host(ip, port, &url_path, None, 0, 0) }
        .ok_or("Range probe falhou (RX?)")?;
    let total = probe.total.ok_or("server sem Content-Range")?;
    if total < 1024 * 1024 {
        // pequeno — baixa inteiro de uma vez
        let body = unsafe { crate::net::http_get_host(ip, port, &url_path, None) }
            .ok_or("GET falhou")?;
        crate::gguf::write_fat_file(&dest, &body).map_err(|e| alloc::format!("FAT write: {}", e))?;
        crate::gguf::load_gguf_streaming(&dest).map_err(|e| alloc::format!("GGUF header: {}", e))?;
        return Ok(dest);
    }

    // Stream-to-disk em chunks de 4MB
    const CHUNK: usize = 4 * 1024 * 1024;
    let mut off = 0usize;
    let mut total_written = 0usize;
    let mut first = true;
    while off < total {
        let end = (off + CHUNK - 1).min(total - 1);
        let body = unsafe { crate::net::http_get_range_host(ip, port, &url_path, None, off, end) }
            .ok_or("Range GET falhou")?
            .body;
        if first {
            crate::gguf::write_fat_file(&dest, &body)
                .map_err(|e| alloc::format!("FAT write: {}", e))?;
            first = false;
        } else {
            crate::gguf::append_fat_file(&dest, &body)
                .map_err(|e| alloc::format!("FAT append: {}", e))?;
        }
        total_written += body.len();
        off += body.len();
        if body.is_empty() {
            break; // server parou de mandar
        }
    }
    if total_written < total {
        return Err(alloc::format!(
            "Range truncado wrote={}/{} — refuse load parcial",
            total_written,
            total
        ));
    }
    crate::gguf::load_gguf_streaming(&dest).map_err(|e| alloc::format!("GGUF header: {}", e))?;
    k_nano::slog_bin!(
        "GGUF",
        "ok",
        "Net stream-to-disk OK dest={} bytes={}/{}",
        dest,
        total_written,
        total
    );
    Ok(dest)
}

/// ATA hot-swap: load header-only do FAT (AirLLM).
pub fn hot_swap_from_ata(path: &str) -> Result<(), alloc::string::String> {
    crate::gguf::load_gguf_streaming(path).map_err(|e| alloc::format!("GGUF header: {}", e))
}

pub fn log_airllm_residuals() {
    k_nano::slog_bin!(
        "GGUF",
        "warn",
        "AirLLM residuals: hot_swap_from_ata/net APIs exist (net=0 callers); generate=STUB load-only; K-quants wired in gguf; DMA prefetch=AWAITING"
    );
}

// ---------------------------------------------------------------------------
// AirLLM forward_streaming: loop layer-wise que carrega pesos do disco sob demanda
// ---------------------------------------------------------------------------

use alloc::vec;
use alloc::vec::Vec;
use cortex_crate::gguf::{self, GgufFile, GgufTensorInfo};

/// Contexto de streaming — header GGUF + config derivado dos metadados.
/// Mantém apenas o necessário para reconstruir LayerWeights camada-a-camada.
pub struct StreamingCtx {
    pub file: GgufFile,
    pub path: alloc::string::String,
    pub n_layers: usize,
    pub hidden: usize,
    pub kv_dim: usize,
    pub num_heads: usize,
    pub num_kv_heads: usize,
    pub intermediate: usize,
    pub vocab_size: usize,
    pub rope_theta: f64,
}

impl StreamingCtx {
    /// Carrega header GGUF do FAT e deriva config dos metadados.
    pub fn from_fat(path: &str) -> Result<Self, &'static str> {
        let file = cortex_crate::gguf::load_gguf_header_from_disk(path).ok_or("GGUF header fail")?;
        let meta = |key: &str| -> Option<usize> {
            file.metadata.iter().find(|m| m.key.contains(key))
                .and_then(|m| m.value.parse().ok())
        };
        // Honesty: sem inventar arch (antes unwrap_or 4L/64h/32k mentia GGUF incompleto).
        let n_layers = meta("block_count")
            .or_else(|| meta("num_hidden_layers"))
            .ok_or("GGUF: missing block_count/num_hidden_layers")?;
        let hidden = meta("hidden_size")
            .or_else(|| meta("embed_dim"))
            .ok_or("GGUF: missing hidden_size/embed_dim")?;
        let num_heads = meta("num_attention_heads")
            .or_else(|| meta("num_heads"))
            .ok_or("GGUF: missing num_attention_heads")?;
        let num_kv_heads = meta("num_key_value_heads").unwrap_or(num_heads);
        let kv_dim = hidden;
        let intermediate = meta("intermediate_size")
            .or_else(|| meta("ffn_dim"))
            .unwrap_or(hidden.saturating_mul(4));
        let vocab_size = meta("vocab_size").ok_or("GGUF: missing vocab_size")?;
        let rope_theta = file
            .metadata
            .iter()
            .find(|m| m.key.contains("rope_theta"))
            .and_then(|m| m.value.parse().ok())
            .unwrap_or(10000.0);
        Ok(StreamingCtx {
            file, path: alloc::string::String::from(path), n_layers, hidden, kv_dim,
            num_heads, num_kv_heads, intermediate, vocab_size, rope_theta,
        })
    }

    /// Carrega um tensor generico pelo nome (para embed/unembed).
    pub fn load_tensor_data(&self, name: &str) -> Option<Vec<f32>> {
        let t = self.file.tensors.iter().find(|t| t.name == name)?;
        let nbytes = t.tensor_type.nbytes_for_elements(
            t.dims.iter().product::<u64>() as usize);
        let offset = (self.file.data_start + t.offset) as usize;
        let data = unsafe {
            k_nano::fat32::read_file_range_by_name(&self.path, offset, nbytes)?
        };
        gguf::dequantize_raw(t.tensor_type, &data, t.dims[0] as usize,
            if t.n_dims > 1 { t.dims[1] as usize } else { 1 })
    }

    /// Carrega os pesos de uma camada do disco (FAT) e dequantiza.
    /// Lê cada tensor pelo nome (blk.{i}.attn_q, etc.) no offset do GGUF.
    pub fn load_layer(&self, layer_idx: usize) -> Option<cortex_crate::cortex::LayerWeights> {
        let find = |name: &str| -> Option<&GgufTensorInfo> {
            self.file.tensors.iter().find(|t| t.name == name)
        };
        let load_tensor = |name: &str| -> Option<Vec<f32>> {
            let t = find(name)?;
            let nbytes = t.tensor_type.nbytes_for_elements(
                t.dims.iter().product::<u64>() as usize);
            let offset = (self.file.data_start + t.offset) as usize;
            let data = unsafe {
                k_nano::fat32::read_file_range_by_name(&self.path, offset, nbytes)?
            };
            gguf::dequantize_raw(t.tensor_type, &data, t.dims[0] as usize,
                if t.n_dims > 1 { t.dims[1] as usize } else { 1 }).map(|v| v)
        };

        let to_ternary = |v: Vec<f32>| {
            gguf::f32_to_ternary_packed(&v, 1, v.len())
        };

        let q = load_tensor(&alloc::format!("blk.{}.attn_q.weight", layer_idx))?;
        let k = load_tensor(&alloc::format!("blk.{}.attn_k.weight", layer_idx))?;
        let v = load_tensor(&alloc::format!("blk.{}.attn_v.weight", layer_idx))?;
        let o = load_tensor(&alloc::format!("blk.{}.attn_output.weight", layer_idx))?;
        let gate = load_tensor(&alloc::format!("blk.{}.ffn_gate.weight", layer_idx))?;
        let up = load_tensor(&alloc::format!("blk.{}.ffn_up.weight", layer_idx))?;
        let down = load_tensor(&alloc::format!("blk.{}.ffn_down.weight", layer_idx))?;

        let rms_default = vec![1.0f32; self.hidden];
        Some(cortex_crate::cortex::LayerWeights {
            rms_attn: rms_default.clone(),
            q: to_ternary(q), q_scale: 1.0,
            k: to_ternary(k), k_scale: 1.0,
            v: to_ternary(v), v_scale: 1.0,
            o: to_ternary(o), o_scale: 1.0,
            rms_ffn: rms_default.clone(),
            rms_inner_attn: rms_default.clone(),
            rms_ffn_norm: vec![1.0f32; self.intermediate],
            gate: to_ternary(gate), gate_scale: 1.0,
            up: to_ternary(up), up_scale: 1.0,
            down: to_ternary(down), down_scale: 1.0,
            kv_dim: self.kv_dim,
            num_kv_heads: self.num_kv_heads,
            intermediate_size: self.intermediate,
            ffn_group_size: self.intermediate,
        })
    }
}

/// Stub: forward_streaming completo requer KvCache + rope tables + loop de tokens.
/// Versão mínima que demonstra o carregamento camada-por-camada (AirLLM core).
/// Retorna o número de camadas carregadas com sucesso (prova de conceito).
pub fn forward_streaming_demo(path: &str) -> Result<usize, &'static str> {
    let ctx = StreamingCtx::from_fat(path)?;
    let mut loaded = 0usize;
    for i in 0..ctx.n_layers {
        if ctx.load_layer(i).is_some() {
            loaded += 1;
        } else {
            break;
        }
    }
    if loaded < ctx.n_layers {
        k_nano::slog_bin!(
            "GGUF",
            "warn",
            "forward_streaming demo PARTIAL: {}/{} camadas (não é forward)",
            loaded,
            ctx.n_layers
        );
        return Err("forward_streaming: loaded < n_layers");
    }
    k_nano::slog_bin!(
        "GGUF",
        "ok",
        "forward_streaming demo: {}/{} camadas carregadas (load-only, sem attn)",
        loaded,
        ctx.n_layers
    );
    Ok(loaded)
}

// ---------------------------------------------------------------------------
// AirLLM StreamingModel — implementa Model trait com forward layer-by-layer
// ---------------------------------------------------------------------------

use alloc::string::String;
use cortex_crate::cortex::{KvCache, Model};

/// Streaming Model: mantem APENAS header GGUF + config + embeddings em RAM.
/// Pesos dos layers carregados do disco (FAT/ATA) sob demanda, 1 layer/forward.
/// RAM necessaria = 1 layer + KV cache + embeddings (nao o modelo inteiro).
pub struct StreamingModel {
    ctx: StreamingCtx,
    cache: KvCache,
    embed_weights: Vec<f32>,
    unembed_weights: Vec<f32>,
    max_seq: usize,
}

impl StreamingModel {
    /// Cria StreamingModel a partir de GGUF no FAT.
    /// Carrega header + embeddings + unembed (permanentemente em RAM).
    pub fn from_fat(path: &str) -> Result<Self, &'static str> {
        let ctx = StreamingCtx::from_fat(path)?;
        let kv_cache = KvCache::new(ctx.n_layers, ctx.kv_dim, ctx.kv_dim);

        // Carrega embedding weights (permanente — ~vocab*hidden floats)
        let embed_weights = ctx
            .load_tensor_data("token_embd.weight")
            .or_else(|| ctx.load_tensor_data("token_embd"))
            .ok_or("AirLLM: embed weights missing — refuse zeros")?;

        // Carrega unembed weights (permanente — tied com embed se ausente)
        let unembed_weights = ctx
            .load_tensor_data("output.weight")
            .unwrap_or_else(|| embed_weights.clone()); // tied

        k_nano::slog_bin!(
            "GGUF",
            "ok",
            "AirLLM StreamingModel: layers={} hidden={} vocab={} embed={}KB unembed={}KB path={}",
            ctx.n_layers,
            ctx.hidden,
            ctx.vocab_size,
            embed_weights.len() * 4 / 1024,
            unembed_weights.len() * 4 / 1024,
            path
        );

        Ok(StreamingModel {
            ctx, cache: kv_cache,
            embed_weights, unembed_weights,
            max_seq: 2048,
        })
    }

    /// Lookup de embedding (token -> hidden vector)
    fn embed_lookup(&self, token: u32) -> Vec<f32> {
        let t = (token as usize).min(self.ctx.vocab_size.saturating_sub(1));
        let h = self.ctx.hidden;
        let start = t * h;
        if start + h <= self.embed_weights.len() {
            self.embed_weights[start..start + h].to_vec()
        } else {
            vec![0.0f32; h]
        }
    }
}

impl Model for StreamingModel {
    fn generate(&self, prompt: &str) -> String {
        // Honesty (s359): loop antigo carregava layers e unembedia x cru sem attn/FFN —
        // inventava tokens. Até forward AirLLM real: recusa explícita.
        let _ = prompt;
        k_nano::slog_bin!(
            "GGUF",
            "warn",
            "AirLLM generate STUB (layers load-only; attn/FFN não wired) — refuse inventar tokens"
        );
        alloc::string::String::from(
            "[AirLLM stub: layer streaming load-only; full forward not wired]",
        )
    }
    fn embed_dim(&self) -> usize { self.ctx.hidden }
    fn vocab_size(&self) -> u32 { self.ctx.vocab_size as u32 }
    fn max_seq(&self) -> usize { self.max_seq }
    fn num_layers(&self) -> usize { self.ctx.n_layers }
    fn hidden(&self) -> usize { self.ctx.hidden }
}
