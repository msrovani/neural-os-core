// E1a: gguf_streaming stays in bin (net-dependent). Crate re-export removed.

/// True if path looks like http://host:port/path
pub fn is_http_model_spec(path: &str) -> bool {
    path.starts_with("http://") || path.starts_with("https://") || path.starts_with("tcp://")
}

/// Stub: net hot-swap not wired yet (AirLLM / net fs path)
pub fn hot_swap_from_net(_path: &str) -> Result<alloc::string::String, alloc::string::String> {
    Err(alloc::format!("[MODEL] Net hot-swap not implemented in this build"))
}

/// Stub: ATA hot-swap not wired yet (AirLLM / streaming GGUF)
pub fn hot_swap_from_ata(_path: &str) -> Result<(), alloc::string::String> {
    Err(alloc::format!("[MODEL] ATA hot-swap not implemented in this build"))
}

pub fn log_airllm_residuals() {
    // stub: AirLLM path requires bin-specific networking
}
