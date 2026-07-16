//! AIOS API — Sprint 106-8
//! Bibliotecas internas (aios_net, aios_fs) expostas como system prompt / RAG
//! para agentes Python (MicroPython WASM) e skills gerados pelo Cortex.

use alloc::string::String;
use alloc::vec::Vec;

/// Documentação embutida da API de rede do AIOS.
pub const AIOS_NET_DOCS: &str = "\
# aios_net — Rede bare-metal (smoltcp)
- aios_net.http_get(url: str) -> str
- aios_net.dns_resolve(host: str) -> str
- aios_net.dhcp_status() -> dict
- aios_net.tcp_connect(host: str, port: int) -> socket
";

/// Documentação embutida da API de filesystem do AIOS.
pub const AIOS_FS_DOCS: &str = "\
# aios_fs — Virtual File System (agentes FS)
- aios_fs.read(path: str) -> bytes
- aios_fs.write(path: str, data: bytes) -> bool
- aios_fs.list(path: str) -> list[str]
- Mounts: /ata, /ram, /proc, /dev, /hermes, /inference
";

/// System prompt completo para agentes Python no AIOS.
pub fn build_system_prompt(user_context: &str) -> String {
    let mut prompt = String::from(
        "You are an AIOS agent running on Neural OS Hermes (bare-metal Rust, no_std).\n\
         Use only the APIs below. Do not assume POSIX or Linux.\n\n",
    );
    prompt.push_str(AIOS_NET_DOCS);
    prompt.push('\n');
    prompt.push_str(AIOS_FS_DOCS);
    if !user_context.is_empty() {
        prompt.push_str("\n# Context\n");
        prompt.push_str(user_context);
    }
    prompt
}

/// Injeta fragmentos RAG relevantes ao intent do usuário.
pub fn rag_inject(intent: &str) -> Vec<String> {
    let lower = intent.to_ascii_lowercase();
    let mut chunks = Vec::new();
    if lower.contains("http") || lower.contains("rede") || lower.contains("wifi")
        || lower.contains("dns") || lower.contains("download")
    {
        chunks.push(String::from(AIOS_NET_DOCS));
    }
    if lower.contains("arquivo") || lower.contains("file") || lower.contains("ler")
        || lower.contains("escrever") || lower.contains("fat32") || lower.contains("vfs")
    {
        chunks.push(String::from(AIOS_FS_DOCS));
    }
    if chunks.is_empty() {
        chunks.push(build_system_prompt(""));
    }
    chunks
}

/// Wrapper Python-like: leitura VFS via hermes globals.
pub fn aios_fs_read(path: &str) -> Result<Vec<u8>, &'static str> {
    crate::globals::read_vfs(path)
}

/// Wrapper Python-like: escrita VFS.
pub fn aios_fs_write(path: &str, data: &[u8]) -> Result<(), &'static str> {
    crate::globals::write_vfs(path, data)
}

/// Wrapper Python-like: HTTP GET via BrowserAgent path (retorna stub se offline).
pub fn aios_net_http_get(url: &str) -> Result<String, &'static str> {
    let _ = url;
    // Integração completa via BrowserAgent no monólito; stub seguro para crate isolado.
    Err("aios_net.http_get: requer kernel runtime (BrowserAgent)")
}
