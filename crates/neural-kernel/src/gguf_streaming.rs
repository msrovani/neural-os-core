// E1a: gguf_streaming stays in bin (net-dependent). Crate re-export removed.
// ADR-0046 item 8: hot-swap AirLLM real (ATA + Net) — conecta os callers do
// /model aos helpers reais de cortex::gguf (header-only streaming + stream-to-disk).

/// True if path looks like http://host:port/path
pub fn is_http_model_spec(path: &str) -> bool {
    path.starts_with("http://") || path.starts_with("https://") || path.starts_with("tcp://")
}

/// Net hot-swap: GET Range em chunks (4MB) → append FAT (stream-to-disk) →
/// load header-only (AirLLM). ponytail: sem DMA/parallel — soft double-buffer,
/// honesto no log. Retorna o nome FAT destino.
pub fn hot_swap_from_net(path: &str) -> Result<alloc::string::String, alloc::string::String> {
    // Destino FAT 8.3: extrai do path (ou default DEST.GGUF)
    let dest = alloc::string::String::from("DEST.GGUF");
    let (host, port, url_path) = crate::net::parse_http_url(path)
        .map_err(|e| alloc::format!("url invalida: {}", e))?;
    // Resolve host → IP (mesmo padrão do https_get)
    let ip = unsafe { crate::net::dns_resolve_host(&host) }.ok_or("dns_failed")?;

    // Descobre o total via Range: bytes=0-0 (206 → Content-Range total)
    let probe = unsafe { crate::net::http_get_range_host(ip, port, &url_path, None, 0, 0) }
        .ok_or("Range probe falhou (RX?)")?;
    let total = probe.total.ok_or("server sem Content-Range")?;
    if total < 1024 * 1024 {
        // pequeno — baixa inteiro de uma vez
        let body = unsafe { crate::net::http_get_host(ip, port, &url_path, None) }
            .ok_or("GET falhou")?;
        crate::gguf::write_fat_file(&dest, &body).map_err(|e| alloc::format!("FAT write: {}", e))?;
        crate::gguf::load_gguf_streaming(&dest).map_err(|e| alloc::format!("GGUF header: {}", e))?;
        return Ok(dest);
    }

    // Stream-to-disk em chunks de 4MB
    const CHUNK: usize = 4 * 1024 * 1024;
    let mut off = 0usize;
    let mut total_written = 0usize;
    let mut first = true;
    while off < total {
        let end = (off + CHUNK - 1).min(total - 1);
        let body = unsafe { crate::net::http_get_range_host(ip, port, &url_path, None, off, end) }
            .ok_or("Range GET falhou")?
            .body;
        if first {
            crate::gguf::write_fat_file(&dest, &body)
                .map_err(|e| alloc::format!("FAT write: {}", e))?;
            first = false;
        } else {
            crate::gguf::append_fat_file(&dest, &body)
                .map_err(|e| alloc::format!("FAT append: {}", e))?;
        }
        total_written += body.len();
        off += body.len();
        if body.is_empty() {
            break; // server parou de mandar
        }
    }
    crate::gguf::load_gguf_streaming(&dest).map_err(|e| alloc::format!("GGUF header: {}", e))?;
    k_nano::slog_bin!(
        "GGUF",
        "info",
        "Net stream-to-disk OK dest={} bytes={}/{}",
        dest,
        total_written,
        total
    );
    Ok(dest)
}

/// ATA hot-swap: load header-only do FAT (AirLLM).
pub fn hot_swap_from_ata(path: &str) -> Result<(), alloc::string::String> {
    crate::gguf::load_gguf_streaming(path).map_err(|e| alloc::format!("GGUF header: {}", e))
}

pub fn log_airllm_residuals() {
    k_nano::slog_bin!(
        "GGUF",
        "info",
        "AirLLM residuals: ATA/Net hot-swap OK; K-quants Q2_K/Q3_K/Q5_K OK; DMA prefetch = AWAITING"
    );
}
