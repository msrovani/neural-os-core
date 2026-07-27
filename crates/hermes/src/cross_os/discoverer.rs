//! CrossOsDiscoverer — busca skills WASM em multiplas fontes em runtime.
//! Pesquisa PackageHub local, marketplaces P2P, GitHub, crates.io.
//! AIOS na veia: descobre solucoes sozinho, nao espera o desenvolvedor.

use alloc::string::String;
use alloc::vec::Vec;
use super::intent::{IntentCategory, IntentResult};

/// Resultado de uma busca por skill.
#[derive(Debug, Clone)]
pub struct SkillCandidate {
    pub name: String,
    pub description: String,
    pub source: SkillSource,
    pub format: SkillFormat,
    pub confidence: f32, // 0.0 a 1.0 — quao bem atende a necessidade
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillSource {
    PackageHub,    // Ja registrado localmente
    FyyMarketplace,// Ecossistema FYY P2P
    WeftOS,        // Ecossistema WeftOS
    GitHub,        // Codigo aberto para compilar
    CratesIO,      // Crate Rust para portar
    Generated,     // Gerado por IA sob demanda
    Legacy,        // App legacy (PE/ELF) como ultimo recurso
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillFormat {
    Wasm,  // Skill WASM pronta para executar
    Wasi,  // WASI module
    WAT,   // Precisa compilar WAT -> WASM
    Rust,  // Codigo Rust fonte, precisa compilar
    Exe,   // Executavel Windows (.exe)
    Elf,   // Executavel Linux (ELF)
}

/// Resultado consolidado da descoberta.
pub struct DiscoverResult {
    pub candidates: Vec<SkillCandidate>,
    pub has_wasm: bool,     // Tem pelo menos uma skill WASM pronta
    pub best: Option<SkillCandidate>,
}

/// Buscador de skills em multiplas fontes.
pub struct CrossOsDiscoverer;

impl CrossOsDiscoverer {
    /// Busca skills para atender a intencao do usuario.
    /// Consulta PackageHub primeiro, depois fontes externas.
    pub fn discover(intent: &IntentResult) -> DiscoverResult {
        let mut candidates = Vec::new();
        let query = Self::build_search_query(intent);

        // Fonte 1: PackageHub local (ja implementado)
        Self::search_package_hub(&query, &mut candidates);

        // Fonte 2: FYY marketplace via MCP
        Self::search_fyy(&query, &mut candidates);

        // Fonte 3: GitHub / codigo aberto
        Self::search_github(&query, &mut candidates);

        let has_wasm = candidates.iter().any(|c| c.format == SkillFormat::Wasm);
        let best = candidates.first().cloned();

        DiscoverResult { candidates, has_wasm, best }
    }

    fn build_search_query(intent: &IntentResult) -> String {
        match intent.category {
            IntentCategory::OfficeSpreadsheet => String::from("spreadsheet xlsx excel wasm"),
            IntentCategory::OfficeDocument => String::from("document docx word wasm"),
            IntentCategory::OfficePresentation => String::from("presentation pptx wasm"),
            IntentCategory::Pdf => String::from("pdf extract convert wasm"),
            IntentCategory::Image => String::from("image edit png jpg wasm"),
            IntentCategory::Code => String::from("code compile script wasm"),
            _ => String::from("wasm skill"),
        }
    }

    fn search_package_hub(query: &str, candidates: &mut Vec<SkillCandidate>) {
        // PackageHub.list() retorna todos os pacotes registrados
        let hub = crate::package_hub::PACKAGE_HUB.lock();
        let packages = hub.list(None);
        for pkg in packages {
            if pkg.name.contains(query) || query.contains(&pkg.name) {
                candidates.push(SkillCandidate {
                    name: pkg.name.clone(),
                    description: alloc::format!("PackageKind={:?}", pkg.kind),
                    source: SkillSource::PackageHub,
                    format: SkillFormat::Wasm,
                    confidence: 1.0,
                });
            }
        }
        drop(hub);
    }

    fn search_fyy(query: &str, candidates: &mut Vec<SkillCandidate>) {
        // FYY marketplace via MCP — stub para integracao futura
        // TODO: chamar MCP server para consultar FYY mesh
        // Por enquanto, retorna candidatos conhecidos da pesquisa
        if query.contains("spreadsheet") || query.contains("xlsx") || query.contains("excel") {
            candidates.push(SkillCandidate {
                name: String::from("office2pdf"),
                description: String::from("Converte DOCX/XLSX/PPTX para PDF. Compila para WASM. Zero dependencias externas."),
                source: SkillSource::GitHub,
                format: SkillFormat::Wasm,
                confidence: 0.9,
            });
            candidates.push(SkillCandidate {
                name: String::from("CasualOffice"),
                description: String::from("Engine Rust para ler/escrever DOCX, ODT, PDF, MD. WASM bindings."),
                source: SkillSource::CratesIO,
                format: SkillFormat::Rust,
                confidence: 0.8,
            });
        }
        if query.contains("pdf") {
            candidates.push(SkillCandidate {
                name: String::from("gigapdf-lib"),
                description: String::from("Engine PDF zero-dependencia. PDF <-> Office/HTML/RTF. Compila WASM (5.6MB)."),
                source: SkillSource::GitHub,
                format: SkillFormat::Wasm,
                confidence: 0.85,
            });
            candidates.push(SkillCandidate {
                name: String::from("AgenticPDF"),
                description: String::from("PDF engine para AI agents. WASM + MCP server. Reading-order Markdown, tabelas, OCR."),
                source: SkillSource::GitHub,
                format: SkillFormat::Wasm,
                confidence: 0.85,
            });
        }
    }

    fn search_github(query: &str, candidates: &mut Vec<SkillCandidate>) {
        // GitHub search stub — em runtime, consultaria API do GitHub
        // TODO: conectar ao NetAgent para consultar GitHub API
        let _ = query;
    }
}
