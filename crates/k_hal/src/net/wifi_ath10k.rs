//! ath10k QCA6174 A3 — wake → target_init → BMI → fw_ready (Note 1050).
//! fw_ready=PASS só com FW_IND_INITIALIZED após BMI_DONE.

use core::ptr::{read_volatile, write_volatile};

use crate::net::ath10k_ce_bmi::{CeBmi, PATCH_LOAD_ADDR};
use crate::net::ath10k_fw;

/// QCA6174 register map (Linux ath10k qca6174_regs).
const SOC_CHIP_ID: usize = 0x0000_00f0;
const RTC_STATE: usize = 0x0000;
const PCIE_SOC_WAKE: usize = 0x0004;
const PCIE_SOC_WAKE_V: u32 = 1;
const RTC_STATE_V_ON: u32 = 3;
const RTC_STATE_V_MASK: u32 = 0x7;
const FW_INDICATOR: usize = 0x0003_a028;
const FW_IND_INITIALIZED: u32 = 2;

pub struct Ath10kDevice {
    bar: usize,
    did: u16,
    pci_rev: u8,
}

impl Ath10kDevice {
    pub fn new(bar: usize, did: u16, pci_rev: u8) -> Self {
        Self { bar, did, pci_rev }
    }

    unsafe fn r32(&self, off: usize) -> u32 {
        read_volatile((self.bar + off) as *const u32)
    }
    unsafe fn w32(&self, off: usize, v: u32) {
        write_volatile((self.bar + off) as *mut u32, v);
    }

    fn pause_busy(&self) {
        for _ in 0..5000 {
            core::hint::spin_loop();
        }
    }

    pub fn probe_log(&self) {
        let chip = if self.bar != 0 {
            unsafe { self.r32(SOC_CHIP_ID) }
        } else {
            0
        };
        k_nano::slog_hal!(
            "ATH10K",
            "info",
            "pci=168c:{:04x} rev={:02x} bar={:#x} chip_id={:#x}",
            self.did,
            self.pci_rev,
            self.bar,
            chip
        );
    }

    fn soc_wake(&self) -> Result<(), &'static str> {
        unsafe {
            self.w32(PCIE_SOC_WAKE, PCIE_SOC_WAKE_V);
        }
        for _ in 0..10_000 {
            let st = unsafe { self.r32(RTC_STATE) } & RTC_STATE_V_MASK;
            if st == RTC_STATE_V_ON {
                k_nano::slog_hal!("ATH10K", "info", "step=wake status=OK rtc={}", st);
                return Ok(());
            }
            self.pause_busy();
        }
        k_nano::slog_hal!("ATH10K", "info", "step=wake status=TIMEOUT");
        Err("wake_timeout")
    }

    fn wait_target_init(&self) -> Result<(), &'static str> {
        for _ in 0..20_000 {
            let ind = unsafe { self.r32(FW_INDICATOR) };
            if ind == 0xffff_ffff {
                return Err("device_gone");
            }
            if ind & FW_IND_INITIALIZED != 0 {
                k_nano::slog_hal!(
                    "ATH10K",
                    "info",
                    "step=target_init status=OK ind={:#x}",
                    ind
                );
                return Ok(());
            }
            self.pause_busy();
        }
        Err("target_init_timeout")
    }

    fn wait_fw_ready(&self) -> Result<(), &'static str> {
        // Após BMI_DONE o ROM limpa e o FW seta INITIALIZED de novo.
        for _ in 0..40_000 {
            let ind = unsafe { self.r32(FW_INDICATOR) };
            if ind == 0xffff_ffff {
                return Err("device_gone");
            }
            if ind & FW_IND_INITIALIZED != 0 {
                k_nano::slog_hal!(
                    "ATH10K",
                    "info",
                    "step=fw_ready status=OK ind={:#x}",
                    ind
                );
                return Ok(());
            }
            self.pause_busy();
        }
        Err("fw_ready_timeout")
    }

    /// A3 bring-up completo. PASS só com fw_ready medido.
    pub fn a3_bringup(&mut self) {
        self.probe_log();

        if self.bar == 0 {
            k_nano::slog_hal!(
                "ATH10K",
                "info",
                "VERDICT=FAIL reason=no_bar"
            );
            return;
        }

        if let Err(e) = self.soc_wake() {
            k_nano::slog_hal!("ATH10K", "info", "VERDICT=FAIL reason={}", e);
            return;
        }

        if let Err(e) = self.wait_target_init() {
            k_nano::slog_hal!("ATH10K", "info", "VERDICT=FAIL reason={}", e);
            return;
        }

        let spec = match ath10k_fw::resolve_ath10k_fw(self.did) {
            Some(s) => s,
            None => {
                k_nano::slog_hal!(
                    "ATH10K",
                    "info",
                    "VERDICT=FAIL reason=did_unsupported"
                );
                return;
            }
        };

        let blobs = match ath10k_fw::load_ath10k_blobs(&spec) {
            Ok(b) => b,
            Err(e) => {
                k_nano::slog_hal!("ATH10K", "info", "VERDICT=FAIL reason={}", e);
                return;
            }
        };

        let mut ce = match CeBmi::init(self.bar) {
            Ok(c) => c,
            Err(e) => {
                k_nano::slog_hal!(
                    "ATH10K",
                    "info",
                    "VERDICT=PARTIAL reason={}",
                    e
                );
                return;
            }
        };

        if let Err(e) = ce.get_target_info() {
            k_nano::slog_hal!(
                "ATH10K",
                "info",
                "VERDICT=PARTIAL reason={}",
                e
            );
            return;
        }

        // OTP (opcional) → board → FW_IMAGE @ patch_load_addr
        if !blobs.otp_image.is_empty() {
            if let Err(e) = ce.lz_download(PATCH_LOAD_ADDR, &blobs.otp_image) {
                k_nano::slog_hal!(
                    "ATH10K",
                    "info",
                    "step=otp status=FAIL reason={}",
                    e
                );
            } else {
                k_nano::slog_hal!("ATH10K", "info", "step=otp status=OK");
            }
        }

        if !blobs.board.is_empty() {
            // Board data: write via BMI próximo do host_interest (DRAM).
            // Endereço típico QCA6174 hi_board_data ≈ 0x004007d4 (residual se falhar).
            const BOARD_ADDR: u32 = 0x0040_07d4;
            match ce.write_memory(BOARD_ADDR, &blobs.board) {
                Ok(()) => k_nano::slog_hal!(
                    "ATH10K",
                    "info",
                    "step=board status=OK bytes={}",
                    blobs.board.len()
                ),
                Err(e) => k_nano::slog_hal!(
                    "ATH10K",
                    "info",
                    "step=board status=FAIL reason={}",
                    e
                ),
            }
        }

        if let Err(e) = ce.lz_download(PATCH_LOAD_ADDR, &blobs.fw_image) {
            k_nano::slog_hal!(
                "ATH10K",
                "info",
                "VERDICT=PARTIAL reason=fw_lz_{}",
                e
            );
            return;
        }

        // Limpa indicator antes do DONE para distinguir ROM vs FW.
        unsafe {
            self.w32(FW_INDICATOR, 0);
        }

        if let Err(e) = ce.done() {
            k_nano::slog_hal!(
                "ATH10K",
                "info",
                "VERDICT=PARTIAL reason={}",
                e
            );
            return;
        }

        match self.wait_fw_ready() {
            Ok(()) => {
                crate::unlock_dag::grant(crate::unlock_dag::CapToken::WifiFwAlive);
                k_nano::slog_hal!(
                    "ATH10K",
                    "info",
                    "VERDICT=PASS fw_ready=1 image={}",
                    blobs.fw_image.len()
                );
            }
            Err(e) => {
                k_nano::slog_hal!(
                    "ATH10K",
                    "info",
                    "VERDICT=PARTIAL reason={} (downloaded_bmi_done)",
                    e
                );
            }
        }
    }
}

/// Bind ath10k — A3 bring-up (Note). QEMU sem 003E não chama.
pub fn a3_on_bind(bar: usize, did: u16, pci_rev: u8) {
    let mut dev = Ath10kDevice::new(bar, did, pci_rev);
    dev.a3_bringup();
}

/// Compat A2 nome — redireciona para A3.
pub fn scaffold_on_bind(bar: usize, did: u16, pci_rev: u8) {
    a3_on_bind(bar, did, pci_rev);
}
