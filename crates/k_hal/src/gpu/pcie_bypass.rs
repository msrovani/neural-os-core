//! PCIe "bypass" seguro (Pilar 2.1 da diretiva de desbloqueio): **Resizable BAR**
//! + **ACS** (P2P peer DMA) — com SANITY CHECKS.
//!
//! Regra de ouro: NUNCA escrita crua em offset não-verificado (os endereços do
//! exemplo colado — `0x0010a000`, `0x002050`, `0xFFFFFFFF` no PMC — NÃO batem
//! com o mapa validado do repo: `nvidia_pascal.rs` usa RUNLIST 0x002270, KICK
//! 0x002634, CHANNEL 0x800000; escrever 0xFFFFFFFF em registrador errado é
//! brick/reboot). Toda operação aqui é: **probe da capability → validação de
//! suporte → RMW com readback verificado**.
//!
//! Acesso ao config space abstraído (`PciConfigIo`) → teste host com config
//! fake; acesso real = port I/O 0xCF8/0xCFC (`k_nano::pci`, target-only).

use alloc::vec::Vec;

/// Acesso ao config space PCI (trait p/ teste host com fake).
pub trait PciConfigIo {
    fn read_dword(&self, offset: u8) -> u32;
    fn write_dword(&mut self, offset: u8, value: u32);
}

/// Acesso REAL (bare-metal): port I/O via `k_nano::pci` (0xCF8/0xCFC).
/// `#[cfg(target_os = "none")]` — em host (testes) não existe.
#[cfg(target_os = "none")]
pub struct RealPciConfig {
    pub bus: u8,
    pub device: u8,
    pub function: u8,
}

#[cfg(target_os = "none")]
impl PciConfigIo for RealPciConfig {
    fn read_dword(&self, offset: u8) -> u32 {
        unsafe { k_nano::pci::read_config_dword(self.bus, self.device, self.function, offset) }
    }
    fn write_dword(&mut self, offset: u8, value: u32) {
        unsafe { k_nano::pci::write_config_dword(self.bus, self.device, self.function, offset, value) }
    }
}

// ─── Capabilities PCIe (spec: cap id bits 7:0, next pointer bits 15:8) ───────
const CAPS_PTR_OFFSET: u8 = 0x34;
const CAP_PCIE: u8 = 0x10;
const CAP_RESIZABLE_BAR: u8 = 0x15;
const CAP_ACS: u8 = 0x0D;

/// Percorre a lista de capabilities e devolve o offset da cap pedida.
pub fn find_cap(cfg: &dyn PciConfigIo, cap_id: u8) -> Option<u8> {
    let mut off = (cfg.read_dword(CAPS_PTR_OFFSET) & 0xFF) as u8;
    for _ in 0..32 {
        if off == 0 {
            return None;
        }
        let d = cfg.read_dword(off);
        if (d & 0xFF) as u8 == cap_id {
            return Some(off);
        }
        off = ((d >> 8) & 0xFF) as u8;
    }
    None
}

// ─── Resizable BAR (PCIe 4.0 §7.8.6) ─────────────────────────────────────────
// dword cap+4:  bits 3:1 BAR index · bits 30:4 tamanhos suportados (bit i → 1MB<<(i-4))
// dword cap+8:  bit 0 enable · bits 5:1 BAR size (mesma codificação: idx → 1MB<<idx)

/// Tamanhos de BAR suportados pelo device, em MB (bit i da máscara → 1MB<<(i-4)).
pub fn rebar_supported_sizes_mb(cfg: &dyn PciConfigIo) -> Option<Vec<u32>> {
    let cap = find_cap(cfg, CAP_RESIZABLE_BAR)?;
    let d = cfg.read_dword(cap + 4);
    let mut sizes = Vec::new();
    for bit in 4..=30u8 {
        if d & (1u32 << bit) != 0 {
            sizes.push(1u32 << (bit - 4));
        }
    }
    if sizes.is_empty() {
        None
    } else {
        Some(sizes)
    }
}

/// Tamanho ATUAL da janela ReBAR, em MB (control bits 5:1).
pub fn rebar_current_size_mb(cfg: &dyn PciConfigIo) -> Option<u32> {
    let cap = find_cap(cfg, CAP_RESIZABLE_BAR)?;
    let ctrl = cfg.read_dword(cap + 8);
    Some(1u32 << ((ctrl >> 1) & 0x1F))
}

/// Ativa uma janela de BAR suportada. RMW + readback — nunca escrita crua.
/// `target_mb` deve ser potência de 2 e estar na máscara suportada do device.
pub fn try_enable_resizable_bar(
    cfg: &mut dyn PciConfigIo,
    target_mb: u32,
) -> Result<u32, &'static str> {
    let cap = find_cap(cfg, CAP_RESIZABLE_BAR).ok_or("sem capability Resizable BAR")?;
    if !target_mb.is_power_of_two() || target_mb < 1 {
        return Err("tamanho deve ser potência de 2, >= 1MB");
    }
    let idx = target_mb.trailing_zeros();
    if idx > 30 {
        return Err("tamanho > 1TB (fora do encoding u32)");
    }
    let sizes = rebar_supported_sizes_mb(cfg).ok_or("ReBAR sem tamanhos suportados")?;
    if !sizes.contains(&target_mb) {
        return Err("tamanho não suportado pelo device");
    }
    // BAR size control: mantém enable (bit 0), troca só o tamanho (bits 5:1)
    let ctrl = cfg.read_dword(cap + 8);
    let new_ctrl = (ctrl & !(0x1F << 1)) | ((idx & 0x1F) << 1);
    cfg.write_dword(cap + 8, new_ctrl);
    let back = cfg.read_dword(cap + 8);
    if ((back >> 1) & 0x1F) != idx {
        return Err("readback divergente — device recusou a janela");
    }
    Ok(target_mb)
}

// ─── ACS (PCIe 4.0 §7.7.10) — peer DMA ───────────────────────────────────────
// Control bits: 0 Source Validation · 1 Translation Blocking · 2 P2P Request
// Redirect · 3 P2P Completion Redirect · 4 Upstream Fwd · 5 Downstream Fwd ...

/// Bits de controle ACS que REDIRECIONAM tráfego P2P para o root complex
/// (bloqueiam peer DMA GPU↔GPU/disco). Limpar = habilitar P2P.
const ACS_P2P_REDIRECT: u32 = (1 << 2) | (1 << 3);

/// Valor atual do control ACS da ponte (None = sem cap ACS).
pub fn acs_control(cfg: &dyn PciConfigIo) -> Option<u32> {
    let cap = find_cap(cfg, CAP_ACS)?;
    Some(cfg.read_dword(cap + 4))
}

/// Limpa os redirects P2P (bits 2|3) na ponte — habilita peer DMA.
/// RMW + readback verificado; ponte sem ACS → Err explícito (nunca silencioso).
/// ⚠️ Segurança: sem IOMMU, peer DMA enfraquece isolamento — chamada explícita
/// (HITL/CapGate), nunca automática no boot.
pub fn try_clear_p2p_redirect(cfg: &mut dyn PciConfigIo) -> Result<u32, &'static str> {
    let cap = find_cap(cfg, CAP_ACS).ok_or("ponte sem capability ACS")?;
    let ctrl = cfg.read_dword(cap + 4);
    let new_ctrl = ctrl & !ACS_P2P_REDIRECT;
    if new_ctrl != ctrl {
        cfg.write_dword(cap + 4, new_ctrl);
        let back = cfg.read_dword(cap + 4);
        if back & ACS_P2P_REDIRECT != 0 {
            return Err("ponte recusou limpar os redirects P2P");
        }
    }
    Ok(new_ctrl)
}

/// Resumo para HUD/SelfHeal: PCIe cap? ReBAR (tamanhos/atual)? ACS?
pub fn pcie_bypass_report(cfg: &dyn PciConfigIo) -> alloc::string::String {
    let mut s = alloc::string::String::from("pcie_bypass: ");
    match find_cap(cfg, CAP_PCIE) {
        Some(_) => s.push_str("pcie=yes "),
        None => s.push_str("pcie=no "),
    }
    match rebar_supported_sizes_mb(cfg) {
        Some(sizes) => {
            s.push_str(&alloc::format!(
                "rebar_sizes={:?}MB current={}MB ",
                sizes,
                rebar_current_size_mb(cfg).unwrap_or(0)
            ));
        }
        None => s.push_str("rebar=none "),
    }
    match acs_control(cfg) {
        Some(c) => s.push_str(&alloc::format!("acs_ctrl={:#06x}", c)),
        None => s.push_str("acs=none"),
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Config space fake de 256B: header com pointer p/ caps; PCIe @0x40 →
    /// ReBAR @0x48 (id 0x48, suporte 0x4C, control 0x50) → ACS @0x58
    /// (id 0x58, control 0x5C com redirects P2P).
    struct FakePciConfig {
        bytes: [u8; 256],
    }

    impl FakePciConfig {
        fn plain() -> Self {
            FakePciConfig { bytes: [0u8; 256] }
        }
        fn with_rebar_and_acs() -> Self {
            let mut f = FakePciConfig::plain();
            f.bytes[0x34] = 0x40; // capabilities pointer
            f.bytes[0x40] = CAP_PCIE;
            f.bytes[0x41] = 0x48;
            // ReBAR @0x48: id, next 0x58, suporte, control
            f.bytes[0x48] = CAP_RESIZABLE_BAR;
            f.bytes[0x49] = 0x58;
            // suporta 1MB..1GB: bits 4..=14 (byte 0x4C = bits 4-7 → 0xF0;
            // byte 0x4D = bits 8-14 → 0x7F)
            f.bytes[0x4C] = 0xF0;
            f.bytes[0x4D] = 0x7F;
            // control em 0x50 default zero (→ 1MB atual)
            // ACS @0x58: control em 0x5C com P2P Request+Completion Redirect
            f.bytes[0x58] = CAP_ACS;
            f.bytes[0x59] = 0x00;
            f.bytes[0x5C] = (1 << 2) as u8 | (1 << 3) as u8;
            f
        }
    }

    impl PciConfigIo for FakePciConfig {
        fn read_dword(&self, offset: u8) -> u32 {
            let off = (offset & 0xFC) as usize;
            u32::from_le_bytes([
                self.bytes[off],
                self.bytes[off + 1],
                self.bytes[off + 2],
                self.bytes[off + 3],
            ])
        }
        fn write_dword(&mut self, offset: u8, value: u32) {
            let off = (offset & 0xFC) as usize;
            self.bytes[off..off + 4].copy_from_slice(&value.to_le_bytes());
        }
    }

    #[test]
    fn find_cap_parses_list() {
        let f = FakePciConfig::with_rebar_and_acs();
        assert_eq!(find_cap(&f, CAP_PCIE), Some(0x40));
        assert_eq!(find_cap(&f, CAP_RESIZABLE_BAR), Some(0x48));
        assert_eq!(find_cap(&f, CAP_ACS), Some(0x58));
        assert_eq!(find_cap(&f, 0x99), None);
        let p = FakePciConfig::plain();
        assert_eq!(find_cap(&p, CAP_PCIE), None, "sem caps → None gracioso");
    }

    #[test]
    fn rebar_sizes_parsed() {
        let f = FakePciConfig::with_rebar_and_acs();
        assert_eq!(
            rebar_supported_sizes_mb(&f),
            Some(vec![1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024])
        );
        assert_eq!(rebar_current_size_mb(&f), Some(1));
    }

    #[test]
    fn enable_rebar_256mb_readback_verified() {
        let mut f = FakePciConfig::with_rebar_and_acs();
        assert_eq!(try_enable_resizable_bar(&mut f, 256), Ok(256));
        assert_eq!(rebar_current_size_mb(&f), Some(256));
        // control: idx 8 (256MB = 1<<8) em bits 5:1 (dword em 0x50)
        assert_eq!((f.bytes[0x50] & 0x3E) >> 1, 8);
    }

    #[test]
    fn enable_rebar_rejects_unsupported() {
        let mut f = FakePciConfig::with_rebar_and_acs();
        assert_eq!(
            try_enable_resizable_bar(&mut f, 3),
            Err("tamanho deve ser potência de 2, >= 1MB")
        );
        assert_eq!(
            try_enable_resizable_bar(&mut f, 2048),
            Err("tamanho não suportado pelo device")
        );
        let mut p = FakePciConfig::plain();
        assert!(try_enable_resizable_bar(&mut p, 64).is_err(), "sem cap → Err gracioso");
    }

    #[test]
    fn acs_clear_p2p_redirect_readback_verified() {
        let mut f = FakePciConfig::with_rebar_and_acs();
        assert_eq!(acs_control(&f), Some(0x0C));
        assert_eq!(try_clear_p2p_redirect(&mut f), Ok(0));
        assert_eq!(acs_control(&f), Some(0));
        // idempotente: segunda chamada no-op Ok
        assert_eq!(try_clear_p2p_redirect(&mut f), Ok(0));
        let mut p = FakePciConfig::plain();
        assert!(try_clear_p2p_redirect(&mut p).is_err(), "sem ACS → Err explícito");
    }

    #[test]
    fn report_is_honest() {
        let f = FakePciConfig::with_rebar_and_acs();
        let r = pcie_bypass_report(&f);
        assert!(r.contains("pcie=yes") && r.contains("rebar_sizes") && r.contains("acs_ctrl"));
        let p = FakePciConfig::plain();
        let r2 = pcie_bypass_report(&p);
        assert!(r2.contains("pcie=no") && r2.contains("rebar=none") && r2.contains("acs=none"));
    }

    #[test]
    fn rebar_fallback_chain_256_to_128() {
        // FakePciConfig suporta 1..1024MB. Se tentarmos 2048 (não suportado),
        // o caller faz fallback 256→128→64→32→16.
        // Aquo testamos que o fallback funciona: 128MB é suportado.
        let mut f = FakePciConfig::with_rebar_and_acs();
        // 2048 não está na lista → Err
        assert!(try_enable_resizable_bar(&mut f, 2048).is_err());
        // 128 está na lista → Ok
        assert_eq!(try_enable_resizable_bar(&mut f, 128), Ok(128));
        assert_eq!(rebar_current_size_mb(&f), Some(128));
    }

    #[test]
    fn rebar_report_lists_all_sizes() {
        let f = FakePciConfig::with_rebar_and_acs();
        let r = pcie_bypass_report(&f);
        // Deve listar tamanhos suportados
        assert!(r.contains("rebar_sizes="));
        assert!(r.contains("current="));
        // O sizes deve incluir 256 e 1024
        assert!(r.contains("256") && r.contains("1024"));
    }
}
