//! Dynamic layer offloader — n_gpu_layers a partir de VRAM livre.

/// Estima camadas GPU: `vram_free - kv - scratch` cobre `layers * mb_per_layer`.
pub fn plan_n_gpu_layers(
    total_layers: u32,
    mb_per_layer: u64,
    vram_free_mb: u64,
    kv_mb: u64,
    scratch_mb: u64,
    reserve_mb: u64,
) -> u32 {
    if total_layers == 0 || mb_per_layer == 0 {
        return 0;
    }
    let budget = vram_free_mb
        .saturating_sub(kv_mb)
        .saturating_sub(scratch_mb)
        .saturating_sub(reserve_mb);
    let n = (budget / mb_per_layer) as u32;
    n.min(total_layers)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_when_no_vram() {
        assert_eq!(plan_n_gpu_layers(32, 100, 0, 50, 50, 64), 0);
    }

    #[test]
    fn fills_budget() {
        // 2000 - 200 - 100 - 64 = 1636 / 100 = 16
        assert_eq!(plan_n_gpu_layers(32, 100, 2000, 200, 100, 64), 16);
    }
}
