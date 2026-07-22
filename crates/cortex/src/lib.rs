#![no_std]
#![allow(dead_code)]
#![allow(static_mut_refs)]
#![allow(unused_unsafe)]

extern crate alloc;

// ─── cortex: BitNet Engine ───
// Pure neural inference engine: tensor ops, transformer layers, tokenization
// No hardware dependencies beyond memory allocation.
// Shared register map type used by HW identification (generate_register_map)

#[derive(Clone, Copy, Debug)]
pub struct HardwareRegisterMap {
    pub tx_ring_low: usize,
    pub rx_ring_low: usize,
    pub rx_control: usize,
    pub doorbell_tx: usize,
    pub doorbell_rx: usize,
    pub cmd_start_rx: u32,
    pub ring_size: usize,
    pub rx_buf_len: usize,
}

pub mod arena;
pub mod global_arena;
pub mod r3;
pub mod bitnet_avx2;
pub mod compute;
pub mod decode;
pub mod bpe;
pub mod burn_flex;
pub mod cortex;
pub mod delta;
pub mod model_hub;
pub mod model_fit;
pub mod nn;
pub mod parallel_matmul;
pub mod tensor;
pub mod trinity;
pub mod tv_dsl;
pub mod ngram_spec;
pub mod projection;
pub mod neuos_probe;
pub mod kv_h2o;
// ponytail: pre-existing compile errors from F4/MoE skeleton (untracked, not BEI)
// pub mod structured_decode;
// pub mod moe;
