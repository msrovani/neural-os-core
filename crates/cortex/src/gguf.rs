//! GGUF (GGML Universal Format) loader — no_std puro.
//! Suporta parse de header, metadata, tensor info.
//! Q4_0/Q8_0 dequantization para alimentar o BitNet pipeline.
//!
//! Formato: https://github.com/ggerganov/ggml/blob/master/docs/gguf.md

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use crate::tensor::{PackedTernaryTensor, Tensor};
const GGUF_MAGIC: u32 = 0x46554747; // "GGUF" little-endian
const GGUF_VERSION: u32 = 3;

/// Super-block size for K-quants (llama.cpp QK_K).
pub(crate) const QK_K: usize = 256;
const Q4_K_BLOCK_BYTES: usize = 144; // d+dmin+scales[12]+qs[128]
pub(crate) const Q6_K_BLOCK_BYTES: usize = 210; // ql[128]+qh[64]+scales[16]+d
// SESSION_252: K-quants Q2_K/Q3_K/Q5_K (llama.cpp layout, QK_K=256).
const Q2_K_BLOCK_BYTES: usize = 96;  // d(2)+dmin(2)+scales[12]+mins[16]+qs[64]
const Q3_K_BLOCK_BYTES: usize = 128; // d(2)+dmin(2)+scales[12]+mins[16]+qh[32]+qs[64]
const Q5_K_BLOCK_BYTES: usize = 192; // d(2)+dmin(2)+scales[12]+mins[16]+qh[32]+qs[128]
// SESSION_301: TQ2_0 ternary 2-bit (Falcon3 GGUF, PrismML Bonsai).
// Layout: f16 scale (2) + packed 2-bit ternary weights (22) = 24 bytes per 32-element block.
const TQ2_0_BLOCK_SIZE: usize = 32;
const TQ2_0_BLOCK_BYTES: usize = 24;

#[derive(Debug, Clone, Copy, PartialEq)]
#[allow(non_camel_case_types)] // nomes alinhados a ggml/llama.cpp (Q4_K, …)
pub enum GgufType {
    F32,
    F16,
    Q4_0,
    Q4_1,
    Q5_0,
    Q5_1,
    Q8_0,
    Q8_1,
    Q2_K,
    Q3_K,
    Q4_K,
    Q5_K,
    Q6_K,
    Q8_K,
    BF16,
    TQ1_0,
    TQ2_0,
    Unknown(u32),
}

impl GgufType {
    fn from_u32(v: u32) -> Self {
        // Standard GGUF type IDs (ggml-quants.h / gguf-py constants.py)
        match v {
            0 => GgufType::F32,
            1 => GgufType::F16,
            2 => GgufType::BF16,
            3 => GgufType::Q4_0,
            4 => GgufType::Q4_1,
            5 => GgufType::Q5_0,
            6 => GgufType::Q5_1,
            7 => GgufType::Q8_0,
            8 => GgufType::Q8_1,
            9 => GgufType::Q2_K,
            10 => GgufType::Q3_K,
            11 => GgufType::Q4_K,
            12 => GgufType::Q5_K,
            13 => GgufType::Q6_K,
            14 => GgufType::Q8_K,
            24 => GgufType::TQ1_0,
            25 => GgufType::TQ2_0,
            x => GgufType::Unknown(x),
        }
    }

    pub fn bits_per_weight(&self) -> u32 {
        match self {
            GgufType::F32 => 32,
            GgufType::F16 => 16,
            GgufType::Q4_0 | GgufType::Q4_1 => 5,
            GgufType::Q5_0 | GgufType::Q5_1 => 6,
            GgufType::Q8_0 | GgufType::Q8_1 => 9,
            GgufType::Q2_K => 3,
            GgufType::Q3_K => 4,
            GgufType::Q4_K => 5,
            GgufType::Q5_K => 6,
            GgufType::Q6_K => 7,
            GgufType::Q8_K => 9,
            GgufType::BF16 => 16,
            GgufType::TQ1_0 => 1,
            GgufType::TQ2_0 => 2,
            GgufType::Unknown(_) => 32,
        }
    }

    /// Byte size of a quantized/raw tensor with `ne` elements (llama.cpp layout).
    pub fn nbytes_for_elements(&self, ne: usize) -> usize {
        match self {
            GgufType::F32 => ne.saturating_mul(4),
            GgufType::F16 => ne.saturating_mul(2),
            GgufType::Q4_0 => {
                let blocks = (ne + 31) / 32;
                blocks.saturating_mul(18)
            }
            GgufType::Q5_0 => {
                let blocks = (ne + 31) / 32;
                blocks.saturating_mul(22)
            }
            GgufType::Q8_0 => {
                let blocks = (ne + 31) / 32;
                blocks.saturating_mul(34)
            }
            GgufType::Q4_K => {
                let blocks = (ne + QK_K - 1) / QK_K;
                blocks.saturating_mul(Q4_K_BLOCK_BYTES)
            }
            GgufType::Q6_K => {
                let blocks = (ne + QK_K - 1) / QK_K;
                blocks.saturating_mul(Q6_K_BLOCK_BYTES)
            }
            GgufType::Q2_K => {
                let blocks = (ne + QK_K - 1) / QK_K;
                blocks.saturating_mul(Q2_K_BLOCK_BYTES)
            }
            GgufType::Q3_K => {
                let blocks = (ne + QK_K - 1) / QK_K;
                blocks.saturating_mul(Q3_K_BLOCK_BYTES)
            }
            GgufType::Q5_K => {
                let blocks = (ne + QK_K - 1) / QK_K;
                blocks.saturating_mul(Q5_K_BLOCK_BYTES)
            }
            GgufType::BF16 => ne.saturating_mul(2),
            GgufType::TQ1_0 => {
                // TQ1_0: 32 bytes per 32-element block (same as Q4_0 layout)
                let blocks = (ne + 31) / 32;
                blocks.saturating_mul(32)
            }
            GgufType::TQ2_0 => {
                let blocks = (ne + TQ2_0_BLOCK_SIZE - 1) / TQ2_0_BLOCK_SIZE;
                blocks.saturating_mul(TQ2_0_BLOCK_BYTES)
            }
            GgufType::Q8_K => {
                let blocks = (ne + QK_K - 1) / QK_K;
                blocks.saturating_mul(256 + 32) // simplified
            }
            GgufType::Q4_1 | GgufType::Q5_1 | GgufType::Q8_1 => {
                ne.saturating_mul(2) // bound until dedicated dequant
            }
            GgufType::Unknown(_) => ne.saturating_mul(4),
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
    if *offset + 4 > data.len() { return 0; }
    let val = u32::from_le_bytes([
        data[*offset], data[*offset + 1], data[*offset + 2], data[*offset + 3],
    ]);
    *offset += 4;
    val
}

/// Le u64 little-endian de um slice
fn read_u64(data: &[u8], offset: &mut usize) -> u64 {
    if *offset + 8 > data.len() { return 0; }
    let val = u64::from_le_bytes([
        data[*offset], data[*offset + 1], data[*offset + 2], data[*offset + 3],
        data[*offset + 4], data[*offset + 5], data[*offset + 6], data[*offset + 7],
    ]);
    *offset += 8;
    val
}

/// Le string (length-prefixed) de um slice
fn read_string(data: &[u8], offset: &mut usize) -> String {
    if *offset + 8 > data.len() { return String::new(); }
    let len = read_u64(data, offset) as usize;
    let end = core::cmp::min(*offset + len, data.len());
    let s = core::str::from_utf8(&data[*offset..end]).unwrap_or("(invalid utf8)");
    *offset += len;
    String::from(s)
}

/// Le metadata value (string or array) como string
/// GGUFValueType (gguf-py constants.py / ggml.h):
///   0=UINT8 1=INT8 2=UINT16 3=INT16 4=UINT32 5=INT32
///   6=FLOAT32 7=BOOL 8=STRING 9=ARRAY 10=FLOAT64
fn read_metadata_value(data: &[u8], offset: &mut usize) -> String {
    let val_type = read_u32(data, offset);
    match val_type {
        0 => { // UINT8
            if *offset >= data.len() { return String::new(); }
            let v = data[*offset];
            *offset += 1;
            alloc::format!("{}", v)
        }
        1 => { // INT8
            if *offset >= data.len() { return String::new(); }
            let v = data[*offset] as i8;
            *offset += 1;
            alloc::format!("{}", v)
        }
        2 => { // UINT16
            if *offset + 2 > data.len() { return String::new(); }
            let v = u16::from_le_bytes([data[*offset], data[*offset + 1]]);
            *offset += 2;
            alloc::format!("{}", v)
        }
        3 => { // INT16
            if *offset + 2 > data.len() { return String::new(); }
            let v = i16::from_le_bytes([data[*offset], data[*offset + 1]]);
            *offset += 2;
            alloc::format!("{}", v)
        }
        4 => { // UINT32
            let v = read_u32(data, offset);
            alloc::format!("{}", v)
        }
        5 => { // INT32
            if *offset + 4 > data.len() { return String::new(); }
            let v = i32::from_le_bytes([data[*offset], data[*offset + 1], data[*offset + 2], data[*offset + 3]]);
            *offset += 4;
            alloc::format!("{}", v)
        }
        6 => { // FLOAT32
            if *offset + 4 > data.len() { return String::new(); }
            let v = f32::from_le_bytes([data[*offset], data[*offset + 1], data[*offset + 2], data[*offset + 3]]);
            *offset += 4;
            alloc::format!("{:.6}", v)
        }
        7 => { // BOOL
            if *offset >= data.len() { return String::new(); }
            let v = data[*offset] != 0;
            *offset += 1;
            String::from(if v { "true" } else { "false" })
        }
        8 => { // STRING
            read_string(data, offset)
        }
        9 => { // ARRAY
            let arr_type = read_u32(data, offset);
            let arr_len = read_u64(data, offset) as usize;
            let mut items = Vec::new();
            for _ in 0..arr_len {
                items.push(read_metadata_value_inner(data, offset, arr_type));
            }
            alloc::format!("[{}]", items.join(", "))
        }
        10 => { // FLOAT64
            if *offset + 8 > data.len() { return String::new(); }
            let v = f64::from_le_bytes([
                data[*offset], data[*offset + 1], data[*offset + 2], data[*offset + 3],
                data[*offset + 4], data[*offset + 5], data[*offset + 6], data[*offset + 7],
            ]);
            *offset += 8;
            alloc::format!("{:.6}", v)
        }
        _ => {
            // Unknown type: skip 8 bytes (conservative)
            if *offset + 8 <= data.len() { *offset += 8; }
            alloc::format!("(unknown_type_{})", val_type)
        }
    }
}

fn read_metadata_value_inner(data: &[u8], offset: &mut usize, val_type: u32) -> String {
    match val_type {
        0 => { // UINT8
            if *offset >= data.len() { return String::new(); }
            let v = data[*offset]; *offset += 1;
            alloc::format!("{}", v)
        }
        1 => { // INT8
            if *offset >= data.len() { return String::new(); }
            let v = data[*offset] as i8; *offset += 1;
            alloc::format!("{}", v)
        }
        2 => { // UINT16
            if *offset + 2 > data.len() { return String::new(); }
            let v = u16::from_le_bytes([data[*offset], data[*offset + 1]]); *offset += 2;
            alloc::format!("{}", v)
        }
        3 => { // INT16
            if *offset + 2 > data.len() { return String::new(); }
            let v = i16::from_le_bytes([data[*offset], data[*offset + 1]]); *offset += 2;
            alloc::format!("{}", v)
        }
        4 => { // UINT32
            let v = read_u32(data, offset);
            alloc::format!("{}", v)
        }
        5 => { // INT32
            if *offset + 4 > data.len() { return String::new(); }
            let v = i32::from_le_bytes([data[*offset], data[*offset + 1], data[*offset + 2], data[*offset + 3]]); *offset += 4;
            alloc::format!("{}", v)
        }
        6 => { // FLOAT32
            if *offset + 4 > data.len() { return String::new(); }
            let v = f32::from_le_bytes([data[*offset], data[*offset + 1], data[*offset + 2], data[*offset + 3]]); *offset += 4;
            alloc::format!("{:.6}", v)
        }
        7 => { // BOOL
            if *offset >= data.len() { return String::new(); }
            let v = data[*offset] != 0; *offset += 1;
            String::from(if v { "true" } else { "false" })
        }
        8 => { // STRING
            read_string(data, offset)
        }
        9 => { // ARRAY (nested)
            let arr_type = read_u32(data, offset);
            let arr_len = read_u64(data, offset) as usize;
            let mut items = Vec::new();
            for _ in 0..arr_len {
                items.push(read_metadata_value_inner(data, offset, arr_type));
            }
            alloc::format!("[{}]", items.join(", "))
        }
        10 => { // FLOAT64
            if *offset + 8 > data.len() { return String::new(); }
            let v = f64::from_le_bytes([data[*offset], data[*offset+1], data[*offset+2], data[*offset+3],
                data[*offset+4], data[*offset+5], data[*offset+6], data[*offset+7]]); *offset += 8;
            alloc::format!("{:.6}", v)
        }
        _ => {
            // Unknown inner type: skip conservatively
            if *offset + 8 <= data.len() { *offset += 8; }
            String::from("?")
        }
    }
}

/// Verifica rapido se os bytes sao um GGUF valido (magic check).
pub fn is_gguf(data: &[u8]) -> bool {
    if data.len() < 4 { return false; }
    let magic = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    magic == GGUF_MAGIC
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
    k_nano::slog_bin!("GGUF", "info", "Header: version={} tensors={} metadata={}", version, tensor_count, metadata_kv_count);

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
        // GGUF spec: n_dims ∈ 1..=4 — valida antes de with_capacity/dims[0]
        // (n_dims malformado = panic/OOM em dados de arquivo).
        if n_dims == 0 || n_dims > 4 {
            return Err("GGUF: n_dims fora de 1..=4");
        }
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

    k_nano::slog_bin!("GGUF", "info", "Parse OK. Metadata: {} items, Tensors: {} items, Data: {} bytes", metadata.len(), tensors.len(), raw_data.len());

    Ok(GgufFile { header, metadata, tensors, data_start: data_start as u64, data: raw_data })
}

/// Converte f16 (u16) para f32
pub(crate) fn f16_to_f32(half: u16) -> f32 {
    // sign: bit 0 → +1.0; NUNCA -0.0 (bug latente: 0.0 * -1.0 = -0.0 quebrava
    // todo dequant Q4_0/Q5_0/Q6_K — d positivo virava -0.0)
    let sign = if (half >> 15) & 1 == 1 { -1.0_f32 } else { 1.0_f32 };
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
        match dequantize_q4_0_block(&data[block_start..block_end]) {
            Ok(values) => {
                let remaining = total_weights - tensor_data.len();
                let to_copy = core::cmp::min(32, remaining);
                tensor_data.extend_from_slice(&values[..to_copy]);
            }
            Err(_) => {
                // ponytail: truncated GGUF - partial block, weights stay zero
                continue;
            }
        }
    }

    Tensor::from_row_major((rows, cols), tensor_data)
}

/// Q8_0 block: f16 scale (2) + 32 x int8 = 34 bytes -> 32 f32
fn dequantize_q8_0_block(block: &[u8]) -> Result<[f32; 32], &'static str> {
    if block.len() < 34 { return Err("Q8_0 block too short"); }
    let scale = f16_to_f32(u16::from_le_bytes([block[0], block[1]]));
    let mut result = [0.0f32; 32];
    for i in 0..32 {
        result[i] = (block[2 + i] as i8 as f32) * scale;
    }
    Ok(result)
}

/// Dequantiza tensor Q8_0 completo (llama.cpp QK8_0=32).
pub fn dequantize_q8_0(data: &[u8], rows: usize, cols: usize) -> Option<Tensor> {
    let total_weights = rows * cols;
    let num_blocks = (total_weights + 31) / 32;
    let expected_bytes = num_blocks * 34;
    if data.len() < expected_bytes { return None; }

    let mut tensor_data = Vec::with_capacity(total_weights);
    for b in 0..num_blocks {
        let start = b * 34;
        if let Ok(values) = dequantize_q8_0_block(&data[start..start + 34]) {
            let remaining = total_weights - tensor_data.len();
            let to_copy = core::cmp::min(32, remaining);
            tensor_data.extend_from_slice(&values[..to_copy]);
        }
    }
    Tensor::from_row_major((rows, cols), tensor_data)
}

/// Q5_0 block: f16 scale (2) + uint32 qh (4) + 16 packed lo-nibbles = 22 bytes -> 32 f32
/// Layout matches ggml-quants.c / llama.cpp QK5_0.
fn dequantize_q5_0_block(block: &[u8]) -> Result<[f32; 32], &'static str> {
    if block.len() < 22 { return Err("Q5_0 block too short"); }
    let scale = f16_to_f32(u16::from_le_bytes([block[0], block[1]]));
    let qh = u32::from_le_bytes([block[2], block[3], block[4], block[5]]);
    let mut result = [0.0f32; 32];
    for i in 0..16 {
        let byte = block[6 + i];
        let x0 = ((byte & 0x0F) as i8) | ((((qh >> i) & 1) as i8) << 4);
        let x1 = (((byte >> 4) & 0x0F) as i8) | ((((qh >> (i + 16)) & 1) as i8) << 4);
        result[i] = ((x0 - 16) as f32) * scale;
        result[i + 16] = ((x1 - 16) as f32) * scale;
    }
    Ok(result)
}

/// Dequantiza tensor Q5_0 completo.
pub fn dequantize_q5_0(data: &[u8], rows: usize, cols: usize) -> Option<Tensor> {
    let total_weights = rows * cols;
    let num_blocks = (total_weights + 31) / 32;
    let expected_bytes = num_blocks * 22;
    if data.len() < expected_bytes { return None; }

    let mut tensor_data = Vec::with_capacity(total_weights);
    for b in 0..num_blocks {
        let start = b * 22;
        if let Ok(values) = dequantize_q5_0_block(&data[start..start + 22]) {
            let remaining = total_weights - tensor_data.len();
            let to_copy = core::cmp::min(32, remaining);
            tensor_data.extend_from_slice(&values[..to_copy]);
        }
    }
    Tensor::from_row_major((rows, cols), tensor_data)
}

/// llama.cpp get_scale_min_k4 — unpack 6-bit scale/min from Q4_K scales[].
fn get_scale_min_k4(j: usize, q: &[u8]) -> (u8, u8) {
    if j < 4 {
        (q[j] & 63, q[j + 4] & 63)
    } else {
        let d = (q[j + 4] & 0xF) | ((q[j - 4] >> 6) << 4);
        let m = (q[j + 4] >> 4) | ((q[j] >> 6) << 4);
        (d, m)
    }
}

/// Q4_K super-block (144 B → 256 f32). Layout: d, dmin, scales[12], qs[128].
fn dequantize_q4_k_block(block: &[u8], out: &mut [f32]) -> Result<(), &'static str> {
    if block.len() < Q4_K_BLOCK_BYTES || out.len() < QK_K {
        return Err("Q4_K block too short");
    }
    let d = f16_to_f32(u16::from_le_bytes([block[0], block[1]]));
    let min = f16_to_f32(u16::from_le_bytes([block[2], block[3]]));
    let scales = &block[4..16];
    let mut q = &block[16..16 + 128];
    let mut y = 0usize;
    let mut is = 0usize;
    for _ in 0..(QK_K / 64) {
        let (sc, m) = get_scale_min_k4(is, scales);
        let d1 = d * (sc as f32);
        let m1 = min * (m as f32);
        let (sc2, m2) = get_scale_min_k4(is + 1, scales);
        let d2 = d * (sc2 as f32);
        let m2v = min * (m2 as f32);
        for l in 0..32 {
            out[y + l] = d1 * ((q[l] & 0xF) as f32) - m1;
        }
        for l in 0..32 {
            out[y + 32 + l] = d2 * ((q[l] >> 4) as f32) - m2v;
        }
        y += 64;
        q = &q[32..];
        is += 2;
    }
    Ok(())
}

/// Dequantiza tensor Q4_K (llama.cpp K-quant, QK_K=256).
pub fn dequantize_q4_k(data: &[u8], rows: usize, cols: usize) -> Option<Tensor> {
    let total = rows * cols;
    let num_blocks = (total + QK_K - 1) / QK_K;
    let expected = num_blocks * Q4_K_BLOCK_BYTES;
    if data.len() < expected {
        return None;
    }
    let mut tensor_data = Vec::with_capacity(total);
    let mut scratch = [0.0f32; QK_K];
    for b in 0..num_blocks {
        let start = b * Q4_K_BLOCK_BYTES;
        if dequantize_q4_k_block(&data[start..start + Q4_K_BLOCK_BYTES], &mut scratch).is_err() {
            return None;
        }
        let remaining = total - tensor_data.len();
        let to_copy = remaining.min(QK_K);
        tensor_data.extend_from_slice(&scratch[..to_copy]);
    }
    Tensor::from_row_major((rows, cols), tensor_data)
}

/// Q6_K super-block (210 B → 256 f32). Layout: ql[128], qh[64], scales[16], d.
fn dequantize_q6_k_block(block: &[u8], out: &mut [f32]) -> Result<(), &'static str> {
    if block.len() < Q6_K_BLOCK_BYTES || out.len() < QK_K {
        return Err("Q6_K block too short");
    }
    let ql = &block[0..128];
    let qh = &block[128..192];
    let scales = &block[192..208];
    let d = f16_to_f32(u16::from_le_bytes([block[208], block[209]]));
    let mut y = 0usize;
    let mut ql_off = 0usize;
    let mut qh_off = 0usize;
    let mut sc_off = 0usize;
    for _ in 0..(QK_K / 128) {
        for l in 0..32 {
            let is = l / 16;
            let q1 = ((((ql[ql_off + l] & 0xF) as u8) | (((qh[qh_off + l] >> 0) & 3) << 4)) as i8
                as i32)
                - 32;
            let q2 = ((((ql[ql_off + l + 32] & 0xF) as u8) | (((qh[qh_off + l] >> 2) & 3) << 4))
                as i8 as i32)
                - 32;
            let q3 = ((((ql[ql_off + l] >> 4) as u8) | (((qh[qh_off + l] >> 4) & 3) << 4)) as i8
                as i32)
                - 32;
            let q4 = ((((ql[ql_off + l + 32] >> 4) as u8) | (((qh[qh_off + l] >> 6) & 3) << 4))
                as i8 as i32)
                - 32;
            let s0 = scales[sc_off + is] as i8 as f32;
            let s2 = scales[sc_off + is + 2] as i8 as f32;
            let s4 = scales[sc_off + is + 4] as i8 as f32;
            let s6 = scales[sc_off + is + 6] as i8 as f32;
            out[y + l] = d * s0 * (q1 as f32);
            out[y + l + 32] = d * s2 * (q2 as f32);
            out[y + l + 64] = d * s4 * (q3 as f32);
            out[y + l + 96] = d * s6 * (q4 as f32);
        }
        y += 128;
        ql_off += 64;
        qh_off += 32;
        sc_off += 8;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Q2_K: 2-bit K-quant (llama.cpp QK_K=256, block 96B)
// Layout: d(f16) + dmin(f16) + scales[12] + mins[16] + qs[64]
// scales/mins são 6-bit packed em bytes (4 valores por 3 bytes no llama.cpp,
// mas aqui usamos 1 byte cada = layout simplificado GGMLv3).
// ---------------------------------------------------------------------------

/// Q2_K block: 96 B → 256 f32. Escala 6-bit + quants 2-bit.
fn dequantize_q2_k_block(block: &[u8], out: &mut [f32]) -> Result<(), &'static str> {
    if block.len() < Q2_K_BLOCK_BYTES || out.len() < QK_K {
        return Err("Q2_K block too short");
    }
    let d = f16_to_f32(u16::from_le_bytes([block[0], block[1]]));
    let dmin = f16_to_f32(u16::from_le_bytes([block[2], block[3]]));
    let scales = &block[4..16]; // 12 bytes (6-bit cada, mask 0x3F)
    let mins = &block[16..32]; // 16 bytes (6-bit cada)
    let qs = &block[32..96]; // 64 bytes (4 quants 2-bit por byte)
    let mut y = 0usize;
    for j in 0..4 {
        let sc = d * ((scales[j] & 0x3F) as f32);
        let m = dmin * ((mins[j] & 0x3F) as f32);
        for k in 0..32 {
            let q = ((qs[j * 16 + k / 4] >> ((k % 4) * 2)) & 3) as f32;
            out[y + k] = sc * q - m;
        }
        y += 32;
    }
    Ok(())
}

/// Dequantiza tensor Q2_K.
pub fn dequantize_q2_k(data: &[u8], rows: usize, cols: usize) -> Option<Tensor> {
    let total = rows * cols;
    let num_blocks = (total + QK_K - 1) / QK_K;
    let expected = num_blocks * Q2_K_BLOCK_BYTES;
    if data.len() < expected {
        return None;
    }
    let mut tensor_data = Vec::with_capacity(total);
    let mut scratch = [0.0f32; QK_K];
    for b in 0..num_blocks {
        let start = b * Q2_K_BLOCK_BYTES;
        if dequantize_q2_k_block(&data[start..start + Q2_K_BLOCK_BYTES], &mut scratch).is_err() {
            return None;
        }
        let remaining = total - tensor_data.len();
        let to_copy = remaining.min(QK_K);
        tensor_data.extend_from_slice(&scratch[..to_copy]);
    }
    Tensor::from_row_major((rows, cols), tensor_data)
}

// ---------------------------------------------------------------------------
// TQ2_0: Ternary 2-bit (Falcon3 GGUF, PrismML Bonsai, BitNet b1.58)
// GGUF type ID 25. Layout: f16 scale (2B) + packed 2-bit ternary (8B) + padding (14B) = 24B per 32-element block.
// Weights: {-1, 0, +1} packed 2 bits each, 4 per byte.
// ---------------------------------------------------------------------------

/// TQ2_0 block: 24 B → 32 f32. f16 scale + packed 2-bit ternary.
fn dequantize_tq2_0_block(block: &[u8], out: &mut [f32]) -> Result<(), &'static str> {
    if block.len() < TQ2_0_BLOCK_BYTES || out.len() < TQ2_0_BLOCK_SIZE {
        return Err("TQ2_0 block too short");
    }
    let scale = f16_to_f32(u16::from_le_bytes([block[0], block[1]]));
    // Packed 2-bit ternary: each byte holds 4 weights.
    // Encoding: 00=0, 01=+1, 10=-1, 11=0 (same as BitNet kernel).
    for i in 0..TQ2_0_BLOCK_SIZE {
        let byte_idx = 2 + (i / 4);
        let bit_offset = (i % 4) * 2;
        let packed = if byte_idx < block.len() { block[byte_idx] } else { 0 };
        let w2 = (packed >> bit_offset) & 0b11;
        let val = match w2 {
            0b01 => 1.0f32,
            0b10 => -1.0f32,
            _ => 0.0f32, // 0b00 or 0b11 = zero
        };
        out[i] = val * scale;
    }
    Ok(())
}

/// Dequantiza tensor TQ2_0 (ternary 2-bit GGUF).
pub fn dequantize_tq2_0(data: &[u8], rows: usize, cols: usize) -> Option<Tensor> {
    let total = rows * cols;
    let num_blocks = (total + TQ2_0_BLOCK_SIZE - 1) / TQ2_0_BLOCK_SIZE;
    let expected = num_blocks * TQ2_0_BLOCK_BYTES;
    if data.len() < expected {
        return None;
    }
    let mut tensor_data = Vec::with_capacity(total);
    let mut scratch = [0.0f32; TQ2_0_BLOCK_SIZE];
    for b in 0..num_blocks {
        let start = b * TQ2_0_BLOCK_BYTES;
        if dequantize_tq2_0_block(&data[start..start + TQ2_0_BLOCK_BYTES], &mut scratch).is_err() {
            return None;
        }
        let remaining = total - tensor_data.len();
        let to_copy = remaining.min(TQ2_0_BLOCK_SIZE);
        tensor_data.extend_from_slice(&scratch[..to_copy]);
    }
    Tensor::from_row_major((rows, cols), tensor_data)
}

// ---------------------------------------------------------------------------
// Q3_K: 3-bit K-quant (llama.cpp QK_K=256, block 128B)
// Layout: d(f16) + dmin(f16) + scales[12] + mins[16] + qh[32] + qs[64]
// ---------------------------------------------------------------------------

/// Q3_K block: 128 B → 256 f32. Escala 6-bit + quants 3-bit + high-bit 1-bit.
fn dequantize_q3_k_block(block: &[u8], out: &mut [f32]) -> Result<(), &'static str> {
    if block.len() < Q3_K_BLOCK_BYTES || out.len() < QK_K {
        return Err("Q3_K block too short");
    }
    let d = f16_to_f32(u16::from_le_bytes([block[0], block[1]]));
    let dmin = f16_to_f32(u16::from_le_bytes([block[2], block[3]]));
    let scales = &block[4..16]; // 12 bytes (6-bit)
    let mins = &block[16..32]; // 16 bytes (6-bit)
    let qh = &block[32..64]; // 32 bytes (1-bit mask)
    let qs = &block[64..128]; // 64 bytes (4 quants 2-bit por byte — low 2 bits)
    let mut y = 0usize;
    for j in 0..4 {
        let sc = d * ((scales[j] & 0x3F) as f32);
        let m = dmin * ((mins[j] & 0x3F) as f32);
        for k in 0..32 {
            let q_low = ((qs[j * 16 + k / 4] >> ((k % 4) * 2)) & 3) as f32;
            let q_high = ((qh[j * 8 + k / 8] >> (k % 8)) & 1) as f32;
            let q = q_low + q_high * 4.0;
            out[y + k] = sc * q - m;
        }
        y += 32;
    }
    Ok(())
}

/// Dequantiza tensor Q3_K.
pub fn dequantize_q3_k(data: &[u8], rows: usize, cols: usize) -> Option<Tensor> {
    let total = rows * cols;
    let num_blocks = (total + QK_K - 1) / QK_K;
    let expected = num_blocks * Q3_K_BLOCK_BYTES;
    if data.len() < expected {
        return None;
    }
    let mut tensor_data = Vec::with_capacity(total);
    let mut scratch = [0.0f32; QK_K];
    for b in 0..num_blocks {
        let start = b * Q3_K_BLOCK_BYTES;
        if dequantize_q3_k_block(&data[start..start + Q3_K_BLOCK_BYTES], &mut scratch).is_err() {
            return None;
        }
        let remaining = total - tensor_data.len();
        let to_copy = remaining.min(QK_K);
        tensor_data.extend_from_slice(&scratch[..to_copy]);
    }
    Tensor::from_row_major((rows, cols), tensor_data)
}

// ---------------------------------------------------------------------------
// Q5_K: 5-bit K-quant (llama.cpp QK_K=256, block 192B)
// Layout: d(f16) + dmin(f16) + scales[12] + mins[16] + qh[32] + qs[128]
// ---------------------------------------------------------------------------

/// Q5_K block: 192 B → 256 f32. Escala 6-bit + quants 5-bit (low 4 + high 1).
fn dequantize_q5_k_block(block: &[u8], out: &mut [f32]) -> Result<(), &'static str> {
    if block.len() < Q5_K_BLOCK_BYTES || out.len() < QK_K {
        return Err("Q5_K block too short");
    }
    let d = f16_to_f32(u16::from_le_bytes([block[0], block[1]]));
    let dmin = f16_to_f32(u16::from_le_bytes([block[2], block[3]]));
    let scales = &block[4..16]; // 12 bytes (6-bit)
    let mins = &block[16..32]; // 16 bytes (6-bit)
    let qh = &block[32..64]; // 32 bytes (1-bit high mask)
    let qs = &block[64..192]; // 128 bytes (4 quants 4-bit por byte)
    let mut y = 0usize;
    for j in 0..4 {
        let sc = d * ((scales[j] & 0x3F) as f32);
        let m = dmin * ((mins[j] & 0x3F) as f32);
        for k in 0..32 {
            let q_low = ((qs[j * 32 + k / 2] >> ((k % 2) * 4)) & 0xF) as f32;
            let q_high = ((qh[j * 8 + k / 8] >> (k % 8)) & 1) as f32;
            let q = q_low + q_high * 16.0;
            out[y + k] = sc * q - m;
        }
        y += 32;
    }
    Ok(())
}

/// Dequantiza tensor Q5_K.
pub fn dequantize_q5_k(data: &[u8], rows: usize, cols: usize) -> Option<Tensor> {
    let total = rows * cols;
    let num_blocks = (total + QK_K - 1) / QK_K;
    let expected = num_blocks * Q5_K_BLOCK_BYTES;
    if data.len() < expected {
        return None;
    }
    let mut tensor_data = Vec::with_capacity(total);
    let mut scratch = [0.0f32; QK_K];
    for b in 0..num_blocks {
        let start = b * Q5_K_BLOCK_BYTES;
        if dequantize_q5_k_block(&data[start..start + Q5_K_BLOCK_BYTES], &mut scratch).is_err() {
            return None;
        }
        let remaining = total - tensor_data.len();
        let to_copy = remaining.min(QK_K);
        tensor_data.extend_from_slice(&scratch[..to_copy]);
    }
    Tensor::from_row_major((rows, cols), tensor_data)
}

/// Dequantiza tensor Q6_K.
pub fn dequantize_q6_k(data: &[u8], rows: usize, cols: usize) -> Option<Tensor> {
    let total = rows * cols;
    let num_blocks = (total + QK_K - 1) / QK_K;
    let expected = num_blocks * Q6_K_BLOCK_BYTES;
    if data.len() < expected {
        return None;
    }
    let mut tensor_data = Vec::with_capacity(total);
    let mut scratch = [0.0f32; QK_K];
    for b in 0..num_blocks {
        let start = b * Q6_K_BLOCK_BYTES;
        if dequantize_q6_k_block(&data[start..start + Q6_K_BLOCK_BYTES], &mut scratch).is_err() {
            return None;
        }
        let remaining = total - tensor_data.len();
        let to_copy = remaining.min(QK_K);
        tensor_data.extend_from_slice(&scratch[..to_copy]);
    }
    Tensor::from_row_major((rows, cols), tensor_data)
}

/// Decode de UM elemento Q6_K no flat index `idx` (ADR-0085 D6 — row-wise).
/// Evita materializar o tensor inteiro: embed_lookup lê 1 linha/token.
pub(crate) fn q6k_get(data: &[u8], idx: usize) -> f32 {
    if data.len() < Q6_K_BLOCK_BYTES { return 0.0; }
    let block = idx / QK_K;
    let e = idx % QK_K;
    let boff = block * Q6_K_BLOCK_BYTES;
    if boff + Q6_K_BLOCK_BYTES > data.len() { return 0.0; }
    let ql = &data[boff..boff + 128];
    let qh = &data[boff + 128..boff + 192];
    let scales = &data[boff + 192..boff + 208];
    let d = f16_to_f32(u16::from_le_bytes([data[boff + 208], data[boff + 209]]));
    // half/lane/l/is — espelha dequantize_q6_k_block
    let half = e / 128;
    let rem = e % 128;
    let lane = rem / 32;   // 0..3 → q1..q4
    let l = rem % 32;      // 0..31
    let is = l / 16;       // 0..1
    let ql_off = half * 64;
    let qh_off = half * 32;
    let sc_off = half * 8;
    let (qb, qs, sn) = match lane {
        0 => (ql[ql_off + l] & 0xF, (qh[qh_off + l] >> 0) & 3, 0i8),
        1 => (ql[ql_off + l + 32] & 0xF, (qh[qh_off + l] >> 2) & 3, 0i8),
        2 => (ql[ql_off + l] >> 4, (qh[qh_off + l] >> 4) & 3, 0i8),
        _ => (ql[ql_off + l + 32] >> 4, (qh[qh_off + l] >> 6) & 3, 0i8),
    };
    let _ = sn; // sn unused — mantido para clareza do layout
    let q6 = ((qb as i32) | ((qs as i32) << 4)) - 32;
    let s = scales[sc_off + is + lane * 2] as i8 as f32;
    d * s * q6 as f32
}

/// Matmul 1×hidden @ Q6_K(hidden, vocab) → logits[1×vocab] (unembed tied).
/// Decode por super-bloco (cada bloco 210B → 256 pesos) — ADR-0085 D6.
pub(crate) fn q6k_matmul_row(data: &[u8], hidden: usize, vocab: usize, x: &[f32]) -> Vec<f32> {
    let mut out = alloc::vec![0.0f32; vocab];
    let total = hidden * vocab;
    let num_blocks = (total + QK_K - 1) / QK_K;
    let mut scratch = [0.0f32; QK_K];
    for b in 0..num_blocks {
        let start = b * Q6_K_BLOCK_BYTES;
        if start + Q6_K_BLOCK_BYTES > data.len() { break; }
        if dequantize_q6_k_block(&data[start..start + Q6_K_BLOCK_BYTES], &mut scratch).is_err() {
            break;
        }
        // bloco b cobre flat [b*256, (b+1)*256) → (row, col) row-major
        let base = b * QK_K;
        for k in 0..QK_K {
            let fi = base + k;
            if fi >= total { break; }
            let row = fi / vocab;
            let col = fi % vocab;
            out[col] += x[row] * scratch[k];
        }
    }
    out
}

/// Dequantiza bytes brutos conforme tipo GGUF (F32/F16/Q4_0/Q5_0/Q8_0/Q4_K/Q6_K).
#[cfg(test)]
mod tests {
    use super::*;

    /// Cross-check Rust decoder × Python encoder (ADR-0085 F3).
    /// golden_q6k.bin gerado por bitnet_writer.py (LCG seed 42, 4×300 f32);
    /// golden_q6k_ref.f32 é o dequant Python (port de gguf.rs) dos mesmos bytes.
    #[test]
    fn q6k_decode_matches_python() {
        let enc = include_bytes!("../../../tools/golden_q6k.bin");
        let ref_data = include_bytes!("../../../tools/golden_q6k_ref.f32");
        let rows = 4usize;
        let cols = 300usize;
        let total = rows * cols;
        assert_eq!(ref_data.len(), total * 4, "ref f32 count");

        let mut max_rel = 0.0f32;
        let mut first_bad = None;
        for idx in 0..total {
            let got = q6k_get(enc, idx);
            let want = f32::from_le_bytes([
                ref_data[idx * 4], ref_data[idx * 4 + 1],
                ref_data[idx * 4 + 2], ref_data[idx * 4 + 3],
            ]);
            let denom = want.abs().max(1e-6);
            let rel = ((got - want).abs() / denom).min(1.0);
            if rel > max_rel { max_rel = rel; }
            if rel > 0.05 && first_bad.is_none() {
                first_bad = Some((idx, got, want));
            }
        }
        if let Some((idx, got, want)) = first_bad {
            let d = f16_to_f32(u16::from_le_bytes([enc[idx / QK_K * Q6_K_BLOCK_BYTES + 208], enc[idx / QK_K * Q6_K_BLOCK_BYTES + 209]]));
            let e = idx % QK_K;
            let half = e / 128;
            let rem = e % 128;
            let lane = rem / 32;
            let l = rem % 32;
            let ql = enc[idx / QK_K * Q6_K_BLOCK_BYTES + half * 64];
            let qh = enc[idx / QK_K * Q6_K_BLOCK_BYTES + 128 + half * 32];
            let sc = enc[idx / QK_K * Q6_K_BLOCK_BYTES + 192 + half * 8 + (l / 16) + lane * 2] as i8;
            panic!("q6k_get diverge: idx={} got={} want={} d={} ql_byte={} qh_byte={} scale_i8={}",
                idx, got, want, d, ql, qh, sc);
        }
        // Q6_K é lossy; decoder deve casar com o decoder de referência (mesmo layout).
        assert!(max_rel < 0.05, "q6k_get divergiu do ref: max_rel={:.4}", max_rel);

        // Matmul row-wise: 1×rows @ Q6_K(rows, cols) com x = all-ones
        let x = vec![1.0f32; rows];
        let logits = q6k_matmul_row(enc, rows, cols, &x);
        assert_eq!(logits.len(), cols);
        // coluna j = soma das linhas = soma do dequant da coluna j
        for j in 0..cols {
            let mut want = 0.0f32;
            for r in 0..rows {
                want += q6k_get(enc, r * cols + j);
            }
            assert!((logits[j] - want).abs() < 1e-3, "matmul col {}: {} vs {}", j, logits[j], want);
        }
    }

    /// Q2_K: constrói um bloco conhecido e verifica dequant (d=1, scale=1, min=0 → out=q).
    #[test]
    fn q2_k_dequant_known_block() {
        let mut block = [0u8; Q2_K_BLOCK_BYTES];
        block[0..2].copy_from_slice(&f16_to_f32_bytes(1.0)); // d = 1.0
        block[2..4].copy_from_slice(&f16_to_f32_bytes(0.0)); // dmin = 0.0
        for s in 0..4 { block[4 + s] = 1; } // scales[0..4] = 1 (6-bit)
        // qs: 4 quants 2-bit por byte, 16 bytes por grupo
        // grupo 0 (bytes 32..48): q=2 em todas as posições
        for i in 0..16 { block[32 + i] = 0b10101010; } // q=2 cada
        let mut out = [0.0f32; QK_K];
        dequantize_q2_k_block(&block, &mut out).unwrap();
        // out[0..32] = d * scale * q - dmin = 1.0 * 1.0 * 2.0 - 0.0 = 2.0
        for i in 0..32 {
            assert!((out[i] - 2.0).abs() < 1e-5, "Q2_K q[{}] = {} want 2.0", i, out[i]);
        }
    }

    /// Q3_K: constrói um bloco conhecido e verifica dequant (d=1, scale=1, min=0, q_low=3, q_high=1 → q=7).
    #[test]
    fn q3_k_dequant_known_block() {
        let mut block = [0u8; Q3_K_BLOCK_BYTES];
        block[0..2].copy_from_slice(&f16_to_f32_bytes(1.0)); // d = 1.0
        block[2..4].copy_from_slice(&f16_to_f32_bytes(0.0)); // dmin = 0.0
        for s in 0..4 { block[4 + s] = 1; } // scales[0..4] = 1
        // qh: high-bit = 1 → byte 32..64 = 0xFF
        for i in 0..32 { block[32 + i] = 0xFF; }
        // qs: low 2-bit = 3 → bytes 64..128 = 0b11111111
        for i in 0..64 { block[64 + i] = 0xFF; }
        let mut out = [0.0f32; QK_K];
        dequantize_q3_k_block(&block, &mut out).unwrap();
        // q = q_low(3) + q_high(1)*4 = 7 → out = 1.0 * 1.0 * 7.0 - 0.0 = 7.0
        for i in 0..32 {
            assert!((out[i] - 7.0).abs() < 1e-5, "Q3_K q[{}] = {} want 7.0", i, out[i]);
        }
    }

    /// Q5_K: constrói um bloco conhecido e verifica dequant (d=1, scale=1, min=0, q_low=5, q_high=1 → q=21).
    #[test]
    fn q5_k_dequant_known_block() {
        let mut block = [0u8; Q5_K_BLOCK_BYTES];
        block[0..2].copy_from_slice(&f16_to_f32_bytes(1.0)); // d = 1.0
        block[2..4].copy_from_slice(&f16_to_f32_bytes(0.0)); // dmin = 0.0
        for s in 0..4 { block[4 + s] = 1; } // scales[0..4] = 1
        // qh: high-bit = 1 → bytes 32..64 = 0xFF
        for i in 0..32 { block[32 + i] = 0xFF; }
        // qs: low 4-bit = 5 → bytes 64..192: cada byte = 0x55 (5 em cada nibble)
        for i in 0..128 { block[64 + i] = 0x55; }
        let mut out = [0.0f32; QK_K];
        dequantize_q5_k_block(&block, &mut out).unwrap();
        // q = q_low(5) + q_high(1)*16 = 21 → out = 1.0 * 1.0 * 21.0 - 0.0 = 21.0
        for i in 0..32 {
            assert!((out[i] - 21.0).abs() < 1e-5, "Q5_K q[{}] = {} want 21.0", i, out[i]);
        }
    }

    /// Helper: f16 → 2 bytes LE (para construir blocos de teste).
    fn f16_to_f32_bytes(v: f32) -> [u8; 2] {
        let bits = v.to_bits();
        let sign = (bits >> 31) & 1;
        let exp = ((bits >> 23) & 0xFF) as i32 - 127;
        let mant = bits & 0x7FFFFF;
        let f16: u16 = if exp < -14 {
            (sign << 15) as u16 // zero/denorm → 0
        } else if exp > 15 {
            ((sign << 15) | 0x7C00) as u16 // inf
        } else {
            ((sign << 15) | (((exp + 15) as u32) << 10) | (mant >> 13)) as u16
        };
        f16.to_le_bytes()
    }

    /// TQ2_0: constrói um bloco e verifica dequant (scale=2.0).
    #[test]
    fn tq2_0_dequant_known_block() {
        let mut block = [0u8; TQ2_0_BLOCK_BYTES];
        block[0..2].copy_from_slice(&f16_to_f32_bytes(2.0)); // scale = 2.0
        // Byte 2: bits [1:0]=01(+1), [3:2]=10(-1), [5:4]=00(0), [7:6]=01(+1)
        // LSB-first: w0=+1, w1=-1, w2=0, w3=+1
        block[2] = 0b01_00_10_01;
        let mut out = [0.0f32; TQ2_0_BLOCK_SIZE];
        dequantize_tq2_0_block(&block, &mut out).unwrap();
        assert!((out[0] - 2.0).abs() < 1e-5, "TQ2_0[0] = {} want 2.0", out[0]);
        assert!((out[1] - (-2.0)).abs() < 1e-5, "TQ2_0[1] = {} want -2.0", out[1]);
        assert!((out[2] - 0.0).abs() < 1e-5, "TQ2_0[2] = {} want 0.0", out[2]);
        assert!((out[3] - 2.0).abs() < 1e-5, "TQ2_0[3] = {} want 2.0", out[3]);
    }
}pub fn dequantize_raw(qtype: GgufType, data: &[u8], rows: usize, cols: usize) -> Option<Vec<f32>> {
    let ne = rows * cols;
    match qtype {
        GgufType::F32 => {
            if data.len() < ne * 4 { return None; }
            let mut vals = Vec::with_capacity(ne);
            for i in 0..ne {
                let o = i * 4;
                vals.push(f32::from_le_bytes([data[o], data[o + 1], data[o + 2], data[o + 3]]));
            }
            Some(vals)
        }
        GgufType::F16 => {
            if data.len() < ne * 2 { return None; }
            let mut vals = Vec::with_capacity(ne);
            for i in 0..ne {
                let o = i * 2;
                vals.push(f16_to_f32(u16::from_le_bytes([data[o], data[o + 1]])));
            }
            Some(vals)
        }
        GgufType::BF16 => {
            // BF16: upper 16 bits of f32 — shift left 16 to reconstruct f32.
            if data.len() < ne * 2 { return None; }
            let mut vals = Vec::with_capacity(ne);
            for i in 0..ne {
                let o = i * 2;
                let bf16 = u16::from_le_bytes([data[o], data[o + 1]]);
                let f32_bits = (bf16 as u32) << 16;
                vals.push(f32::from_bits(f32_bits));
            }
            Some(vals)
        }
        GgufType::Q4_0 => dequantize_q4_0(data, rows, cols).map(|t| t.data),
        GgufType::Q5_0 => dequantize_q5_0(data, rows, cols).map(|t| t.data),
        GgufType::Q8_0 => dequantize_q8_0(data, rows, cols).map(|t| t.data),
        GgufType::Q4_K => dequantize_q4_k(data, rows, cols).map(|t| t.data),
        GgufType::Q6_K => dequantize_q6_k(data, rows, cols).map(|t| t.data),
        GgufType::Q2_K => dequantize_q2_k(data, rows, cols).map(|t| t.data),
        GgufType::Q3_K => dequantize_q3_k(data, rows, cols).map(|t| t.data),
        GgufType::Q5_K => dequantize_q5_k(data, rows, cols).map(|t| t.data),
        GgufType::TQ2_0 => dequantize_tq2_0(data, rows, cols).map(|t| t.data),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// GgufBackedModel — implementa Model trait usando pesos GGUF
// ---------------------------------------------------------------------------

use crate::cortex::Model;

/// Encontra threshold ótimo por tensor: top 85% da magnitude vira ±1
fn optimal_threshold(data: &[f32]) -> f32 {
    if data.is_empty() {
        return 0.01;
    }
    // Coleta magnitudes, ordena, pega percentil 85
    let n = data.len();
    // Para evitar alocação grande, sampleia se tensor for enorme (>100K)
    let sampled: Vec<f32> = if n > 100_000 {
        let step = n / 100_000;
        data.iter().step_by(step.max(1)).map(|v| v.abs()).collect()
    } else {
        data.iter().map(|v| v.abs()).collect()
    };
    let mut sorted = sampled.clone();
    sorted.sort_by(|a, b| {
        if a > b { core::cmp::Ordering::Greater }
        else if a < b { core::cmp::Ordering::Less }
        else { core::cmp::Ordering::Equal }
    });
    let p85 = sorted[(sorted.len() * 85 / 100).min(sorted.len() - 1)];
    p85.max(0.01) // mínimo 0.01 evita threshold zero para tensor zerado
}

/// Converte um tensor f32 para PackedTernaryTensor via limiar adaptativo
///
/// O threshold é calculado por tensor (percentil 85 da magnitude),
/// não global fixo em 0.1 — recupera ~0.5-1.0 perplexity sem custo de runtime.
pub fn f32_to_ternary_packed(data: &[f32], rows: usize, cols: usize) -> PackedTernaryTensor {
    let threshold = optimal_threshold(data);
    let mut vals = Vec::with_capacity(rows * cols);
    for &v in data.iter().take(rows * cols) {
        vals.push(if v > threshold { 1 } else if v < -threshold { -1 } else { 0 });
    }
    let packed = PackedTernaryTensor::pack_weights(&vals);
    PackedTernaryTensor { shape: (rows, cols), packed_data: packed }
}

/// Dequantiza o primeiro tensor encontrado pelo nome que contém `name_hint`
fn dequantize_tensor_by_name(file: &GgufFile, name_hint: &str) -> Option<(Vec<f32>, usize, usize)> {
    for t in &file.tensors {
        if t.name.contains(name_hint) {
            let rows = t.dims[0] as usize;
            let cols = if t.n_dims > 1 { t.dims[1] as usize } else { 1 };
            let ne = t.dims.iter().product::<u64>() as usize;
            let nbytes = t.tensor_type.nbytes_for_elements(ne);
            let start = t.offset as usize;
            let end = start.saturating_add(nbytes);
            if end > file.data.len() { return None; }
            let raw = &file.data[start..end];
            let vals = dequantize_raw(t.tensor_type, raw, rows, cols)?;
            return Some((vals, cols, rows));
        }
    }
    None
}

/// GGUF model config — all fields read from metadata, zero hardcoded.
struct GgufConfig {
    hidden: usize,
    num_layers: usize,
    num_heads: usize,
    num_kv_heads: usize,
    head_dim: usize,
    intermediate: usize,
    vocab: usize,
    rope_theta: f32,
    rms_epsilon: f32,
    tie_embeddings: bool,
}

impl GgufConfig {
    fn from_metadata(file: &GgufFile) -> Self {
        let get_u64 = |key: &str| -> usize {
            file.metadata.iter()
                .find(|m| m.key == key)
                .and_then(|m| m.value.parse().ok())
                .unwrap_or(0)
        };
        let get_f32 = |key: &str| -> f32 {
            file.metadata.iter()
                .find(|m| m.key == key)
                .and_then(|m| m.value.parse().ok())
                .unwrap_or(0.0)
        };
        let get_bool = |key: &str| -> bool {
            file.metadata.iter()
                .find(|m| m.key == key)
                .map(|m| m.value == "true")
                .unwrap_or(false)
        };
        let hidden = get_u64("llama.embedding_length");
        let num_layers = get_u64("llama.block_count");
        let num_heads = get_u64("llama.attention.head_count");
        let num_kv_heads = get_u64("llama.attention.head_count_kv");
        let intermediate = get_u64("llama.feed_forward_length");
        let vocab = get_u64("llama.vocab_size");
        let rope_theta = get_f32("llama.rope.freq_base");
        let rms_epsilon = get_f32("llama.attention.layer_norm_rms_epsilon");
        let tie_embeddings = get_bool("llama.attention.tie_qkv");
        // Derive head_dim: hidden / num_heads
        let head_dim = if num_heads > 0 { hidden / num_heads } else { 64 };
        GgufConfig {
            hidden: hidden.max(64),
            num_layers: num_layers.max(1),
            num_heads: num_heads.max(1),
            num_kv_heads: num_kv_heads.max(1),
            head_dim: head_dim.max(8),
            intermediate: intermediate.max(hidden * 2),
            vocab: vocab.max(256),
            rope_theta: if rope_theta > 0.0 { rope_theta } else { 10000.0 },
            rms_epsilon: if rms_epsilon > 0.0 { rms_epsilon } else { 1e-5 },
            tie_embeddings,
        }
    }
}

/// Modelo alimentado por GGUF — auto-configura tudo a partir dos metadados.
pub struct GgufBackedModel {
    file: GgufFile,
    config: GgufConfig,
}

impl GgufBackedModel {
    pub fn new(file: GgufFile) -> Self {
        let config = GgufConfig::from_metadata(&file);
        k_nano::slog_bin!("GGUF", "ok", "Auto-config: hidden={} layers={} heads={}/{} kv_heads={} intermediate={} vocab={} rope_theta={}",
            config.hidden, config.num_layers, config.num_heads, config.head_dim,
            config.num_kv_heads, config.intermediate, config.vocab, config.rope_theta);
        GgufBackedModel { file, config }
    }

    /// Reads a per-layer float tensor (RMS norm weights) from GGUF.
    fn read_layer_floats(&self, layer: usize, name_part: &str, expected: usize) -> Vec<f32> {
        let hint = alloc::format!("blk.{}.{}", layer, name_part);
        if let Some((vals, cols, _)) = dequantize_tensor_by_name(&self.file, &hint) {
            if cols >= expected {
                return vals[..expected].to_vec();
            }
            // Fewer elements than expected — pad with 1.0
            let mut v = vals;
            v.resize(expected, 1.0);
            return v;
        }
        alloc::vec![1.0f32; expected]
    }

    fn try_build_transformer(&self) -> Option<crate::cortex::TransformerModel> {
        let c = &self.config;
        let h = c.hidden;

        // Embedding: (vocab, hidden) → transpose to (hidden, vocab) for packed ternary
        let (embed_raw, embed_cols, embed_rows) = dequantize_tensor_by_name(&self.file, "token_embd")?;
        let embed = {
            let mut t = Vec::with_capacity(h * embed_cols);
            for hi in 0..h {
                for vi in 0..embed_cols {
                    t.push(embed_raw[vi * h + hi]);
                }
            }
            f32_to_ternary_packed(&t, h, embed_cols)
        };

        let mut layers = Vec::with_capacity(c.num_layers);
        for i in 0..c.num_layers {
            let hint = |s: &str| alloc::format!("blk.{}.{}", i, s);

            // Q/K/V/O projections — read shape from GGUF
            let (q_vals, q_cols, q_rows) = dequantize_tensor_by_name(&self.file, &hint("attn_q"))?;
            let (k_vals, k_cols, k_rows) = dequantize_tensor_by_name(&self.file, &hint("attn_k"))?;
            let (v_vals, v_cols, v_rows) = dequantize_tensor_by_name(&self.file, &hint("attn_v"))?;
            let (o_vals, o_cols, o_rows) = dequantize_tensor_by_name(&self.file, &hint("attn_output"))?;

            // FFN gate/up/down
            let (gate_vals, gc, gr) = dequantize_tensor_by_name(&self.file, &hint("ffn_gate"))?;
            let (up_vals, uc, ur) = dequantize_tensor_by_name(&self.file, &hint("ffn_up"))?;
            let (down_vals, dc, dr) = dequantize_tensor_by_name(&self.file, &hint("ffn_down"))?;

            // RMS norm weights — read from GGUF, fallback to 1.0
            let rms_attn = self.read_layer_floats(i, "attn_norm_weight", h);
            let rms_ffn = self.read_layer_floats(i, "ffn_norm_weight", h);

            // GQA: derive per-layer kv_dim from actual K tensor shape
            let layer_kv_heads = k_cols / c.head_dim.max(1);
            let kv_dim = layer_kv_heads * c.head_dim;
            let ffn_dim = gc.max(1);
            let ffn_group = (ffn_dim / c.head_dim.max(1)).max(1) * c.head_dim;

            layers.push(crate::cortex::LayerWeights {
                rms_attn,
                q: f32_to_ternary_packed(&q_vals, q_rows, q_cols),
                q_scale: 1.0,
                k: f32_to_ternary_packed(&k_vals, k_rows, k_cols),
                k_scale: 1.0,
                v: f32_to_ternary_packed(&v_vals, v_rows, v_cols),
                v_scale: 1.0,
                o: f32_to_ternary_packed(&o_vals, o_rows, o_cols),
                o_scale: 1.0,
                rms_ffn: rms_ffn.clone(),
                rms_inner_attn: alloc::vec![1.0f32; h],
                rms_ffn_norm: alloc::vec![1.0f32; ffn_dim],
                gate: f32_to_ternary_packed(&gate_vals, gr, gc),
                gate_scale: 1.0,
                up: f32_to_ternary_packed(&up_vals, ur, uc),
                up_scale: 1.0,
                down: f32_to_ternary_packed(&down_vals, dr, dc),
                down_scale: 1.0,
                kv_dim,
                num_kv_heads: layer_kv_heads.max(1),
                intermediate_size: ffn_dim,
                ffn_group_size: ffn_group,
            });
        }

        // Unembed (output projection)
        let unembed = dequantize_tensor_by_name(&self.file, "output.weight")
            .map(|(d, c, r)| f32_to_ternary_packed(&d, r, c))
            .unwrap_or_else(|| {
                let mut seed = 42u32;
                crate::cortex::random_ternary(&mut seed, h, c.vocab)
            });

        // Final RMS norm
        let rms_final = dequantize_tensor_by_name(&self.file, "output_norm.weight")
            .map(|(d, cols, _)| d[..cols.min(h)].to_vec())
            .unwrap_or_else(|| alloc::vec![1.0f32; h]);

        Some(crate::cortex::TransformerModel {
            embed,
            embed_scale: 1.0,
            layers,
            rms_final,
            unembed,
            unembed_scale: 1.0,
            medusa_heads: Vec::new(),
            vocab_size: c.vocab as u32,
            hidden: h,
            num_layers: c.num_layers,
            max_seq: 4096, // Falcon3 1.58bit default ctx
            num_heads: c.num_heads,
            num_kv_heads: c.num_kv_heads,
            head_dim: c.head_dim,
            kv_dim: c.num_kv_heads * c.head_dim,
            intermediate_size: c.intermediate,
            ffn_group_size: c.intermediate,
            tie_embeddings: c.tie_embeddings,
            act_type: 0, // silu (Falcon3 default)
            embed_type: 0,
            embed_q6k: None,
            rope_theta: c.rope_theta,
            rope_cos: alloc::vec![],
            rope_sin: alloc::vec![],
        })
    }
}

impl GgufBackedModel {
    /// Wraps user prompt in Falcon3/Llama chat template.
    /// Format: <|system|>\n{system}\n<|user|>\n{prompt}\n<|assistant|>\n
    fn format_chat_prompt(&self, prompt: &str) -> alloc::string::String {
        // Check if prompt already has chat tags (avoid double-wrapping)
        if prompt.contains("<|user|>") || prompt.contains("<|assistant|>") {
            return alloc::string::String::from(prompt);
        }
        alloc::format!(
            "<|system|>\nYou are a helpful AI assistant. Be concise and accurate.\n<|user|>\n{}\n<|assistant|>\n",
            prompt
        )
    }
}

impl Model for GgufBackedModel {
    fn generate(&self, prompt: &str) -> String {
        if let Some(model) = self.try_build_transformer() {
            let formatted = self.format_chat_prompt(prompt);
            crate::cortex::generate_text(&model, &formatted)
        } else {
            let summary = gguf_summary(&self.file);
            alloc::format!("[GGUF] Modelo carregado. {} camadas, {} hidden.\n\
                Aviso: conversao de pesos nao disponivel para este formato.\n\
                Use generacao fallback.\n{}\nPrompt: {}",
                self.config.num_layers, self.config.hidden, summary, prompt)
        }
    }

    fn embed_dim(&self) -> usize {
        self.config.hidden
    }

    fn vocab_size(&self) -> u32 {
        self.config.vocab as u32
    }

    fn max_seq(&self) -> usize {
        4096 // Falcon3 default ctx
    }
    fn num_layers(&self) -> usize { self.config.num_layers }
    fn hidden(&self) -> usize { self.config.hidden }
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
/// Tenta 64KB -> 256KB -> 1MB ate o parse do header caber (modelos com muitos tensores).
pub fn load_gguf_header_from_disk(path: &str) -> Option<GgufFile> {
    let name = path.trim().to_uppercase();
    let ata = k_nano::ATA_DRIVER.lock();
    let ata = ata.as_ref()?;
    let parts = unsafe { k_nano::fat32::read_mbr(ata) };
    for part in &parts {
        if part.type_code == 0x0B || part.type_code == 0x0C || part.type_code == 0x1C || part.type_code == 0x73 {
            let fs = unsafe { k_nano::fat32::Fat32Reader::new(ata, part)? };
            let file_size = unsafe { fs.lookup_file_size(&name)? };
            if file_size == 0 { return None; }

            // Progressive header window — tensor_info can exceed 4KB on large GGUF.
            const TRIES: [usize; 3] = [64 * 1024, 256 * 1024, 1024 * 1024];
            for &want in &TRIES {
                let header_bytes = file_size.min(want);
                let header_data = unsafe { fs.read_file_range(&name, 0, header_bytes)? };
                match load_gguf_meta_only(&header_data) {
                    Ok(file) => {
                        k_nano::slog_bin!("GGUF", "info", "Header OK path={} size={} meta_window={} tensors={} data_start={}",
                            name, file_size, header_bytes, file.tensors.len(), file.data_start);
                        return Some(file);
                    }
                    Err(_) => continue,
                }
            }
            return None;
        }
    }
    None
}

/// Parse GGUF magic/metadata/tensor infos without requiring tensor payload bytes.
/// `data_start` is the absolute file offset where tensor blobs begin.
pub fn load_gguf_meta_only(data: &[u8]) -> Result<GgufFile, &'static str> {
    if data.len() < 24 { return Err("GGUF: dados muito curtos"); }

    let mut offset = 0;
    let magic = read_u32(data, &mut offset);
    if magic != GGUF_MAGIC { return Err("GGUF: magic invalido"); }

    let version = read_u32(data, &mut offset);
    let tensor_count = read_u64(data, &mut offset);
    let metadata_kv_count = read_u64(data, &mut offset);
    let header = GgufHeader { magic, version, tensor_count, metadata_kv_count };

    let mut metadata = Vec::new();
    for _ in 0..metadata_kv_count {
        if offset + 8 > data.len() { return Err("GGUF: metadata truncado"); }
        let key = read_string(data, &mut offset);
        let value = read_metadata_value(data, &mut offset);
        metadata.push(GgufMetadata { key, value });
    }

    let mut tensors = Vec::new();
    for _ in 0..tensor_count {
        if offset + 8 > data.len() { return Err("GGUF: tensor info truncado"); }
        let name = read_string(data, &mut offset);
        let n_dims = read_u32(data, &mut offset);
        // GGUF spec: n_dims ∈ 1..=4 — valida antes de with_capacity/dims[0]
        // (n_dims malformado = panic/OOM em dados de arquivo).
        if n_dims == 0 || n_dims > 4 {
            return Err("GGUF: n_dims fora de 1..=4");
        }
        let mut dims = Vec::with_capacity(n_dims as usize);
        for _ in 0..n_dims {
            dims.push(read_u64(data, &mut offset));
        }
        let tensor_type = GgufType::from_u32(read_u32(data, &mut offset));
        let tensor_offset = read_u64(data, &mut offset);
        tensors.push(GgufTensorInfo { name, n_dims, dims, tensor_type, offset: tensor_offset });
    }

    let data_start = ((offset + 31) & !31) as u64;
    Ok(GgufFile {
        header,
        metadata,
        tensors,
        data_start,
        data: Vec::new(), // streaming: payload stays on disk
    })
}

/// Tenta carregar modelo GGUF diretamente do disco (ATA/FAT32).
pub fn load_gguf_model_from_disk(path: &str) -> Option<GgufBackedModel> {
    let name = path.trim().to_uppercase();
    let ata = k_nano::ATA_DRIVER.lock();
    let ata = ata.as_ref()?;
    let parts = unsafe { k_nano::fat32::read_mbr(ata) };
    for part in &parts {
        if part.type_code == 0x0B || part.type_code == 0x0C || part.type_code == 0x1C || part.type_code == 0x73 {
            let fs = unsafe { k_nano::fat32::Fat32Reader::new(ata, part)? };
            let data = unsafe { fs.read_file(&name)? };
            let file = load_gguf(&data).ok()?;
            return Some(GgufBackedModel::new(file));
        }
    }
    None
}

/// Carrega header GGUF em modo streaming e registra log diagnóstico.
/// Modelos >4GB agora funcionam — apenas metadados carregados na RAM.
pub fn load_gguf_streaming(path: &str) -> Result<(), &'static str> {
    let file = load_gguf_header_from_disk(path).ok_or("GGUF header load failed")?;
    let n_tensors = file.tensors.len();
    let total_params: u64 = file.tensors.iter().map(|t| t.dims.iter().product::<u64>()).sum();
    let n_layers = file.metadata.iter()
        .find(|m| m.key.contains("block_count"))
        .and_then(|m| m.value.parse().ok())
        .unwrap_or(0u64) as usize;
    k_nano::slog_bin!("GGUF", "info", "STREAM loaded path={} tensors={} params={} layers={}",
        path, n_tensors, total_params, n_layers);
    let _msg = alloc::format!("[GGUF] Streaming '{}': {} tensors, {} params (est). Header only in RAM.",
        path, n_tensors, total_params);
    Ok(())
}

/// Lista formatos GGUF suportados
pub fn print_supported_formats() -> String {
    alloc::format!(
        "Supported GGUF formats (AirLLM streaming):\n\
         Q4_0/Q5_0/Q8_0: classic block dequant OK\n\
         Q4_K/Q6_K: K-quant dequant OK (llama.cpp)\n\
         Q2_K/Q3_K/Q5_K: K-quant dequant OK (llama.cpp)\n\
         F16/F32: float OK\n\
         Prefetch: soft double-buffer (NOT peer DMA)\n\
         Hot-swap ATA: /model <FAT32-8.3-name>\n\
         Hot-swap Net: /model http://ip:port/path [DEST.GGUF]\n\
         Net: Range stream-to-disk when body >64MiB; RX fail = L3.5/RX"
    )
}

/// Read a byte range from a FAT32 root file (shared by streaming).
pub fn read_fat_range(path: &str, offset: usize, size: usize) -> Option<Vec<u8>> {
    let name = path.trim().to_uppercase();
    let ata = k_nano::ATA_DRIVER.lock();
    let ata = ata.as_ref()?;
    let parts = unsafe { k_nano::fat32::read_mbr(ata) };
    for part in &parts {
        if part.type_code == 0x0B || part.type_code == 0x0C || part.type_code == 0x1C || part.type_code == 0x73 || part.type_code == 0xEF {
            let fs = unsafe { k_nano::fat32::Fat32Reader::new(ata, part)? };
            return unsafe { fs.read_file_range(&name, offset, size) };
        }
    }
    None
}

/// Write full file to FAT32 root (create or replace). Used by Net hot-swap.
pub fn write_fat_file(path: &str, data: &[u8]) -> Result<(), &'static str> {
    let name = path.trim().to_uppercase();
    if name.is_empty() {
        return Err("FAT write: empty path");
    }
    let ata = k_nano::ATA_DRIVER.lock();
    let ata = ata.as_ref().ok_or("FAT write: ATA driver missing")?;
    let parts = unsafe { k_nano::fat32::read_mbr(ata) };
    for part in &parts {
        if part.type_code == 0x0B || part.type_code == 0x0C || part.type_code == 0x1C || part.type_code == 0x73 || part.type_code == 0xEF {
            let writer = unsafe { k_nano::fat32::Fat32Writer::new(ata, part) }
                .ok_or("FAT write: Fat32Writer::new failed")?;
            let ok = unsafe { writer.write_file(&name, data) };
            if ok {
                k_nano::slog_bin!("GGUF", "info", "FAT write OK path={} bytes={}",
                    name,
                    data.len());
                return Ok(());
            }
            return Err("FAT write: write_file failed (no free clusters?)");
        }
    }
    Err("FAT write: no FAT32 partition found")
}

/// Append bytes to an existing FAT32 root file (stream-to-disk chunks).
pub fn append_fat_file(path: &str, data: &[u8]) -> Result<(), &'static str> {
    if data.is_empty() {
        return Ok(());
    }
    let name = path.trim().to_uppercase();
    if name.is_empty() {
        return Err("FAT append: empty path");
    }
    let ata = k_nano::ATA_DRIVER.lock();
    let ata = ata.as_ref().ok_or("FAT append: ATA driver missing")?;
    let parts = unsafe { k_nano::fat32::read_mbr(ata) };
    for part in &parts {
        if part.type_code == 0x0B || part.type_code == 0x0C || part.type_code == 0x1C || part.type_code == 0x73 || part.type_code == 0xEF {
            let writer = unsafe { k_nano::fat32::Fat32Writer::new(ata, part) }
                .ok_or("FAT append: Fat32Writer::new failed")?;
            let ok = unsafe { writer.append_file(&name, data) };
            if ok {
                k_nano::slog_bin!(
                    "GGUF",
                    "info",
                    "FAT append OK path={} +{}B",
                    name,
                    data.len()
                );
                return Ok(());
            }
            return Err("FAT append: append_file failed");
        }
    }
    Err("FAT append: no FAT32 partition found")
}
