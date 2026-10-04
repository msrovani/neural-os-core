//! E4 (OPCODE-0098): estatística pura para o harness de bench — P50/P99,
//! média, desvio-padrão e filtro de amostras n/a (0). Sem dependências.
//!
//! Regra de honestidade: `0` = n/a (nunca conta como valor real) — uma série
//! vazia devolve `None`, jamais `0`.

use alloc::vec::Vec;

/// Remove amostras inválidas (`0` = n/a) preservando a ordem.
pub fn filter_valid(values: &[u64]) -> Vec<u64> {
    values.iter().copied().filter(|&v| v != 0).collect()
}

/// Percentil nearest-rank (`p` em 0..=100). `None` se vazio (n/a).
/// `p=50` = mediana; `p=99` = p99.
pub fn percentile(values: &[u64], p: u64) -> Option<u64> {
    let mut v = filter_valid(values);
    if v.is_empty() {
        return None;
    }
    v.sort_unstable();
    let n = v.len();
    let p = p.min(100) as usize;
    // nearest-rank: ceil(p/100 * n) - 1, clampado a [0, n-1].
    let rank = (p * n + 99) / 100;
    let idx = rank.saturating_sub(1).min(n - 1);
    Some(v[idx])
}

pub fn median(values: &[u64]) -> Option<u64> {
    percentile(values, 50)
}

pub fn p99(values: &[u64]) -> Option<u64> {
    percentile(values, 99)
}

/// Média inteira (truncada). `None` se vazio.
pub fn mean(values: &[u64]) -> Option<u64> {
    let v = filter_valid(values);
    if v.is_empty() {
        return None;
    }
    let sum: u128 = v.iter().map(|&x| x as u128).sum();
    Some((sum / v.len() as u128) as u64)
}

/// Desvio-padrão POPULACIONAL (inteiro, arredondado). `None` se vazio.
pub fn stddev(values: &[u64]) -> Option<u64> {
    let v = filter_valid(values);
    if v.is_empty() {
        return None;
    }
    let n = v.len() as u128;
    let sum: u128 = v.iter().map(|&x| x as u128).sum();
    let m = sum / n;
    let var = v
        .iter()
        .map(|&x| {
            let d = x as i128 - m as i128;
            (d * d) as u128
        })
        .sum::<u128>()
        / n;
    Some(isqrt_u64(var as u64))
}

/// Raiz quadrada inteira (Newton) — sem f64/libm (no_std core não tem `sqrt`).
fn isqrt_u64(n: u64) -> u64 {
    if n < 2 {
        return n;
    }
    let mut x = n;
    let mut y = (x + 1) / 2;
    while y < x {
        x = y;
        y = (x + n / x) / 2;
    }
    x
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn percentile_median_p99_and_na() {
        let v: Vec<u64> = (1..=100).collect();
        // nearest-rank: p50 de 1..100 = 50; p99 = 99.
        assert_eq!(median(&v), Some(50));
        assert_eq!(p99(&v), Some(99));

        // Vazio → None (n/a ≠ 0), nunca 0.
        assert_eq!(median(&[]), None);
        assert_eq!(p99(&[]), None);
        assert_eq!(mean(&[]), None);
        assert_eq!(stddev(&[]), None);

        // 0 = n/a: descartado; a série só-zeros vira vazia → None.
        let with_zeros = [0u64, 10, 0, 20, 0, 30];
        assert_eq!(filter_valid(&with_zeros), vec![10, 20, 30]);
        assert_eq!(median(&with_zeros), Some(20));
        assert_eq!(filter_valid(&[0u64, 0, 0]), Vec::<u64>::new());
        assert_eq!(median(&[0u64, 0, 0]), None);

        // mean/stddev conhecidos: [2,4,4,4,5,5,7,9] → mean 5, stddev 2.
        assert_eq!(mean(&[2, 4, 4, 4, 5, 5, 7, 9]), Some(5));
        assert_eq!(stddev(&[2, 4, 4, 4, 5, 5, 7, 9]), Some(2));

        // p100 = máximo; p0 = mínimo.
        assert_eq!(percentile(&v, 100), Some(100));
        assert_eq!(percentile(&v, 0), Some(1));
    }
}
