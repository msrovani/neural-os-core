//! Prefetch de hardware e Streaming Stores (non-temporal writes).
//!
//! # Prefetch predictivo
//! Para operações massivas de tensores (DMA disco→Cortex Arena, inference loops),
//! `_mm_prefetch` instrui o hardware a carregar dados para a L1 Cache antes
//! de serem acessados, eliminando cache misses em patterns previsíveis.
//!
//! # Streaming Stores (MOVNTDQ)
//! Para gravar blocos de tensores resultantes da inferência, `MOVNTDQ` escreve
//! diretamente na RAM via write-combining buffer, **sem poluir L1/L2/L3**.
//! Essencial para DMA writes e outputs de inference onde os dados não serão
//! reutilizados imediatamente.
//!
//! # Ambas as operações são no-ops em targets não-x86_64.

/// Prefetch de dados para L1 Cache (T0 = todas as caches).
///
/// # Safety
/// O ponteiro deve ser acessível (não precisa ser válido, mas não deve
/// ultrapassar limites alocados). O prefetch é uma dica — o hardware
/// pode ignorá-lo.
///
/// # Exemplo
/// ```no_run
/// // Prefetch do próximo bloco de 64 bytes
/// unsafe { prefetch_l1(tensor_ptr.add(64) as *const u8); }
/// ```
#[inline(always)]
pub unsafe fn prefetch_l1(ptr: *const u8) {
    #[cfg(target_arch = "x86_64")]
    {
        core::arch::x86_64::_mm_prefetch(ptr as *const i8, core::arch::x86_64::_MM_HINT_T0);
    }
    let _ = ptr; // no-op em targets não-x86
}

/// Prefetch para L2 Cache (T1 = L2 e abaixo, não L1).
///
/// Útil para dados que serão acessados depois do bloco atual
/// mas não imediatamente (double-buffering).
#[inline(always)]
pub unsafe fn prefetch_l2(ptr: *const u8) {
    #[cfg(target_arch = "x86_64")]
    {
        core::arch::x86_64::_mm_prefetch(ptr as *const i8, core::arch::x86_64::_MM_HINT_T1);
    }
    let _ = ptr;
}

/// Prefetch para L3/Non-temporal (T2 = somente L3, não L1/L2).
///
/// Para dados que serão reutilizados em远 future (warm-up do cache).
#[inline(always)]
pub unsafe fn prefetch_l3(ptr: *const u8) {
    #[cfg(target_arch = "x86_64")]
    {
        core::arch::x86_64::_mm_prefetch(ptr as *const i8, core::arch::x86_64::_MM_HINT_T2);
    }
    let _ = ptr;
}

/// Double-buffer prefetch: prefetch do próximo bloco enquanto processa o atual.
///
/// # Safety
/// `current` e `next` devem ser ponteiros para blocos de pelo menos `block_size` bytes.
#[inline(always)]
pub unsafe fn prefetch_double_buffer(current: *const u8, next: *const u8, block_size: usize) {
    // Prefetch do próximo bloco (T0 = L1)
    prefetch_l1(next);
    // touches no bloco atual para garantir que está em cache
    let _ = core::ptr::read_volatile(current);
    let _ = block_size;
}

// ─── Streaming Stores (non-temporal writes) ──────────────────────────────

/// Streaming store de 512 bits (64 bytes) — escreve diretamente na RAM
/// via write-combining buffer, sem sujar L1/L2/L3.
///
/// **Útil para:** outputs de inference, DMA writes, cópias de grandes blocos.
/// **NÃO usar para:** dados que serão reutilizados em < ~100 acessos
/// (write-combining é lento para re-leitura).
///
/// # Safety
/// `dst` deve ser alinhado a 64 bytes e apontar para pelo menos 64 bytes de memória.
#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "avx512f")]
pub unsafe fn stream_write_512(dst: *mut u64, src: core::arch::x86_64::__m512i) {
    core::arch::x86_64::_mm512_stream_si512(dst as *mut core::arch::x86_64::__m512i, src);
}

/// Streaming store genérico: copia `len` bytes de `src` para `dst` usando
/// MOVNTI/MOVNTDQ (non-temporal), processando em blocos de 64 bytes.
///
/// O bloco restante (se `len % 64 != 0`) é copiado com `memcpy` regular.
///
/// # Safety
/// `src` e `dst` devem apontar para pelo menos `len` bytes.
/// `dst` deve ser alinhado a 64 bytes para máximo throughput.
pub unsafe fn streaming_copy(dst: *mut u8, src: *const u8, len: usize) {
    let mut written = 0usize;

    // Blocos de 64 bytes via MOVNTDQ (se AVX-512 disponível)
    #[cfg(target_arch = "x86_64")]
    {
        if crate::platform_probe::allow_avx512() {
            let chunks = len / 64;
            for i in 0..chunks {
                let offset = i * 64;
                let data = core::arch::x86_64::_mm512_loadu_si512(src.add(offset) as *const _);
                stream_write_512(dst.add(offset) as *mut u64, data);
            }
            written = chunks * 64;
        }
    }

    // Cauda: memcpy regular
    if written < len {
        let remain = len - written;
        core::ptr::copy_nonoverlapping(src.add(written), dst.add(written), remain);
    }

    // SFENCE: garante que todas as non-temporal stores foram visíveis
    // antes de qualquer leitura subsequente em dst.
    #[cfg(target_arch = "x86_64")]
    {
        core::arch::x86_64::_mm_sfence();
    }
}

/// Streaming copy com prefetch interleaved: prefetch do próximo chunk
/// enquanto escreve o chunk atual.
///
/// Reduz latência em loops de cópia de tensores grandes (> 4KB).
pub unsafe fn streaming_copy_prefetched(dst: *mut u8, src: *const u8, len: usize) {
    const CHUNK: usize = 4096; // 4KB chunks

    let mut offset = 0;
    while offset + CHUNK <= len {
        // Prefetch do próximo chunk
        if offset + CHUNK * 2 <= len {
            prefetch_l1(src.add(offset + CHUNK));
        }
        // Streaming copy do chunk atual
        streaming_copy(dst.add(offset), src.add(offset), CHUNK);
        offset += CHUNK;
    }

    // Cauda
    if offset < len {
        streaming_copy(dst.add(offset), src.add(offset), len - offset);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaming_copy_matches_memcpy() {
        let src = [0xABu8; 256];
        let mut dst = [0u8; 256];

        unsafe {
            streaming_copy(dst.as_mut_ptr(), src.as_ptr(), 256);
        }

        assert_eq!(dst, src);
    }

    #[test]
    fn streaming_copy_non_aligned() {
        let src = [0xCDu8; 100]; // não múltiplo de 64
        let mut dst = [0u8; 100];

        unsafe {
            streaming_copy(dst.as_mut_ptr(), src.as_ptr(), 100);
        }

        assert_eq!(dst, src);
    }

    #[test]
    fn streaming_copy_zero_len() {
        let src = [1u8; 10];
        let mut dst = [0u8; 10];

        unsafe {
            streaming_copy(dst.as_mut_ptr(), src.as_ptr(), 0);
        }

        assert_eq!(dst, [0u8; 10]); // unchanged
    }
}
