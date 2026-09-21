//! Poll com budget TSC (SESSION_354/373). Contagem de spins mente em TCG/WHPX.

/// Espera `pred` até `budget_us`. Sem TSC calibrado: cap de spins = min(budget, 1e6).
pub fn until(budget_us: u64, mut pred: impl FnMut() -> bool) -> bool {
    if k_nano::tsc::tsc_hz() != 0 {
        let t0 = k_nano::tsc::now_us();
        loop {
            if pred() {
                return true;
            }
            if k_nano::tsc::now_us().saturating_sub(t0) > budget_us {
                return false;
            }
            core::hint::spin_loop();
        }
    }
    let spins = budget_us.min(1_000_000);
    for _ in 0..spins {
        if pred() {
            return true;
        }
        core::hint::spin_loop();
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn until_true_immediately() {
        assert!(until(1_000, || true));
    }

    #[test]
    fn until_false_on_timeout() {
        assert!(!until(1, || false));
    }
}
