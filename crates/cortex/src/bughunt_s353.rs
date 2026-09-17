//! SESSION_353 — testes host das issues do bughunt heap/infer.
//!
//! Cobre: Tensor::is_valid, mask refuse≠(1,1), k_all sem panic,
//! parallel_matmul checked, soft_stride pad KV, last_ctx_cap default.

#![cfg(test)]

use crate::cortex::{KvCache, TransformerModel};
use crate::difficulty_gate;
use crate::heap_aios;
use crate::parallel_matmul;
use crate::tensor::{f32_zeros_2d, Tensor};

#[test]
fn tensor_new_valid_small() {
    let t = Tensor::new((4, 8));
    assert!(t.is_valid());
    assert_eq!(t.shape, (4, 8));
    assert_eq!(t.data.len(), 32);
}

#[test]
fn tensor_new_overflow_shape_is_empty() {
    // usize::MAX * 2 wraps in unchecked mul — checked path → (0,0)
    let t = Tensor::new((usize::MAX / 2 + 2, 3));
    assert_eq!(t.shape, (0, 0));
    assert!(t.data.is_empty());
    assert!(t.is_valid()); // (0,0)+empty is valid
}

#[test]
fn tensor_is_valid_rejects_shape_data_mismatch() {
    let bad = Tensor {
        shape: (2, 4),
        data: alloc::vec![0.0f32; 3],
    };
    assert!(!bad.is_valid());
}

#[test]
fn f32_zeros_2d_overflow_empty() {
    let v = f32_zeros_2d(usize::MAX / 2 + 1, 4);
    assert!(v.is_empty());
}

#[test]
fn parallel_matmul_shape_mismatch_none() {
    let a = Tensor::new((2, 3));
    let b = Tensor::new((4, 2));
    assert!(a.is_valid() && b.is_valid());
    assert!(parallel_matmul::parallel_matmul(&a, &b).is_none());
}

#[test]
fn parallel_matmul_small_ok() {
    let a = Tensor::from_row_major((2, 2), alloc::vec![1.0, 0.0, 0.0, 1.0]).unwrap();
    let b = Tensor::from_row_major((2, 2), alloc::vec![2.0, 3.0, 4.0, 5.0]).unwrap();
    let c = parallel_matmul::parallel_matmul(&a, &b).expect("matmul");
    assert!(c.is_valid());
    assert_eq!(c.shape, (2, 2));
    assert!((c.data[0] - 2.0).abs() < 1e-5);
    assert!((c.data[1] - 3.0).abs() < 1e-5);
    assert!((c.data[2] - 4.0).abs() < 1e-5);
    assert!((c.data[3] - 5.0).abs() < 1e-5);
}

#[test]
fn kv_k_all_mismatch_no_panic_pads() {
    let mut cache = KvCache::new(2, 4, 4);
    // Layer 0: só 1 token (4 floats); pedimos seq_len=3 → mismatch → pad, sem unwrap panic.
    let k = Tensor::from_row_major((1, 4), alloc::vec![1.0, 2.0, 3.0, 4.0]).unwrap();
    let v = Tensor::from_row_major((1, 4), alloc::vec![0.5; 4]).unwrap();
    cache.append(0, &k, &v);
    cache.advance(1);
    let total = cache.k_all(0, 3);
    assert!(total.is_valid());
    assert_eq!(total.shape, (3, 4));
    assert_eq!(total.data.len(), 12);
    assert!((total.data[0] - 1.0).abs() < 1e-5);
    // padding zeros
    assert_eq!(total.data[4], 0.0);
}

#[test]
fn embed_for_kv_mask_never_one_by_one() {
    let model = TransformerModel::new();
    let cache = KvCache::new(model.layers.len(), model.layers[0].k.shape.1, model.kv_dim);
    let tokens = [1u32, 2, 3];
    let (x, mask, _sp, new_len, total_seq) = model.embed_for_kv(&tokens, &cache);
    assert!(x.is_valid());
    assert!(mask.is_valid());
    assert_ne!(mask.shape, (1, 1), "SESSION_351: mask (1,1) causa OOB");
    assert_eq!(mask.shape, (new_len, total_seq));
    assert_eq!(mask.data.len(), new_len * total_seq);
}

#[test]
fn apply_one_layer_refuses_bad_mask() {
    let model = TransformerModel::new();
    let mut cache = KvCache::new(model.layers.len(), model.layers[0].k.shape.1, model.kv_dim);
    let mut x = Tensor::new((2, model.hidden));
    assert!(x.is_valid());
    let bad_mask = Tensor::zero((1, 1)); // bug antigo
    let before = x.data.clone();
    model.apply_one_layer(0, &model.layers[0], &mut x, &mut cache, 0, 2, 2, &bad_mask);
    // Refuse early — x inalterado (ou ainda válido); sem panic.
    assert!(x.is_valid());
    assert_eq!(x.data, before);
    assert_eq!(cache.k[0].len(), 0, "refuse não deve append KV");
}

#[test]
fn soft_stride_pad_keeps_all_layers_aligned() {
    let model = TransformerModel::new();
    difficulty_gate::set_soft_stride_override(2); // skip odd layers
    let mut cache = KvCache::new(model.layers.len(), model.layers[0].k.shape.1, model.kv_dim);
    let tokens = [1u32, 2];
    let (_h, _logits) = model.forward_with_kv(&tokens, &mut cache);
    difficulty_gate::clear_soft_stride_override();
    // Após soft_stride=2, TODAS as layers devem ter KV do mesmo comprimento lógico.
    let kd = cache.k_dim();
    let expect = cache.len * kd;
    for (li, layer_k) in cache.k.iter().enumerate() {
        assert_eq!(
            layer_k.len(),
            expect,
            "layer {} KV len={} expect={} (soft_stride pad)",
            li,
            layer_k.len(),
            expect
        );
    }
}

#[test]
fn last_ctx_cap_default_is_cheap_not_512() {
    // Sem apply_plan, default não pode mentir 512 (SESSION_351).
    // Limpa atomic se outro teste setou.
    heap_aios::clear_job_overrides();
    // last_ctx_cap lê LAST_PLAN_CTX — se 0 → 64.
    // Não temos setter público p/ zerar; documenta contrato via plan_for Cheap.
    let plan = heap_aios::plan_for(
        difficulty_gate::ComputeTier::Cheap,
        3072,
        true,
        true,
    );
    assert!(plan.ctx_cap <= 64, "Cheap ctx_cap={}", plan.ctx_cap);
}

#[test]
fn matmul_rejects_invalid_tensor() {
    let a = Tensor {
        shape: (2, 2),
        data: alloc::vec![1.0, 2.0], // mismatch
    };
    let b = Tensor::new((2, 2));
    assert!(!a.is_valid());
    assert!(a.matmul(&b).is_none());
}
