//! QMD v01_07 (PASCAL_COMPUTE_B / NVC1C0) — bitfields oficiais `clc1c0qmd.h`.
//!
//! Degrau 4: monta o descriptor 256 B; não prova execução (fence/golden = silício).

/// Tamanho canônico do QMD Pascal.
pub const QMD_SIZE: usize = 256;

/// Parâmetros mínimos para vector_add canário.
pub struct QmdLaunch {
    pub program_offset: u32,
    pub grid: (u32, u32, u32),
    pub block: (u32, u32, u32),
    pub shared_mem: u32,
    pub register_count: u32,
    pub barrier_count: u32,
    pub sass_version: u32,
    pub cb0_iova: u64,
    pub cb0_size: u32,
    pub fence_iova: u64,
    pub fence_payload: u32,
}

impl QmdLaunch {
    /// Defaults seguros para canário n=4, 1 CTA × 4 threads.
    pub fn vector_add_canary(cb0_iova: u64, fence_iova: u64) -> Self {
        Self {
            program_offset: 0,
            grid: (1, 1, 1),
            block: (4, 1, 1),
            shared_mem: 0,
            register_count: 16,
            barrier_count: 1,
            sass_version: 0x61, // sm_61
            cb0_iova,
            cb0_size: 256,
            fence_iova,
            fence_payload: 1,
        }
    }
}

/// Escreve campo MW(hi:lo) no QMD (bit hi = MSB, lo = LSB).
fn set_field(qmd: &mut [u8; QMD_SIZE], hi: u32, lo: u32, mut value: u64) {
    debug_assert!(hi >= lo && hi < (QMD_SIZE as u32) * 8);
    let width = hi - lo + 1;
    let mask = if width == 64 {
        u64::MAX
    } else {
        (1u64 << width) - 1
    };
    value &= mask;
    for bit in lo..=hi {
        let byte_idx = (bit / 8) as usize;
        let bit_idx = (bit % 8) as u8;
        let src = ((value >> (bit - lo)) & 1) as u8;
        if src != 0 {
            qmd[byte_idx] |= 1 << bit_idx;
        } else {
            qmd[byte_idx] &= !(1 << bit_idx);
        }
    }
}

/// Constrói QMD v01_07 zerado + campos canário.
pub fn build_qmd_v01_07(launch: &QmdLaunch) -> [u8; QMD_SIZE] {
    let mut q = [0u8; QMD_SIZE];

    // SM_GLOBAL_CACHING_ENABLE MW(198:198)
    set_field(&mut q, 198, 198, 1);
    // SEMAPHORE_RELEASE_ENABLE0 MW(202:202)
    set_field(&mut q, 202, 202, 1);
    // API_VISIBLE_CALL_LIMIT = NO_CHECK MW(378:378)
    set_field(&mut q, 378, 378, 1);

    // PROGRAM_OFFSET MW(287:256)
    set_field(&mut q, 287, 256, launch.program_offset as u64);

    // CTA_RASTER_*
    set_field(&mut q, 415, 384, launch.grid.0 as u64);
    set_field(&mut q, 431, 416, launch.grid.1 as u64);
    set_field(&mut q, 447, 432, launch.grid.2 as u64);

    // SHARED_MEMORY_SIZE MW(561:544)
    set_field(&mut q, 561, 544, launch.shared_mem as u64);
    // QMD_VERSION=7, MAJOR=1
    set_field(&mut q, 579, 576, 7);
    set_field(&mut q, 583, 580, 1);

    // CTA_THREAD_DIMENSION*
    set_field(&mut q, 607, 592, launch.block.0 as u64);
    set_field(&mut q, 623, 608, launch.block.1 as u64);
    set_field(&mut q, 639, 624, launch.block.2 as u64);

    // CONSTANT_BUFFER_VALID(0) MW(640:640)
    set_field(&mut q, 640, 640, 1);

    // RELEASE0 address + payload + ONE_WORD structure
    set_field(&mut q, 767, 736, launch.fence_iova & 0xFFFF_FFFF);
    set_field(&mut q, 775, 768, (launch.fence_iova >> 32) & 0xFF);
    set_field(&mut q, 799, 799, 1); // STRUCTURE_SIZE_ONE_WORD
    set_field(&mut q, 831, 800, launch.fence_payload as u64);

    // CONSTANT_BUFFER_ADDR(0) + SIZE — slot i=0 @ MW(959:928) lower, (967:960) upper, (991:975) size
    set_field(&mut q, 959, 928, launch.cb0_iova & 0xFFFF_FFFF);
    set_field(&mut q, 967, 960, (launch.cb0_iova >> 32) & 0xFF);
    set_field(&mut q, 991, 975, launch.cb0_size as u64);

    // BARRIER_COUNT MW(1471:1467)
    set_field(&mut q, 1471, 1467, launch.barrier_count as u64);
    // REGISTER_COUNT MW(1503:1496)
    set_field(&mut q, 1503, 1496, launch.register_count as u64);
    // SASS_VERSION MW(1535:1528)
    set_field(&mut q, 1535, 1528, launch.sass_version as u64);

    q
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qmd_version_bits() {
        let q = build_qmd_v01_07(&QmdLaunch::vector_add_canary(0x1000, 0x2000));
        // Word containing bits 576..583 is around byte 72
        let mut ver = 0u64;
        for bit in 576..=583 {
            let b = q[(bit / 8) as usize];
            if (b >> (bit % 8)) & 1 != 0 {
                ver |= 1 << (bit - 576);
            }
        }
        assert_eq!(ver & 0xF, 7);
        assert_eq!((ver >> 4) & 0xF, 1);
    }
}
