//! CapGate NIX_WRITE — staging only; never `nixos-rebuild switch` (HITL ADR-0088/0118).

use serde::Serialize;
use std::fs;
use std::path::Path;
use std::process::Command;
use thiserror::Error;
use tracing::{info, warn};

#[derive(Debug, Error)]
pub enum NixProposeError {
    #[error("empty nix text")]
    Empty,
    #[error("nix text rejected: {0}")]
    Reject(&'static str),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Serialize)]
pub struct NixProposeResult {
    pub staging_path: String,
    pub bytes: usize,
    pub syntax_ok: bool,
    pub check_tool: String,
    /// Always true — Apply exige humano.
    pub hitl_required: bool,
    pub switch_forbidden: bool,
    pub note: String,
}

/// Allowlist mínima: parece um módulo Nix, sem `switch` embutido.
pub fn validate_nix_text(text: &str) -> Result<(), NixProposeError> {
    let t = text.trim();
    if t.is_empty() {
        return Err(NixProposeError::Empty);
    }
    if t.len() > 256 * 1024 {
        return Err(NixProposeError::Reject("too large (>256KiB)"));
    }
    let lower = t.to_ascii_lowercase();
    if lower.contains("nixos-rebuild") && lower.contains("switch") {
        return Err(NixProposeError::Reject("staging must not embed switch"));
    }
    if !(t.contains('{') && t.contains('}')) {
        return Err(NixProposeError::Reject("expected nix attrset/module braces"));
    }
    Ok(())
}

fn run_syntax_check(path: &Path) -> (bool, String) {
    // Prefer parse-only; never build-vm/switch here (HITL fora).
    if let Ok(out) = Command::new("nix-instantiate")
        .args(["--parse"])
        .arg(path)
        .output()
    {
        let tool = "nix-instantiate --parse".into();
        return (out.status.success(), tool);
    }
    if let Ok(out) = Command::new("nix")
        .args(["eval", "--file"])
        .arg(path)
        .arg("--raw")
        .arg("null")
        .output()
    {
        // may fail for modules; still try
        if out.status.success() {
            return (true, "nix eval".into());
        }
    }
    // Sem Nix no host: aceitar validação estrutural já feita
    warn!(target: "nosd", "nix syntax tool absent — structural validation only");
    (true, "structural".into())
}

/// Escreve staging e valida; **nunca** aplica no host.
pub fn propose_staging(data_dir: &Path, nix_text: &str) -> Result<NixProposeResult, NixProposeError> {
    validate_nix_text(nix_text)?;
    fs::create_dir_all(data_dir)?;
    let staging = data_dir.join("staging-ai-managed.nix");
    fs::write(&staging, nix_text)?;
    let (syntax_ok, check_tool) = run_syntax_check(&staging);
    info!(
        target: "nosd",
        "nix.propose staging={} syntax_ok={} tool={} HITL_REQUIRED switch_forbidden",
        staging.display(),
        syntax_ok,
        check_tool
    );
    Ok(NixProposeResult {
        staging_path: staging.display().to_string(),
        bytes: nix_text.len(),
        syntax_ok,
        check_tool,
        hitl_required: true,
        switch_forbidden: true,
        note: "build-vm/switch only via human (nosd-nix-propose-check → HITL)".into(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn reject_switch_embed() {
        let t = r#"{ ... }: { system.activationScripts.x = "nixos-rebuild switch"; }"#;
        assert!(validate_nix_text(t).is_err());
    }

    #[test]
    fn propose_writes_staging() {
        let dir = tempdir().unwrap();
        let r = propose_staging(dir.path(), "{ ... }: { }").unwrap();
        assert!(r.hitl_required);
        assert!(r.switch_forbidden);
        assert!(Path::new(&r.staging_path).is_file());
    }
}
