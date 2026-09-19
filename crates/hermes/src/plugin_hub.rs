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
    /// ADR-0106 M2: Score tipado (`typed_sites::decide_plugin_risk`).
    pub fn scan(&mut self, name: &str, wasm_bytes: &[u8]) -> PluginScan {
        let (veredict, details) = crate::typed_sites::plugin_verdict(wasm_bytes);
        if details.is_empty() && matches!(veredict, ScanVerdict::Safe) {
            // keep empty details for Safe
        }
        let scan = PluginScan {
            name: String::from(name),
            veredict,
            details,
        };
        self.plugins.insert(String::from(name), scan.clone());
        let tag = match veredict {
            ScanVerdict::Safe => "SAFE",
            ScanVerdict::Suspicious => "SUSPICIOUS",
            ScanVerdict::Blocked => "BLOCKED",
        };
        k_nano::slog_hermes!("PLUGIN", "ok", "[{}] VERDICT={} (ADR-0106)", name, tag);
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
