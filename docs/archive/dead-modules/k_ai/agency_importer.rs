//! Compatibilidade do importer: seleciona divisões importadas do seed gerado.

use alloc::vec::Vec;

use crate::agency::{Agency, Division};

pub fn import_divisions() -> Vec<Division> {
    Agency::new()
        .divisions
        .into_iter()
        .filter(|division| division.name.ends_with("-imported"))
        .collect()
}
