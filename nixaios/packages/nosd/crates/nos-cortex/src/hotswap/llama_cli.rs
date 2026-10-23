//! Backend GGUF via subprocess `llama-cli` / `llama-completion` (MVP Onda 2 — sem libllama).
//! Ausência do binário → EngineAbsent. `NOSD_LLAMA_NGL>0` só acelera builds CUDA;
//! nixpkgs `llama-cpp` default = CPU (`system_info` sem CUDA) — slog `cuda_engine_absent`.

use super::backend::{InferenceBackend, SwapError};
use std::path::{Path, PathBuf};
use std::process::Command;
use tracing::{info, warn};

/// Resolve CLI: `NOSD_LLAMA_CLI` ou completion-first no PATH.
/// Nix `llama-cpp` ≥0.6: `llama-cli` é wrapper de server — preferir `llama-completion`.
pub fn find_llama_cli() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("NOSD_LLAMA_CLI") {
        let pb = PathBuf::from(p);
        if pb.is_file() {
            return Some(pb);
        }
    }
    which("llama-completion")
        .or_else(|| which("llama-cli"))
        .or_else(|| which("llama"))
}

fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let cand = dir.join(name);
        if cand.is_file() {
            return Some(cand);
        }
        #[cfg(windows)]
        {
            let exe = dir.join(format!("{name}.exe"));
            if exe.is_file() {
                return Some(exe);
            }
        }
    }
    None
}

fn ngl_from_env() -> u32 {
    std::env::var("NOSD_LLAMA_NGL")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

fn n_predict_from_env() -> u32 {
    std::env::var("NOSD_LLAMA_N_PREDICT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(64)
}

pub struct LlamaCliBackend {
    cli: PathBuf,
    model: PathBuf,
    n_predict: u32,
    ngl: u32,
}

impl LlamaCliBackend {
    pub fn try_new(model: PathBuf) -> Result<Self, SwapError> {
        if !model.is_file() {
            return Err(SwapError::Infer("gguf missing"));
        }
        let cli = find_llama_cli().ok_or(SwapError::EngineAbsent("llama-cli"))?;
        let ngl = ngl_from_env();
        let n_predict = n_predict_from_env();
        info!(
            target: "nosd",
            "llama-cli backend cli={} model={} ngl={} n_predict={}",
            cli.display(),
            model.display(),
            ngl,
            n_predict
        );
        if ngl > 0 {
            warn!(
                target: "nosd",
                "NOSD_LLAMA_NGL={} set — needs CUDA-enabled llama; nixpkgs llama-cpp default is CPU",
                ngl
            );
        }
        Ok(Self {
            cli,
            model,
            n_predict,
            ngl,
        })
    }
}

impl InferenceBackend for LlamaCliBackend {
    fn name(&self) -> &'static str {
        "llama-gguf"
    }

    fn infer(&self, prompt: &str) -> Result<String, SwapError> {
        run_llama_cli(&self.cli, &self.model, prompt, self.n_predict, self.ngl)
    }
}

fn run_llama_cli(
    cli: &Path,
    model: &Path,
    prompt: &str,
    n_predict: u32,
    ngl: u32,
) -> Result<String, SwapError> {
    // Merge stdout+stderr: alguns builds logam em stderr e texto em stdout.
    let try_run = |extra: &[&str]| -> Result<std::process::Output, SwapError> {
        let mut cmd = Command::new(cli);
        cmd.arg("-m")
            .arg(model)
            .arg("-p")
            .arg(prompt)
            .arg("-n")
            .arg(n_predict.to_string());
        if ngl > 0 {
            cmd.arg("-ngl").arg(ngl.to_string());
        }
        for a in extra {
            cmd.arg(a);
        }
        cmd.output().map_err(SwapError::Io)
    };

    // Preferência: completion tool; stdout primeiro (stderr = banners/perf).
    for extra in [&[][..], &["--single-turn"][..], &["--simple-io"][..]] {
        let output = try_run(extra)?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if ngl > 0 {
            log_cuda_posture(&stdout, &stderr, ngl);
        }
        if !output.status.success() {
            warn!(
                target: "nosd",
                "llama fail status={:?} extra={:?} stderr={}",
                output.status.code(),
                extra,
                stderr.chars().take(240).collect::<String>()
            );
            continue;
        }
        let text = clean_completion(&stdout, prompt);
        if text.chars().filter(|c| !c.is_whitespace()).count() >= 2 {
            return Ok(text);
        }
        // Último recurso: stdout+stderr se o build misturar (raro)
        let text2 = clean_completion(&format!("{stdout}\n{stderr}"), prompt);
        if text2.chars().filter(|c| !c.is_whitespace()).count() >= 2 {
            return Ok(text2);
        }
        warn!(target: "nosd", "llama empty completion after clean; retry");
    }
    Err(SwapError::Infer("llama-cli failed"))
}

/// Detecta se o build usou GPU (honesty: ngl sem CUDA = CPU silencioso).
fn log_cuda_posture(stdout: &str, stderr: &str, ngl: u32) {
    let blob = format!("{stdout}\n{stderr}");
    let lower = blob.to_ascii_lowercase();
    if lower.contains("no usable gpu")
        || lower.contains("compiled without gpu")
        || (lower.contains("ggml_cuda") && lower.contains("not compiled"))
    {
        warn!(
            target: "nosd",
            "cuda_engine_absent ngl={} (llama build sem GPU utilizável)",
            ngl
        );
        return;
    }
    // Builds CUDA costumam listar CUDA=1 / GGML_CUDA / offloaded N/M layers
    let has_cuda = lower.contains("cuda = 1")
        || lower.contains("cuda=1")
        || lower.contains("ggml_cuda")
        || lower.contains("offloaded")
        || lower.contains("nvidia");
    let sys_cpu_only = lower.contains("system_info")
        && lower.contains("| cpu :")
        && !has_cuda;
    if sys_cpu_only {
        warn!(
            target: "nosd",
            "cuda_engine_absent ngl={} (system_info CPU-only; nix llama-cpp default)",
            ngl
        );
    } else if has_cuda {
        info!(target: "nosd", "cuda_path_observed ngl={}", ngl);
    }
}

/// Remove eco do prompt e ruído típico do CLI / server logs.
fn clean_completion(stdout: &str, prompt: &str) -> String {
    let mut s = stdout.replace('\r', "");
    if let Some(idx) = s.find(prompt) {
        s = s[idx + prompt.len()..].to_string();
    }
    let skip = |l: &str| {
        let t = l.trim();
        t.is_empty()
            || t.starts_with("llama_")
            || t.starts_with("ggml_")
            || t.starts_with("load_")
            || t.starts_with("warning:")
            || t.starts_with("main:")
            || t.starts_with("sampler ")
            || t.contains("system_info")
            || t.contains("gguf_init")
            || t.contains("common_init")
            || t.contains("common_perf")
            || t.contains("sampler chain")
            || t.contains("generate: n_ctx")
            || t.contains("srv ")
            || t.contains("build info")
            || t.contains("log_disable")
            // timestamps estilo "0.00.129.321 I ..."
            || (t.len() > 12 && t.as_bytes().get(0).map(|c| c.is_ascii_digit()).unwrap_or(false) && t.contains(" I "))
    };
    let lines: Vec<&str> = s.lines().map(str::trim).filter(|l| !skip(l)).collect();
    if lines.is_empty() {
        // stdout one-shot sem newlines (comum no llama-completion)
        let one = s.trim();
        if skip(one) {
            return String::new();
        }
        return one.chars().take(1024).collect();
    }
    lines.join(" ").chars().take(1024).collect::<String>().trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_strips_prompt() {
        let out = clean_completion("Once upon a time, there was a girl", "Once upon a time");
        assert!(out.contains("girl"));
        assert!(!out.contains("common_perf"));
    }

    #[test]
    fn clean_drops_perf_logs() {
        let dirty = "Once upon a time\n0.00.129.321 I common_perf_print: load time\nsampler chain: logits";
        let out = clean_completion(dirty, "Once upon a time");
        assert!(!out.contains("common_perf"));
        assert!(!out.contains("sampler chain"));
    }

    #[test]
    fn missing_model_absent() {
        let err = LlamaCliBackend::try_new(PathBuf::from("/nonexistent/model.gguf"));
        assert!(matches!(err, Err(SwapError::Infer(_)) | Err(SwapError::EngineAbsent(_))));
    }
}
