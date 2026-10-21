use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EngineKind {
    CpuEcho,
    LlamaGguf,
    Exl2,
    VllmCuda,
    VllmRocm,
    LlamaHip,
    Ipex,
    OpenVino,
    LlamaSycl,
}

#[derive(Debug)]
pub enum SwapError {
    EngineAbsent(&'static str),
    Infer(&'static str),
    Io(std::io::Error),
}

impl fmt::Display for SwapError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EngineAbsent(s) => write!(f, "engine absent: {s}"),
            Self::Infer(s) => write!(f, "infer: {s}"),
            Self::Io(e) => write!(f, "io: {e}"),
        }
    }
}

impl std::error::Error for SwapError {}

pub trait InferenceBackend: Send {
    fn name(&self) -> &'static str;
    fn infer(&self, prompt: &str) -> Result<String, SwapError>;
}

pub struct CpuEchoBackend;

impl InferenceBackend for CpuEchoBackend {
    fn name(&self) -> &'static str {
        "cpu-echo"
    }

    fn infer(&self, prompt: &str) -> Result<String, SwapError> {
        Ok(format!(
            "[cpu-echo] {}",
            prompt.chars().take(256).collect::<String>()
        ))
    }
}
