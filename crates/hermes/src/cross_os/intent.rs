//! CrossOsIntent — analisa a intencao do usuario e classifica em categorias.
//! AIOS na veia: entende o que o usuario quer antes de buscar solucoes.

use alloc::string::String;

/// Categoria da necessidade do usuario.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntentCategory {
    OfficeSpreadsheet,  // Excel, planilha, xlsx, csv
    OfficeDocument,     // Word, docx, texto, rtf
    OfficePresentation, // PowerPoint, pptx, slides
    Pdf,                // PDF, extrair, converter
    Image,              // imagem, foto, editar, png, jpg
    Code,               // codigo, compilar, script
    Communication,      // email, mensagem, chat
    Data,               // dados, analise, banco
    Network,            // rede, download, upload
    System,             // configuracao, terminal
    Unknown,            // nao classificado
}

/// Resultado da analise de intencao.
#[derive(Debug, Clone)]
pub struct IntentResult {
    pub category: IntentCategory,
    pub keywords: String,
    pub wants_edit: bool,     // true se quer editar (nao so ler)
    pub wants_convert: bool,  // true se quer converter formato
    pub confidence: f32,      // 0.0 a 1.0
}

/// Analisador de intencao do usuario.
pub struct CrossOsIntent;

impl CrossOsIntent {
    /// Analisa o texto do usuario e classifica a intencao.
    pub fn analyze(text: &str) -> IntentResult {
        let lower = text.to_lowercase();
        let mut result = IntentResult {
            category: IntentCategory::Unknown,
            keywords: String::from(text),
            wants_edit: false,
            wants_convert: false,
            confidence: 0.0,
        };

        // Office Spreadsheet
        if lower.contains("excel")
            || lower.contains("planilha")
            || lower.contains("xlsx")
            || lower.contains("csv")
            || lower.contains("spreadsheet")
            || lower.contains("calc")
        {
            result.category = IntentCategory::OfficeSpreadsheet;
            result.confidence = 0.8;
        }
        // Office Document
        else if lower.contains("word")
            || lower.contains("documento")
            || lower.contains("docx")
            || lower.contains("texto")
            || lower.contains("doc")
        {
            result.category = IntentCategory::OfficeDocument;
            result.confidence = 0.8;
        }
        // Office Presentation
        else if lower.contains("powerpoint")
            || lower.contains("pptx")
            || lower.contains("slides")
            || lower.contains("apresentacao")
        {
            result.category = IntentCategory::OfficePresentation;
            result.confidence = 0.8;
        }
        // PDF
        else if lower.contains("pdf")
            || lower.contains("extrair")
            || lower.contains("converter")
        {
            result.category = IntentCategory::Pdf;
            result.confidence = 0.7;
            result.wants_convert = lower.contains("converter");
        }
        // Image
        else if lower.contains("imagem")
            || lower.contains("foto")
            || lower.contains("png")
            || lower.contains("jpg")
            || lower.contains("editar imagem")
        {
            result.category = IntentCategory::Image;
            result.confidence = 0.7;
        }
        // Code
        else if lower.contains("codigo")
            || lower.contains("compilar")
            || lower.contains("script")
            || lower.contains("programa")
        {
            result.category = IntentCategory::Code;
            result.confidence = 0.7;
        }

        result.wants_edit = lower.contains("editar")
            || lower.contains("criar")
            || lower.contains("modificar")
            || lower.contains("alterar");

        result
    }
}
