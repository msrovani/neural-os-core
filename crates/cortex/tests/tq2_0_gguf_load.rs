//! Integration test: load synthetic TQ2_0 GGUF and validate dequantization.
//!
//! The test GGUF (target/test_tq2_0.gguf) contains a single tensor:
//!   blk.0.attn_q.weight, shape [4,4], type TQ2_0 (GGUF type 25)
//!   scale=1.5, weights: +1,+1,-1,-1,0,0,+1,-1,+1,0,-1,+1,0,+1,-1,0

#[test]
fn load_tq2_0_gguf_synthetic() {
    let data = include_bytes!("../../../target/test_tq2_0.gguf");
    let file = cortex::gguf::load_gguf(data).expect("GGUF parse failed");

    assert_eq!(file.header.tensor_count, 1);
    assert_eq!(file.header.version, 3);

    let tensor = &file.tensors[0];
    assert_eq!(tensor.name, "blk.0.attn_q.weight");
    assert_eq!(tensor.n_dims, 2);
    assert_eq!(tensor.dims, vec![4, 4]);
    assert_eq!(tensor.tensor_type, cortex::gguf::GgufType::TQ2_0);

    // Dequantize
    let ne = 4 * 4;
    let nbytes = tensor.tensor_type.nbytes_for_elements(ne);
    let raw = &file.data[tensor.offset as usize..tensor.offset as usize + nbytes];
    let vals = cortex::gguf::dequantize_raw(tensor.tensor_type, raw, 4, 4)
        .expect("dequantize TQ2_0 failed");

    assert_eq!(vals.len(), 16);

    // Expected: scale=1.5, weights packed as +1,+1,-1,-1,0,0,+1,-1,+1,0,-1,+1,0,+1,-1,0
    let expected = [
        1.5, 1.5, -1.5, -1.5,
        0.0, 0.0, 1.5, -1.5,
        1.5, 0.0, -1.5, 1.5,
        0.0, 1.5, -1.5, 0.0,
    ];
    for (i, (got, want)) in vals.iter().zip(expected.iter()).enumerate() {
        assert!(
            (got - want).abs() < 1e-4,
            "TQ2_0[{}]: got {} want {}",
            i, got, want
        );
    }
}
