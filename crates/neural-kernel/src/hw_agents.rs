/// Hardware Agent — cada dispositivo de hardware vira um agente
/// que sabe o que pode fazer, suas capabilities, e fica aguardando
/// o usuario pedir algo ("quero fazer video chamada" → mic+camera)
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

#[derive(Debug, Clone, PartialEq)]
pub enum HwCapability {
    AudioInput, AudioOutput, VideoInput, VideoOutput,
    NetworkAccess, Storage, Compute, Display, HumanInput,
    UsbHost, PciBridge, Encryption, Wireless,
}

#[derive(Debug)]
pub struct HwAgent {
    pub name: String,
    pub device_id: String,
    pub class: u16,
    pub subclass: u16,
    pub capabilities: Vec<HwCapability>,
    pub available: bool,
    pub description: String,
}

impl HwAgent {
    pub fn can(&self, cap: &HwCapability) -> bool {
        self.available && self.capabilities.contains(cap)
    }
}

/// Registry de hardware — mapeia PCI/USB IDs para HwAgents
pub struct HwRegistry {
    pub agents: Vec<HwAgent>,
}

impl HwRegistry {
    pub fn new() -> Self { HwRegistry { agents: Vec::new() } }

    /// Detecta hardware via PCI scan e cria HwAgent para cada dispositivo
    pub unsafe fn detect_all(&mut self) {
        let devices = crate::pci::scan_pci();
        for dev in &devices {
            let (class, subclass) = (dev.class as u16, dev.subclass as u16);
            let caps = Self::class_to_capabilities(class, subclass);
            let desc = Self::device_description(dev.vendor_id, dev.device_id);
            self.agents.push(HwAgent {
                name: desc.clone(),
                device_id: alloc::format!("{:04x}:{:04x}", dev.vendor_id, dev.device_id),
                class, subclass,
                capabilities: caps,
                available: true,
                description: desc,
            });
        }
    }

    /// LLM pergunta: "o que tem de hardware disponivel?"
    pub fn llm_context(&self) -> String {
        let mut ctx = String::from("Hardware disponivel:\n");
        for a in &self.agents {
            if a.available {
                let caps: Vec<&str> = a.capabilities.iter().map(|c| match c {
                    HwCapability::AudioInput => "audio-in", HwCapability::AudioOutput => "audio-out",
                    HwCapability::VideoInput => "video-in", HwCapability::VideoOutput => "video-out",
                    HwCapability::NetworkAccess => "net", HwCapability::Storage => "storage",
                    HwCapability::Compute => "compute", HwCapability::Display => "display",
                    HwCapability::HumanInput => "input", HwCapability::UsbHost => "usb",
                    HwCapability::PciBridge => "bridge", HwCapability::Encryption => "crypto",
                    HwCapability::Wireless => "wifi",
                }).collect();
                ctx.push_str(&alloc::format!("  {}: {:?}\n", a.name, caps));
            }
        }
        ctx
    }

    /// Ativa agentes baseado em intent do usuario
    /// "quero fazer video chamada" → mic+camera+display+net
    pub fn activate_for_intent(&mut self, intent: &str) -> Vec<String> {
        let lower = intent.to_ascii_lowercase();
        let mut activated = Vec::new();

        if lower.contains("video") || lower.contains("camera") || lower.contains("chamada") {
            for a in self.agents.iter_mut() {
                if a.can(&HwCapability::VideoInput) { a.available = true; activated.push(a.name.clone()); }
                if a.can(&HwCapability::AudioInput) { a.available = true; activated.push(a.name.clone()); }
                if a.can(&HwCapability::Display) { a.available = true; activated.push(a.name.clone()); }
            }
        }
        if lower.contains("audio") || lower.contains("music") || lower.contains("grav") {
            for a in self.agents.iter_mut() {
                if a.can(&HwCapability::AudioInput) || a.can(&HwCapability::AudioOutput) {
                    a.available = true; activated.push(a.name.clone());
                }
            }
        }
        if lower.contains("rede") || lower.contains("internet") || lower.contains("net") {
            for a in self.agents.iter_mut() {
                if a.can(&HwCapability::NetworkAccess) || a.can(&HwCapability::Wireless) {
                    a.available = true; activated.push(a.name.clone());
                }
            }
        }
        activated
    }

    fn class_to_capabilities(class: u16, subclass: u16) -> Vec<HwCapability> {
        match class {
            0x01 => vec![HwCapability::Storage],
            0x02 => vec![HwCapability::NetworkAccess],
            0x03 => vec![HwCapability::Display, HwCapability::Compute],
            0x04 => vec![HwCapability::AudioInput, HwCapability::AudioOutput],
            0x0C if subclass == 0x03 => vec![HwCapability::UsbHost],
            0x06 => vec![HwCapability::PciBridge],
            0x07 => vec![HwCapability::HumanInput],
            0x08 => vec![HwCapability::Encryption],
            0x0D => vec![HwCapability::Wireless],
            _ => vec![],
        }
    }

    fn device_description(vendor: u16, device: u16) -> String {
        match (vendor, device) {
            (0x8086, 0x100e) => String::from("Intel PRO/1000 Network"),
            (0x10EC, 0x8139) => String::from("Realtek RTL8139 Ethernet"),
            (0x1AF4, 0x1041) => String::from("VirtIO Network"),
            (0x1AF4, 0x1050) => String::from("VirtIO GPU"),
            (0x1234, 0x1111) => String::from("QEMU VGA"),
            _ => alloc::format!("PCI {:04x}:{:04x}", vendor, device),
        }
    }
}
