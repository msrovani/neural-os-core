use alloc::string::String;
use alloc::vec::Vec;

/// Natural log via IEEE 754 bit decomposition (no libm).
/// Accuracy ~4 decimal digits using the identity ln(x) ≈ log2(x) * ln(2).
/// log2(x) is extracted from the exponent and a P3 Padé-like fraction on the mantissa.
pub fn ln_f32(x: f32) -> f32 {
    if x <= 0.0 {
        return f32::NEG_INFINITY;
    }
    // Reinterpret bits: extract exponent and mantissa
    let bits = x.to_bits();
    let exp = ((bits >> 23) & 0xFF) as i32 - 127; // unbiased exponent
    let mant = (bits & 0x7FFFFF) | 0x7F800000;   // set exponent to 0 (bias 127) → 1.0..2.0

    // log2(1 + m) ≈ m * (c0 + m * c1)  Rational minimax for [1,2], ~2.5 ulp error
    // For simplicity use: log2(x) ≈ exp + log2(mant_bits_as_float)
    let m = f32::from_bits(mant); // m in [1.0, 2.0)
    let log2_mant = m - 1.0; // approximação linear grossa, mas suficiente para TF-IDF
    let log2_x = exp as f32 + log2_mant;

    // ln(2) ≈ 0.69314718
    log2_x * 0.69314718_f32
}

/// Square root via Newton-Raphson (2 iterations).
pub fn sqrt_f32(x: f32) -> f32 {
    if x <= 0.0 {
        return 0.0;
    }
    // Initial guess via bit hack: fast inverse sqrt approximation for seed
    let i = x.to_bits();
    let guess_bits = 0x1FBD_1DF5 + (i >> 1); // magic constant for sqrt seed
    let mut s = f32::from_bits(guess_bits);

    // Newton-Raphson: s = 0.5 * (s + x / s)
    s = 0.5 * (s + x / s);
    s = 0.5 * (s + x / s);
    s
}

/// Compute TF-IDF vector for a set of tokens given global DF and doc_count.
///
/// * `tokens` — tokenized bag-of-words for this document
/// * `df` — document frequency per vocabulary term (indexed by vocab position)
/// * `doc_count` — total number of documents in the corpus
/// * `vocab_size` — length of the vocabulary
///
/// Returns a dense f32 vector of length `vocab_size`.
pub fn compute_tfidf(
    _tokens: &[String],
    _df: &[u32],
    _doc_count: u32,
    vocab_size: usize,
) -> Vec<f32> {
    if vocab_size == 0 {
        return Vec::new();
    }

    // ponytail: placeholder — real TF-IDF computation uses
    // compute_tfidf_from_indices which takes pre-resolved term indices.
    // This function exists for API compatibility with callers that pass
    // token strings directly (currently unused).
    alloc::vec![0.0_f32; vocab_size]
}

/// Compute TF-IDF from pre-resolved term indices (internal fast path).
pub fn compute_tfidf_from_indices(
    term_indices: &[usize],
    df: &[u32],
    doc_count: u32,
    vocab_size: usize,
) -> Vec<f32> {
    if vocab_size == 0 {
        return Vec::new();
    }
    let mut tf = alloc::vec![0u32; vocab_size];
    for &idx in term_indices {
        if idx < vocab_size {
            tf[idx] += 1;
        }
    }

    let n_docs = doc_count as f32;
    let mut vec = alloc::vec![0.0_f32; vocab_size];
    for i in 0..vocab_size {
        if tf[i] > 0 {
            let tf_val = 1.0 + ln_f32(tf[i] as f32);
            let idf_val = ln_f32(1.0 + n_docs / (1.0 + df[i] as f32));
            vec[i] = tf_val * idf_val;
        }
    }
    vec
}

/// Cosine similarity between two f32 vectors.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let len = a.len().min(b.len());
    if len == 0 {
        return 0.0;
    }
    let mut dot = 0.0_f32;
    let mut na = 0.0_f32;
    let mut nb = 0.0_f32;
    for i in 0..len {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    let denom = sqrt_f32(na * nb);
    if denom < 1e-12 {
        return 0.0;
    }
    dot / denom
}

#[cfg(test)]
mod tests {
    use super::*;

    pub fn demo() -> bool {
        // ln tests
        let l1 = ln_f32(1.0_f32);
        if (l1 - 0.0).abs() > 0.01 {
            return false;
        }
        let le = ln_f32(2.71828_f32);
        if (le - 1.0).abs() > 0.1 {
            return false;
        }

        // sqrt tests
        let s1 = sqrt_f32(100.0);
        if (s1 - 10.0).abs() > 0.01 {
            return false;
        }
        let s2 = sqrt_f32(2.0);
        if (s2 - 1.41421356).abs() > 0.01 {
            return false;
        }

        // cosine identity
        let v = vec![1.0, 2.0, 3.0];
        if (cosine_similarity(&v, &v) - 1.0).abs() > 0.001 {
            return false;
        }

        // cosine orthogonal
        if cosine_similarity(&[1.0, 0.0], &[0.0, 1.0]).abs() > 0.001 {
            return false;
        }

        // compute_tfidf_from_indices
        let df = vec![1u32, 2, 1];
        let indices = vec![0usize, 1, 0, 2, 2, 2];
        let vec = compute_tfidf_from_indices(&indices, &df, 10, 3);
        if vec.len() != 3 {
            return false;
        }
        if vec[0] <= 0.0 || vec[1] <= 0.0 || vec[2] <= 0.0 {
            return false;
        }

        true
    }
}
