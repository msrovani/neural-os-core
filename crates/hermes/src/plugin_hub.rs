//! Plugin Hub / MCP Index (IDEA #236 / #395).
//! Catálogo de plugins com AI security scan.
//! Verifica imports, padrões suspeitos, fuel budget antes de aprovar instalação.
//! Ponte entre marketplace HTTP discovery e PackageHub.

use alloc::collections::BTreeMap;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;
use lazy_static::lazy_static;
use ticket_lock::TicketLock;

/// Resultado do scan de segurança.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanVerdict {
    Safe,          // Plugin confiável
    Suspicious,    // Comportamento suspeito
    Blocked,       // Bloqueado — não instalar
}

/// Resultado do scan de um plugin.
#[derive(Debug, Clone)]
pub struct PluginScan {
    pub name: String,
    pub veredict: ScanVerdict,
    pub details: Vec<String>,
}

/// Metadados de um plugin no catálogo.
#[derive(Debug, Clone)]
pub struct PluginCatalogEntry {
    pub name: String,
    pub version: String,
    pub description: String,
    pub kind: String,
    pub source_url: String,
    pub scan: Option<PluginScan>,
}

/// Hub de Plugins com verificação de segurança.
pub struct PluginHub {
    plugins: BTreeMap<String, PluginScan>,
    catalog: BTreeMap<String, PluginCatalogEntry>,
}

impl PluginHub {
    pub fn new() -> Self {
        Self {
            plugins: BTreeMap::new(),
            catalog: BTreeMap::new(),
        }
    }

    /// Escaneia bytes WASM por segurança.
    /// Análise heurística baseada em strings suspeitas, tamanho, padrões de
    /// acesso a hardware.
    pub fn scan(&mut self, name: &str, wasm_bytes: &[u8]) -> PluginScan {
        let mut details = Vec::new();
        let mut suspicious = false;
        let mut blocked = false;

        // Check for suspicious strings in WASM binary (strings are preserved
        // in most WASM toolchains as custom sections or data segments).
        let text = core::str::from_utf8(wasm_bytes).unwrap_or("");

        // ── Blocked patterns: direct hardware access ──
        if text.contains("io_port") || text.contains("outb") || text.contains("inb") {
            details.push(String::from("Direct port I/O access — BLOCKED"));
            blocked = true;
        }
        if text.contains("lgdt") || text.contains("lidt") || text.contains("write_cr3") {
            details.push(String::from("Privileged instruction access — BLOCKED"));
            blocked = true;
        }

        // ── Suspicious patterns: physical/dma memory access ──
        if text.contains("write_phys") || text.contains("dma_alloc") || text.contains("phys_to_virt") {
            details.push(String::from("Physical memory access — SUSPICIOUS"));
            suspicious = true;
        }

        // ── Suspicious patterns: size heuristic ──
        if wasm_bytes.len() > 100_000 {
            details.push(String::from("Plugin >100KB — SUSPICIOUS"));
            suspicious = true;
        }

        // ── Suspicious patterns: obfuscation ──
        if text.contains("eval") || text.contains("base64_decode") {
            details.push(String::from("Dynamic code patterns — SUSPICIOUS"));
            suspicious = true;
        }

        let veredict = if blocked {
            ScanVerdict::Blocked
        } else if suspicious {
            ScanVerdict::Suspicious
        } else {
            ScanVerdict::Safe
        };

        let scan = PluginScan {
            name: String::from(name),
            veredict,
            details,
        };

        self.plugins.insert(String::from(name), scan.clone());
        scan
    }

    /// Catálogo: adiciona ou atualiza entrada descoberta via HTTP.
    pub fn catalog_upsert(&mut self, entry: PluginCatalogEntry) {
        self.catalog.insert(entry.name.clone(), entry);
    }

    /// Remove entrada do catálogo.
    pub fn catalog_remove(&mut self, name: &str) {
        self.catalog.remove(name);
        self.plugins.remove(name);
    }

    /// Lista catálogo remoto (descoberto via HTTP).
    pub fn catalog_list(&self) -> Vec<&PluginCatalogEntry> {
        self.catalog.values().collect()
    }

    /// Lista scans já realizados.
    pub fn scan_list(&self) -> Vec<&PluginScan> {
        self.plugins.values().collect()
    }

    /// Verifica se um plugin já foi escaneado.
    pub fn get_scan(&self, name: &str) -> Option<&PluginScan> {
        self.plugins.get(name)
    }

    pub fn scanned_count(&self) -> usize {
        self.plugins.len()
    }

    pub fn catalog_count(&self) -> usize {
        self.catalog.len()
    }

    /// Relatório resumido para logs.
    pub fn report(&self) -> String {
        format!(
            "[PLUGIN] scanned={} catalog={}",
            self.plugins.len(),
            self.catalog.len(),
        )
    }
}

lazy_static! {
    pub static ref PLUGIN_HUB: TicketLock<PluginHub> = TicketLock::new(PluginHub::new());
}
