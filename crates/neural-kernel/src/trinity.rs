use crate::tensor::{PackedTernaryTensor, Tensor};
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

pub enum ExpertKind {
    HwIdentify,
    RustCoder,
    DiskDiag,
    Security,
    Generator,
    Unknown,
}

pub struct Expert {
    pub kind: ExpertKind,
    pub name: &'static str,
    pub description: &'static str,
    pub weight: Option<PackedTernaryTensor>,
}

pub struct TrinityRouter {
    experts: Vec<Expert>,
    router_weight: Option<PackedTernaryTensor>,
}

impl TrinityRouter {
    pub fn new() -> Self {
        TrinityRouter {
            experts: Vec::new(),
            router_weight: None,
        }
    }

    pub fn register_expert(&mut self, expert: Expert) {
        crate::serial_println!("[TRINITY] Expert '{}' registered: {}", expert.name, expert.description);
        self.experts.push(expert);
    }

    pub fn classify_intent(&self, text: &str) -> &Expert {
        let lower = text.to_lowercase();
        for expert in &self.experts {
            match expert.kind {
                ExpertKind::HwIdentify => {
                    if lower.contains("/hw") || lower.contains("hardware") || lower.contains("pci")
                        || lower.contains("dispositivo") || lower.contains("device")
                    {
                        return expert;
                    }
                }
                ExpertKind::RustCoder => {
                    if lower.contains("crie") || lower.contains("create") || lower.contains("code")
                        || lower.contains("write") || lower.contains("implement")
                    {
                        return expert;
                    }
                }
                ExpertKind::DiskDiag => {
                    if lower.contains("disk") || lower.contains("disco") || lower.contains("smart")
                        || lower.contains("storage") || lower.contains("armazenamento")
                    {
                        return expert;
                    }
                }
                ExpertKind::Security => {
                    if lower.contains("security") || lower.contains("seguranca")
                        || lower.contains("cve") || lower.contains("attack") || lower.contains("ataque")
                    {
                        return expert;
                    }
                }
                ExpertKind::Generator | ExpertKind::Unknown => {}
            }
        }
        &self.experts[0]
    }

    pub fn agent_count(&self) -> usize { self.experts.len() }
}
