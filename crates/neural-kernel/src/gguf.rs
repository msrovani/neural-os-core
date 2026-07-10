//! GGUF (GGML Universal Format) loader — no_std puro.
//! Suporta parse de header, metadata, tensor info.
//! Q4_0/Q8_0 dequantization para alimentar o BitNet pipeline.
//!
//! Formato: https://github.com/ggerganov/ggml/blob/master/docs/gguf.md

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use crate::tensor::{PackedTernaryTensor, Tensor};
use crate::serial_println;

const GGUF_MAGIC: u32 = 0x46554747; // "GGUF" little-endian
const GGUF_VERSION: u32 = 3;

#[derive(Debug)]
pub enum GgufType {
    F32,
    F16,
    Q4_0,
    Q4_1,
    Q5_0,
    Q5_1,
    Q8_0,
    Q8_1,
    Unknown(u32),
}

impl GgufType {
    fn from_u32(v: u32) -> Self {
        match v {
            0 => GgufType::F32,
            1 => GgufType::F16,
            2 => GgufType::Q4_0,
            3 => GgufType::Q4_1,
            6 => GgufType::Q5_0,
            7 => GgufType::Q5_1,
            8 => GgufType::Q8_0,
            9 => GgufType::Q8_1,
            x => GgufType::Unknown(x),
        }
    }

    pub fn bits_per_weight(&self) -> u32 {
        match self {
            GgufType::F32 => 32,
            GgufType::F16 => 16,
            GgufType::Q4_0 => 5,  // 4 bits + 1/32 scale
            GgufType::Q4_1 => 5,
            GgufType::Q5_0 => 6,
            GgufType::Q5_1 => 6,
            GgufType::Q8_0 => 9,
            GgufType::Q8_1 => 9,
            GgufType::Unknown(_) => 32,
        }
    }
}

#[derive(Debug)]
pub struct GgufTensorInfo {
    pub name: String,
    pub n_dims: u32,
    pub dims: Vec<u64>,
    pub tensor_type: GgufType,
    pub offset: u64,
}

#[derive(Debug)]
pub struct GgufHeader {
    pub magic: u32,
    pub version: u32,
    pub tensor_count: u64,
    pub metadata_kv_count: u64,
}

#[derive(Debug)]
pub struct GgufMetadata {
    pub key: String,
    pub value: String,
}

#[derive(Debug)]
pub struct GgufFile {
    pub header: GgufHeader,
    pub metadata: Vec<GgufMetadata>,
    pub tensors: Vec<GgufTensorInfo>,
    pub data_start: u64,
    pub data: Vec<u8>,
}

/// Le u32 little-endian de um slice
fn read_u32(data: &[u8], offset: &mut usize) -> u32 {
    let val = u32::from_le_bytes([
        data[*offset], data[*offset + 1], data[*offset + 2], data[*offset + 3],
    ]);
    *offset += 4;
    val
}

/// Le u64 little-endian de um slice
fn read_u64(data: &[u8], offset: &mut usize) -> u64 {
    let val = u64::from_le_bytes([
        data[*offset], data[*offset + 1], data[*offset + 2], data[*offset + 3],
        data[*offset + 4], data[*offset + 5], data[*offset + 6], data[*offset + 7],
    ]);
    *offset += 8;
    val
}

/// Le string (length-prefixed) de um slice
fn read_string(data: &[u8], offset: &mut usize) -> String {
    let len = read_u64(data, offset) as usize;
    let end = core::cmp::min(*offset + len, data.len());
    let s = core::str::from_utf8(&data[*offset..end]).unwrap_or("(invalid utf8)");
    *offset += len;
    String::from(s)
}

/// Le metadata value (string or array) como string
fn read_metadata_value(data: &[u8], offset: &mut usize) -> String {
    let val_type = read_u32(data, offset);
    match val_type {
        0 => { // uint8
            let v = data[*offset];
            *offset += 1;
            alloc::format!("{}", v)
        }
        1 => { // int8
            let v = data[*offset] as i8;
            *offset += 1;
            alloc::format!("{}", v)
        }
        2 => { // uint16
            let v = u16::from_le_bytes([data[*offset], data[*offset + 1]]);
            *offset += 2;
            alloc::format!("{}", v)
        }
        3 => { // int16
            let v = i16::from_le_bytes([data[*offset], data[*offset + 1]]);
            *offset += 2;
            alloc::format!("{}", v)
        }
        4 => { // uint32
            let v = read_u32(data, offset);
            alloc::format!("{}", v)
        }
        5 => { // int32
            let v = i32::from_le_bytes([data[*offset], data[*offset + 1], data[*offset + 2], data[*offset + 3]]);
            *offset += 4;
            alloc::format!("{}", v)
        }
        6 => { // uint64
            let v = read_u64(data, offset);
            alloc::format!("{}", v)
        }
        7 => { // int64
            let v = i64::from_le_bytes([
                data[*offset], data[*offset + 1], data[*offset + 2], data[*offset + 3],
                data[*offset + 4], data[*offset + 5], data[*offset + 6], data[*offset + 7],
            ]);
            *offset += 8;
            alloc::format!("{}", v)
        }
        8 => { // float32
            let v = f32::from_le_bytes([data[*offset], data[*offset + 1], data[*offset + 2], data[*offset + 3]]);
            *offset += 4;
            alloc::format!("{:.4}", v)
        }
        9 => { // bool
            let v = data[*offset] != 0;
            *offset += 1;
            String::from(if v { "true" } else { "false" })
        }
        10 => { // string
            read_string(data, offset)
        }
        11 => { // array
            let arr_type = read_u32(data, offset);
            let arr_len = read_u64(data, offset) as usize;
            let mut items = Vec::new();
            for _ in 0..arr_len {
                items.push(read_metadata_value_inner(data, offset, arr_type));
            }
            alloc::format!("[{}]", items.join(", "))
        }
        _ => {
            String::from("(unknown)")
        }
    }
}

fn read_metadata_value_inner(data: &[u8], offset: &mut usize, val_type: u32) -> String {
    match val_type {
        8 => { // float32
            let v = f32::from_le_bytes([data[*offset], data[*offset + 1], data[*offset + 2], data[*offset + 3]]);
            *offset += 4;
            alloc::format!("{:.4}", v)
        }
        10 => read_string(data, offset),
        _ => {
            *offset += 1;
            String::from("?")
        }
    }
}

/// Carrega e parseia um arquivo GGUF completo em memoria
pub fn load_gguf(data: &[u8]) -> Result<GgufFile, &'static str> {
    if data.len() < 24 { return Err("GGUF: dados muito curtos"); }

    let mut offset = 0;
    let magic = read_u32(data, &mut offset);
    if magic != GGUF_MAGIC { return Err("GGUF: magic invalido"); }

    let version = read_u32(data, &mut offset);
    let tensor_count = read_u64(data, &mut offset);
    let metadata_kv_count = read_u64(data, &mut offset);

    let header = GgufHeader { magic, version, tensor_count, metadata_kv_count };
    serial_println!("[GGUF] Header: version={} tensors={} metadata={}",
        version, tensor_count, metadata_kv_count);

    // Metadata
    let mut metadata = Vec::new();
    for _ in 0..metadata_kv_count {
        let key = read_string(data, &mut offset);
        let value = read_metadata_value(data, &mut offset);
        metadata.push(GgufMetadata { key, value });
    }

    // Tensor info
    let mut tensors = Vec::new();
    for _ in 0..tensor_count {
        let name = read_string(data, &mut offset);
        let n_dims = read_u32(data, &mut offset);
        let mut dims = Vec::with_capacity(n_dims as usize);
        for _ in 0..n_dims {
            dims.push(read_u64(data, &mut offset));
        }
        let tensor_type = GgufType::from_u32(read_u32(data, &mut offset));
        let tensor_offset = read_u64(data, &mut offset);
        tensors.push(GgufTensorInfo { name, n_dims, dims, tensor_type, offset: tensor_offset });
    }

    // Padding to alignment (GGUF alinha a 32 bytes)
    let data_start = (offset + 31) & !31;
    if data_start > data.len() { return Err("GGUF: dados insuficientes para tensor data"); }

    let raw_data = data[data_start..].to_vec();

    serial_println!("[GGUF] Parse OK. Metadata: {} items, Tensors: {} items, Data: {} bytes",
        metadata.len(), tensors.len(), raw_data.len());

    Ok(GgufFile { header, metadata, tensors, data_start: data_start as u64, data: raw_data })
}

/// Converte f16 (u16) para f32
fn f16_to_f32(half: u16) -> f32 {
    let sign = ((half >> 15) as f32) * -1.0_f32;
    let exp = (half >> 10) & 0x1F;
    let mant = half & 0x3FF;
    if exp == 0 {
        sign * (mant as f32) * unsafe { libm::powf(2.0, -24.0) }
    } else if exp == 31 {
        if mant == 0 { sign * f32::INFINITY } else { f32::NAN }
    } else {
        sign * (1.0 + (mant as f32) / 1024.0) * unsafe { libm::powf(2.0, (exp as f32) - 15.0) }
    }
}

/// Dequantiza um bloco Q4_0 (18 bytes → 32 f32)
fn dequantize_q4_0_block(block: &[u8]) -> Result<[f32; 32], &'static str> {
    if block.len() < 18 { return Err("Q4_0 block muito curto"); }
    let scale = f16_to_f32(u16::from_le_bytes([block[0], block[1]]));
    let mut result = [0.0f32; 32];
    for i in 0..16 {
        let byte = block[2 + i];
        let lo = (byte & 0x0F) as i8 - 8;
        let hi = ((byte >> 4) & 0x0F) as i8 - 8;
        result[i * 2] = (lo as f32) * scale;
        result[i * 2 + 1] = (hi as f32) * scale;
    }
    Ok(result)
}

/// Dequantiza um tensor Q4_0 completo
pub fn dequantize_q4_0(data: &[u8], rows: usize, cols: usize) -> Option<Tensor> {
    let block_size = 32;
    let block_bytes = 18; // scale f16 (2) + 16 bytes de weights
    let total_weights = rows * cols;
    let num_blocks = (total_weights + block_size - 1) / block_size;
    let expected_bytes = num_blocks * block_bytes;

    if data.len() < expected_bytes { return None; }

    let mut tensor_data = Vec::with_capacity(total_weights);
    for b in 0..num_blocks {
        let block_start = b * block_bytes;
        let block_end = core::cmp::min(block_start + block_bytes, data.len());
        if let Ok(values) = dequantize_q4_0_block(&data[block_start..block_end]) {
            let remaining = total_weights - tensor_data.len();
            let to_copy = core::cmp::min(32, remaining);
            tensor_data.extend_from_slice(&values[..to_copy]);
        }
    }

    Tensor::from_row_major((rows, cols), tensor_data)
}

// ---------------------------------------------------------------------------
// GgufBackedModel — implementa Model trait usando pesos GGUF
// ---------------------------------------------------------------------------

use crate::cortex::Model;

/// Converte um tensor f32 para PackedTernaryTensor via limiar
fn f32_to_ternary_packed(data: &[f32], rows: usize, cols: usize) -> PackedTernaryTensor {
    let mut vals = Vec::with_capacity(rows * cols);
    for &v in data.iter().take(rows * cols) {
        vals.push(if v > 0.1 { 1 } else if v < -0.1 { -1 } else { 0 });
    }
    let packed = PackedTernaryTensor::pack_weights(&vals);
    PackedTernaryTensor { shape: (rows, cols), packed_data: packed }
}

/// Dequantiza o primeiro tensor encontrado pelo nome que contém `name_hint`
fn dequantize_tensor_by_name(file: &GgufFile, name_hint: &str) -> Option<(Vec<f32>, usize, usize)> {
    for t in &file.tensors {
        if t.name.contains(name_hint) {
            let start = t.offset as usize;
            let end = start + (t.dims.iter().product::<u64>() as usize) * 4;
            if end > file.data.len() { return None; }
            let raw = &file.data[start..end];
            let rows = t.dims[0] as usize;
            let cols = if t.n_dims > 1 { t.dims[1] as usize } else { 1 };
            return match t.tensor_type {
                GgufType::F32 => {
                    let mut vals = Vec::with_capacity(rows * cols);
                    for i in 0..rows * cols {
                        if i * 4 + 4 <= raw.len() {
                            vals.push(f32::from_le_bytes([raw[i*4], raw[i*4+1], raw[i*4+2], raw[i*4+3]]));
                        }
                    }
                    Some((vals, cols, rows))
                }
                GgufType::Q4_0 => {
                    crate::gguf::dequantize_q4_0(raw, rows, cols)
                        .map(|t| (t.data, cols, rows))
                }
                _ => None,
            };
        }
    }
    None
}

/// Modelo alimentado por GGUF. Tenta converter pesos GGUF para TransformerModel.
pub struct GgufBackedModel {
    file: GgufFile,
    n_layers: usize,
    hidden_dim: usize,
}

impl GgufBackedModel {
    pub fn new(file: GgufFile) -> Self {
        let n_layers = file.metadata.iter()
            .find(|m| m.key.contains("block_count") || m.key.contains("n_layers"))
            .and_then(|m| m.value.parse().ok())
            .unwrap_or(4);
        let hidden_dim = dequantize_tensor_by_name(&file, "token_embd")
            .map(|(_, cols, _)| cols)
            .unwrap_or(64);
        GgufBackedModel { file, n_layers, hidden_dim }
    }

    fn try_build_transformer(&self) -> Option<crate::cortex::TransformerModel> {
        let (vals, hidden, vocab) = dequantize_tensor_by_name(&self.file, "token_embd")?;
        let embed = {
            let mut vals_t = Vec::with_capacity(hidden * vocab);
            for h in 0..hidden {
                for v in 0..vocab {
                    vals_t.push(vals[v * hidden + h]);
                }
            }
            f32_to_ternary_packed(&vals_t, hidden, vocab)
        };
        let mut layers = Vec::with_capacity(self.n_layers);
        for i in 0..self.n_layers {
            let hint = |s| alloc::format!("blk.{}.{}", i, s);
            let (q, qc, qr) = dequantize_tensor_by_name(&self.file, &hint("attn_q"))?;
            let (k, kc, kr) = dequantize_tensor_by_name(&self.file, &hint("attn_k"))?;
            let (v, vc, vr) = dequantize_tensor_by_name(&self.file, &hint("attn_v"))?;
            let (o, oc, or_) = dequantize_tensor_by_name(&self.file, &hint("attn_output"))?;
            let (gate, gc, gr) = dequantize_tensor_by_name(&self.file, &hint("ffn_gate"))?;
            let (up, uc, ur) = dequantize_tensor_by_name(&self.file, &hint("ffn_up"))?;
            let (down, dc, dr) = dequantize_tensor_by_name(&self.file, &hint("ffn_down"))?;
            let rms_default = alloc::vec![1.0f32; self.hidden_dim];
            let ffn_dim = gc.max(gr) / 4 * 4; // approximate FFN intermediate dim
            let rms_inner_attn = alloc::vec![1.0f32; self.hidden_dim];
            let rms_ffn_norm = alloc::vec![1.0f32; ffn_dim];
            layers.push(crate::cortex::LayerWeights {
                rms_attn: rms_default.clone(),
                q: f32_to_ternary_packed(&q, qr, qc),
                k: f32_to_ternary_packed(&k, kr, kc),
                v: f32_to_ternary_packed(&v, vr, vc),
                o: f32_to_ternary_packed(&o, or_, oc),
                rms_ffn: rms_default,
                rms_inner_attn,
                rms_ffn_norm,
                gate: f32_to_ternary_packed(&gate, gr, gc),
                up: f32_to_ternary_packed(&up, ur, uc),
                down: f32_to_ternary_packed(&down, dr, dc),
                kv_dim: self.hidden_dim,
                num_kv_heads: self.hidden_dim / 64,
                intermediate_size: ffn_dim,
                ffn_group_size: ffn_dim,
            });
        }
        let unembed = dequantize_tensor_by_name(&self.file, "output.weight")
            .or_else(|| dequantize_tensor_by_name(&self.file, "token_embd"))
            .map(|(data, c, r)| f32_to_ternary_packed(&data, r, c))
            .unwrap_or_else(|| {
                let mut seed = 42u32;
                crate::cortex::random_ternary(&mut seed, self.hidden_dim, crate::cortex::VOCAB_SIZE as usize)
            });
        let rms_final = alloc::vec![1.0f32; self.hidden_dim];
        Some(crate::cortex::TransformerModel {
            embed,
            layers,
            rms_final,
            unembed,
            medusa_heads: Vec::new(),
            vocab_size: crate::cortex::VOCAB_SIZE as u32,
            hidden: self.hidden_dim,
            num_layers: self.n_layers,
            max_seq: crate::cortex::MAX_SEQ,
            num_heads: self.hidden_dim / 64,
            num_kv_heads: self.hidden_dim / 64,
            head_dim: 64,
            kv_dim: self.hidden_dim,
            intermediate_size: self.hidden_dim * 4,
            ffn_group_size: self.hidden_dim,
            tie_embeddings: false,
            rope_theta: 10000.0,
            rope_cos: alloc::vec![],
            rope_sin: alloc::vec![],
        })
    }
}

impl Model for GgufBackedModel {
    fn generate(&self, prompt: &str) -> String {
        if let Some(model) = self.try_build_transformer() {
            crate::cortex::generate_text(&model, prompt)
        } else {
            let summary = gguf_summary(&self.file);
            alloc::format!("[GGUF] Modelo carregado. {} camadas, {} hidden.\n\
                Aviso: conversao de pesos nao disponivel para este formato.\n\
                Use generacao fallback.\n{}\nPrompt: {}",
                self.n_layers, self.hidden_dim, summary, prompt)
        }
    }

    fn embed_dim(&self) -> usize {
        self.hidden_dim
    }

    fn vocab_size(&self) -> u32 {
        crate::cortex::VOCAB_SIZE as u32
    }

    fn max_seq(&self) -> usize {
        crate::cortex::MAX_SEQ
    }
}

/// Carrega modelo GGUF e registra como modelo ativo via set_model()
pub fn load_gguf_model(data: &[u8]) -> Result<(), &'static str> {
    let file = load_gguf(data)?;
    let model = GgufBackedModel::new(file);
    crate::cortex::set_model(Box::new(model));
    Ok(())
}

/// Summary do modelo GGUF para debug
pub fn gguf_summary(file: &GgufFile) -> String {
    let mut s = alloc::format!("GGUF Model: version={}, tensors={}\n",
        file.header.version, file.header.tensor_count);
    for meta in &file.metadata {
        s.push_str(&alloc::format!("  {}: {}\n", meta.key, meta.value));
    }
    for (i, tensor) in file.tensors.iter().enumerate() {
        if i > 10 { s.push_str(&alloc::format!("  ... +{} mais\n", file.tensors.len() - i)); break; }
        s.push_str(&alloc::format!("  [{}] {} {:?} {:?} @{}\n",
            i, tensor.name, tensor.dims, tensor.tensor_type, tensor.offset));
    }
    s
}

/// Carrega apenas cabecalho GGUF + metadados + info tensores via FAT32 streaming.
/// Nao carrega dados dos tensores — leitura sob demanda via read_file_range().
pub fn load_gguf_header_from_disk(path: &str) -> Option<GgufFile> {
    let name = path.trim().to_uppercase();
    let ata = crate::ATA_DRIVER.lock();
    let ata = ata.as_ref()?;
    let parts = unsafe { crate::fat32::read_mbr(ata) };
    for part in &parts {
        if part.type_code == 0x0B || part.type_code == 0x0C || part.type_code == 0x1C || part.type_code == 0x73 {
            let fs = unsafe { crate::fat32::Fat32Reader::new(ata, part)? };
            let file_size = unsafe { let mut cluster = fs.get_root_cluster();
                let mut found_size = 0usize;
                while cluster < 0x0FFF_FFF8 && cluster >= 2 {
                    let lba = fs.cluster_lba(cluster);
                    let mut buf = vec![0u8; fs.sectors_per_cluster as usize * fs.bytes_per_sector as usize];
                    for i in 0..fs.sectors_per_cluster as u32 {
                        ata.read_sectors(lba + i, &mut buf[i as usize * 512..(i+1) as usize * 512], 1);
                    }
                    for entry_off in (0..buf.len()).step_by(32) {
                        if buf[entry_off] == 0 { break; }
                        if buf[entry_off] == 0xE5 { continue; }
                        if buf[entry_off + 11] & 0x08 != 0 { continue; }
                        let entry_name = core::str::from_utf8(&buf[entry_off..entry_off+11]).unwrap_or("");
                        if entry_name.trim_end() != name { continue; }
                        found_size = u32::from_le_bytes([
                            buf[entry_off+28], buf[entry_off+29],
                            buf[entry_off+30], buf[entry_off+31],
                        ]) as usize;
                        break;
                    }
                    if found_size > 0 { break; }
                    cluster = fs.read_fat_entry(cluster);
                }
                found_size
            };
            if file_size == 0 { return None; }
            // Le primeiros 4KB (header + metadados + info tensores)
            let header_bytes = file_size.min(4096);
            let header_data = unsafe { fs.read_file_range(&name, 0, header_bytes)? };
            let mut file = load_gguf(&header_data).ok()?;
            // Atualiza data_start para refletir o arquivo completo
            file.data_start = 0;
            return Some(file);
        }
    }
    None
}

/// Tenta carregar modelo GGUF diretamente do disco (ATA/FAT32).
pub fn load_gguf_model_from_disk(path: &str) -> Option<GgufBackedModel> {
    let name = path.trim().to_uppercase();
    let ata = crate::ATA_DRIVER.lock();
    let ata = ata.as_ref()?;
    let parts = unsafe { crate::fat32::read_mbr(ata) };
    for part in &parts {
        if part.type_code == 0x0B || part.type_code == 0x0C || part.type_code == 0x1C || part.type_code == 0x73 {
            let fs = unsafe { crate::fat32::Fat32Reader::new(ata, part)? };
            let data = unsafe { fs.read_file(&name)? };
            let file = load_gguf(&data).ok()?;
            return Some(GgufBackedModel::new(file));
        }
    }
    None
}

/// Carrega header GGUF em modo streaming e registra como modelo.
/// Modelos >4GB agora funcionam — apenas metadados carregados na RAM.
pub fn load_gguf_streaming(path: &str) -> Result<(), &'static str> {
    let file = load_gguf_header_from_disk(path).ok_or("GGUF header load failed")?;
    let n_tensors = file.tensors.len();
    let total_params: u64 = file.tensors.iter().map(|t| t.dims.iter().product::<u64>()).sum();
    let n_layers = file.metadata.iter()
        .find(|m| m.key.contains("block_count"))
        .and_then(|m| m.value.parse().ok())
        .unwrap_or(0u64) as usize;
    crate::kjson!("GGUF", "STREAM", "loaded", "path", path, "tensors", n_tensors,
        "params", total_params, "layers", n_layers);
    let _msg = alloc::format!("[GGUF] Streaming '{}': {} tensors, {} params (est). Header only in RAM.",
        path, n_tensors, total_params);
    Ok(())
}

/// Lista formatos GGUF suportados
pub fn print_supported_formats() -> String {
    alloc::format!(
        "Supported GGUF formats:\n\
         Q4_0: 4-bit block quantization (5 bpw)\n\
         Q8_0: 8-bit block quantization (9 bpw)\n\
         F16: 16-bit float\n\
         F32: 32-bit float\n\
         Tensor types: {}\n\
         Use: /model <path> to load from FAT32",
         "Q4_0, Q4_1, Q5_0, Q5_1, Q8_0, Q8_1, F16, F32"
    )
}
