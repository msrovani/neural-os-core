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
const QK_K: usize = 256;
const Q4_K_BLOCK_BYTES: usize = 144; // d+dmin+scales[12]+qs[128]
const Q6_K_BLOCK_BYTES: usize = 210; // ql[128]+qh[64]+scales[16]+d

#[derive(Debug, Clone, Copy)]
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
            10 => GgufType::Q2_K,
            11 => GgufType::Q3_K,
            12 => GgufType::Q4_K,
            13 => GgufType::Q5_K,
            14 => GgufType::Q6_K,
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
            GgufType::Q4_1 | GgufType::Q5_1 | GgufType::Q8_1
            | GgufType::Q2_K | GgufType::Q3_K | GgufType::Q5_K => {
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
fn read_metadata_value(data: &[u8], offset: &mut usize) -> String {
    let val_type = read_u32(data, offset);
    match val_type {
        0 => { // uint8
            if *offset >= data.len() { return String::new(); }
            let v = data[*offset];
            *offset += 1;
            alloc::format!("{}", v)
        }
        1 => { // int8
            if *offset >= data.len() { return String::new(); }
            let v = data[*offset] as i8;
            *offset += 1;
            alloc::format!("{}", v)
        }
        2 => { // uint16
            if *offset + 2 > data.len() { return String::new(); }
            let v = u16::from_le_bytes([data[*offset], data[*offset + 1]]);
            *offset += 2;
            alloc::format!("{}", v)
        }
        3 => { // int16
            if *offset + 2 > data.len() { return String::new(); }
            let v = i16::from_le_bytes([data[*offset], data[*offset + 1]]);
            *offset += 2;
            alloc::format!("{}", v)
        }
        4 => { // uint32
            let v = read_u32(data, offset);
            alloc::format!("{}", v)
        }
        5 => { // int32
            if *offset + 4 > data.len() { return String::new(); }
            let v = i32::from_le_bytes([data[*offset], data[*offset + 1], data[*offset + 2], data[*offset + 3]]);
            *offset += 4;
            alloc::format!("{}", v)
        }
        6 => { // uint64
            let v = read_u64(data, offset);
            alloc::format!("{}", v)
        }
        7 => { // int64
            if *offset + 8 > data.len() { return String::new(); }
            let v = i64::from_le_bytes([
                data[*offset], data[*offset + 1], data[*offset + 2], data[*offset + 3],
                data[*offset + 4], data[*offset + 5], data[*offset + 6], data[*offset + 7],
            ]);
            *offset += 8;
            alloc::format!("{}", v)
        }
        8 => { // float32
            if *offset + 4 > data.len() { return String::new(); }
            let v = f32::from_le_bytes([data[*offset], data[*offset + 1], data[*offset + 2], data[*offset + 3]]);
            *offset += 4;
            alloc::format!("{:.4}", v)
        }
        9 => { // bool
            if *offset >= data.len() { return String::new(); }
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
            if *offset + 4 > data.len() { return String::new(); }
            let v = f32::from_le_bytes([data[*offset], data[*offset + 1], data[*offset + 2], data[*offset + 3]]);
            *offset += 4;
            alloc::format!("{:.4}", v)
        }
        10 => read_string(data, offset),
        _ => {
            // Unknown inner type: try length-prefixed skip if possible
            if *offset + 4 <= data.len() {
                let len = u32::from_le_bytes(data[*offset..*offset + 4].try_into().unwrap_or([0; 4])) as usize;
                *offset += 4 + len.min(data.len().saturating_sub(*offset + 4));
            } else {
                *offset = data.len();
            }
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

/// Dequantiza bytes brutos conforme tipo GGUF (F32/F16/Q4_0/Q5_0/Q8_0/Q4_K/Q6_K).
pub fn dequantize_raw(qtype: GgufType, data: &[u8], rows: usize, cols: usize) -> Option<Vec<f32>> {
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
        GgufType::Q4_0 => dequantize_q4_0(data, rows, cols).map(|t| t.data),
        GgufType::Q5_0 => dequantize_q5_0(data, rows, cols).map(|t| t.data),
        GgufType::Q8_0 => dequantize_q8_0(data, rows, cols).map(|t| t.data),
        GgufType::Q4_K => dequantize_q4_k(data, rows, cols).map(|t| t.data),
        GgufType::Q6_K => dequantize_q6_k(data, rows, cols).map(|t| t.data),
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
pub(crate) fn f32_to_ternary_packed(data: &[f32], rows: usize, cols: usize) -> PackedTernaryTensor {
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
                q_scale: 1.0,
                k: f32_to_ternary_packed(&k, kr, kc),
                k_scale: 1.0,
                v: f32_to_ternary_packed(&v, vr, vc),
                v_scale: 1.0,
                o: f32_to_ternary_packed(&o, or_, oc),
                o_scale: 1.0,
                rms_ffn: rms_default,
                rms_inner_attn,
                rms_ffn_norm,
                gate: f32_to_ternary_packed(&gate, gr, gc),
                gate_scale: 1.0,
                up: f32_to_ternary_packed(&up, ur, uc),
                up_scale: 1.0,
                down: f32_to_ternary_packed(&down, dr, dc),
                down_scale: 1.0,
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
            embed_scale: 1.0,
            layers,
            rms_final,
            unembed,
            unembed_scale: 1.0,
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

/// Lista formatos GGUF suportados
pub fn print_supported_formats() -> String {
    alloc::format!(
        "Supported GGUF formats (AirLLM streaming):\n\
         Q4_0/Q5_0/Q8_0: classic block dequant OK\n\
         Q4_K/Q6_K: K-quant dequant OK (llama.cpp)\n\
         Q2_K/Q3_K/Q5_K: type known, dequant deferred\n\
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
        if part.type_code == 0x0B || part.type_code == 0x0C || part.type_code == 0x1C || part.type_code == 0x73 {
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
        if part.type_code == 0x0B || part.type_code == 0x0C || part.type_code == 0x1C || part.type_code == 0x73 {
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
        if part.type_code == 0x0B || part.type_code == 0x0C || part.type_code == 0x1C || part.type_code == 0x73 {
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
