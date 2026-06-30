//! GGUF (GGML Universal Format) loader — no_std puro.
//! Suporta parse de header, metadata, tensor info.
//! Q4_0/Q8_0 dequantization para alimentar o BitNet pipeline.
//!
//! Formato: https://github.com/ggerganov/ggml/blob/master/docs/gguf.md

use alloc::string::String;
use alloc::vec::Vec;
use crate::tensor::Tensor;
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
