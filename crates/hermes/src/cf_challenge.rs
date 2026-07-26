//! Cloudflare challenge — Labor 47 honesty SKIP (solver pleno = residual).

pub fn try_cf_challenge(_html: &[u8]) -> Result<(), &'static str> {
    Err("cf_challenge_skip")
}

pub fn boot_smoke() {
    k_nano::slog_bin!(
        "HTTP",
        "info",
        "step=cf_challenge status=SKIP VERDICT=SKIP reason=honesty_no_solver (L47)"
    );
}





