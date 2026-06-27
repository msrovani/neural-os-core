//! CDC Rabin Chunking — content-defined chunk split via rolling hash.
//! Útil para dividir bitmaps, logs e snapshots em chunks de tamanho variável
//! baseados no conteúdo, não em posição fixa.
//!
//! Algoritmo: polinômio Rabin de 64 bits sobre janela deslizante de 8 bytes.
//! Um chunk termina quando os N bits inferiores do hash são zero.

use alloc::vec::Vec;

/// Tamanho mínimo e máximo de chunk para evitar chunks microscópicos ou gigantes
const CHUNK_MIN: usize = 64;
const CHUNK_MAX: usize = 4096;
const CHUNK_MASK: u64 = 0x3F; // 6 bits → chunk médio de ~64 bytes

/// Polinômio Rabin (irredutível de 64 bits)
const POLY: u64 = 0x0000_0000_0000_001B;
const INIT_WINDOW: u64 = 0xB168_9B41_7A2F_CD05;

/// Rolling hash: atualiza o hash ao deslizar um byte para fora
/// e um byte para dentro da janela.
fn rabin_hash(curr: u64, out_byte: u8, in_byte: u8) -> u64 {
    let mut h = curr.wrapping_sub((out_byte as u64) << 56);
    h = h.wrapping_mul(POLY);
    h ^= in_byte as u64;
    h
}

/// Inicializa o hash sobre uma janela de 8 bytes
fn rabin_init(window: &[u8]) -> u64 {
    let mut h = INIT_WINDOW;
    for &b in window {
        h ^= b as u64;
        h = h.wrapping_mul(POLY);
    }
    h
}

/// Divide o buffer em chunks baseados no conteúdo (Rabin fingerprint).
/// Cada chunk é um `Vec<u8>` que pode ser processado independentemente.
pub fn chunk_data(data: &[u8]) -> Vec<Vec<u8>> {
    let mut chunks = Vec::new();
    if data.is_empty() { return chunks; }

    let mut chunk_start = 0;

    // Primeira janela
    while chunk_start < data.len() {
        let end = core::cmp::min(chunk_start + CHUNK_MAX, data.len());
        let mut hash = if data.len() - chunk_start >= 8 {
            rabin_init(&data[chunk_start..chunk_start + 8])
        } else {
            rabin_init(&data[chunk_start..end])
        };

        let mut chunk_end = chunk_start;
        let search_end = if data.len() >= chunk_start + CHUNK_MAX {
            chunk_start + CHUNK_MAX - 8
        } else {
            chunk_start
        };

        while chunk_end + 8 < search_end {
            hash = rabin_hash(hash, data[chunk_end], data[chunk_end + 8]);
            chunk_end += 1;
            if hash & CHUNK_MASK == 0 && (chunk_end - chunk_start) >= CHUNK_MIN {
                break;
            }
        }

        let actual_end = core::cmp::min(chunk_end + 8, data.len());
        let mut chunk = Vec::with_capacity(actual_end - chunk_start);
        chunk.extend_from_slice(&data[chunk_start..actual_end]);
        chunks.push(chunk);
        chunk_start = actual_end;
    }

    chunks
}

/// Recombina chunks em ordem → dados originais
pub fn merge_chunks(chunks: &[Vec<u8>]) -> Vec<u8> {
    let total: usize = chunks.iter().map(|c| c.len()).sum();
    let mut out = Vec::with_capacity(total);
    for chunk in chunks {
        out.extend_from_slice(chunk);
    }
    out
}

#[cfg(test)]
mod test {
    use super::*;
    #[test]
    fn roundtrip() {
        let data = (0..4096).map(|i| (i % 251) as u8).collect::<Vec<_>>();
        let chunks = chunk_data(&data);
        let merged = merge_chunks(&chunks);
        assert_eq!(data, merged);
    }
}
