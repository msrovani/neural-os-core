//! USB Mass Storage via Bulk-Only Transport (BOT) + SCSI.
//! Bring-up do stick: `xhci::bringup_boot_msc` (ADR-0062 P11) — sem Address Device
//! o probe antigo (slot=2 fixo) falhava e BOOT.LOG nunca gravava no pendrive.

use crate::xhci::{self, BulkEndpoint};

const CBW_SIGNATURE: u32 = 0x43425355;
const CSW_SIGNATURE: u32 = 0x53425355;
const SCSI_READ10: u8 = 0x28;
const SCSI_WRITE10: u8 = 0x2A;
const SCSI_INQUIRY: u8 = 0x12;
const SCSI_READ_CAPACITY: u8 = 0x25;
const SCSI_TEST_UNIT_READY: u8 = 0x00;
const SCSI_REQUEST_SENSE: u8 = 0x03;
const SCSI_SYNCHRONIZE_CACHE: u8 = 0x35;

pub struct UsbMassStorage {
    slot: u8,
    bulk_in: BulkEndpoint,
    bulk_out: BulkEndpoint,
    tag: u32,
    pub max_lba: u64,
    pub sector_size: u16,
    pub model: [u8; 36],
}

unsafe impl Send for UsbMassStorage {}

impl UsbMassStorage {
    pub unsafe fn probe() -> Option<Self> {
        {
            let state = xhci::XHCI_STATE.lock();
            if state.is_none() {
                return None;
            }
        }

        // Até 8 portas CCS: bringup falha em webcam/BT → skip porta → próxima.
        for attempt in 0..8u8 {
            let msc_dev = match xhci::bringup_boot_msc() {
                Some(d) => d,
                None => {
                    if attempt == 0 {
                        crate::slog_nano!("USB", "msc", "bringup_boot_msc FAIL — sem stick/enum");
                    }
                    return None;
                }
            };

            crate::slog_nano!(
                "USB",
                "msc",
                "bringup OK slot={} port={} speed={} mps={} try={}",
                msc_dev.slot,
                msc_dev.port,
                msc_dev.speed,
                msc_dev.max_packet,
                attempt
            );

            let mut msc = UsbMassStorage {
                slot: msc_dev.slot,
                tag: 1,
                max_lba: 0,
                sector_size: 512,
                bulk_in: msc_dev.ep_in,
                bulk_out: msc_dev.ep_out,
                model: [0u8; 36],
            };

            // Flash drives: TUR + sense antes de INQUIRY (Unit Attention).
            let _ = msc.scsi_test_unit_ready();
            let _ = msc.scsi_request_sense();
            let _ = msc.scsi_test_unit_ready();

            let mut scsi_ok = false;
            if let Some(inq) = msc.scsi_inquiry() {
                msc.model = inq;
                let vendor = core::str::from_utf8(&inq[8..16]).unwrap_or("?");
                let product = core::str::from_utf8(&inq[16..32]).unwrap_or("?");
                crate::slog_nano!("USB", "msc", "{} {}", vendor.trim(), product.trim());
                scsi_ok = true;
            }

            if let Some((lba, blk_sz)) = msc.scsi_read_capacity() {
                msc.max_lba = lba;
                msc.sector_size = if blk_sz == 0 { 512 } else { blk_sz };
                let gb = (lba as u128 * msc.sector_size as u128) / (1024 * 1024 * 1024);
                crate::slog_nano!(
                    "USB",
                    "msc",
                    "Capacidade: {} setores x {}B = {}GB",
                    lba,
                    msc.sector_size,
                    gb
                );
                scsi_ok = true;
            }

            if scsi_ok {
                return Some(msc);
            }
            crate::slog_nano!(
                "USB",
                "warn",
                "SCSI falhou port={} slot={} — Disable Slot + skip",
                msc_dev.port,
                msc.slot
            );
            unsafe { xhci::disable_slot(msc.slot) };
            xhci::mark_msc_port_failed(msc_dev.port);
        }
        crate::slog_nano!("USB", "msc", "SCSI falhou em todas as portas — MSC ignorado");
        None
    }

    unsafe fn bot_transfer(&mut self, cbw_data: &[u8], dir_in: bool, data_buf: &mut [u8]) -> bool {
        let tag = self.tag.wrapping_add(1);
        self.tag = tag;

        let mut cbw = [0u8; 31];
        cbw[0..4].copy_from_slice(&CBW_SIGNATURE.to_le_bytes());
        cbw[4..8].copy_from_slice(&tag.to_le_bytes());
        cbw[8..12].copy_from_slice(&(data_buf.len() as u32).to_le_bytes());
        cbw[12] = if dir_in { 0x80 } else { 0x00 };
        cbw[13] = 0;
        cbw[14] = cbw_data.len() as u8;
        cbw[15..15 + cbw_data.len()].copy_from_slice(cbw_data);

        let data_len = data_buf.len();
        let total = 31 + data_len.max(13);
        let dma_pa = match Self::alloc_dma(total) {
            Some(p) => p,
            None => return false,
        };
        let pmoff = crate::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
        let dma_va = (dma_pa + pmoff) as *mut u8;
        core::ptr::copy_nonoverlapping(cbw.as_ptr(), dma_va, 31);
        if !dir_in && data_len > 0 {
            core::ptr::copy_nonoverlapping(data_buf.as_ptr(), dma_va.add(31), data_len);
        }

        if !xhci::bulk_transfer(self.slot, 1, &mut self.bulk_out, dma_pa, 31, 0) {
            return false;
        }

        if dir_in && data_len > 0 {
            if !xhci::bulk_transfer(self.slot, 1, &mut self.bulk_in, dma_pa + 31, data_len as u32, 1)
            {
                return false;
            }
            for i in 0..data_len {
                data_buf[i] = dma_va.add(31 + i).read_volatile();
            }
        } else if !dir_in && data_len > 0 {
            if !xhci::bulk_transfer(self.slot, 1, &mut self.bulk_out, dma_pa + 31, data_len as u32, 0)
            {
                return false;
            }
        }

        let csw_off = 31 + data_len;
        if !xhci::bulk_transfer(self.slot, 1, &mut self.bulk_in, dma_pa + csw_off as u64, 13, 1)
        {
            return false;
        }
        let mut csw_raw = [0u8; 13];
        for i in 0..13 {
            csw_raw[i] = dma_va.add(csw_off + i).read_volatile();
        }

        let csw_sig = u32::from_le_bytes([csw_raw[0], csw_raw[1], csw_raw[2], csw_raw[3]]);
        let csw_status = csw_raw[12];
        csw_sig == CSW_SIGNATURE && csw_status == 0
    }

    pub unsafe fn read_sector(&mut self, lba: u64, data: &mut [u8; 512]) -> bool {
        if self.sector_size != 0 && self.sector_size != 512 {
            crate::slog_nano!("USB", "warn", "MSC 4Kn sector_size={} — recusa LBA 512 (honest)",
                self.sector_size);
            return false;
        }
        let mut cmd = [0u8; 16];
        cmd[0] = SCSI_READ10;
        cmd[2..6].copy_from_slice(&(lba as u32).to_be_bytes());
        cmd[8] = 1;
        self.bot_transfer(&cmd, true, data)
    }

    pub unsafe fn write_sector(&mut self, lba: u64, data: &[u8; 512]) -> bool {
        let mut cmd = [0u8; 16];
        cmd[0] = SCSI_WRITE10;
        cmd[2..6].copy_from_slice(&(lba as u32).to_be_bytes());
        cmd[8] = 1;
        let mut buf = *data;
        self.bot_transfer(&cmd, false, &mut buf)
    }

    unsafe fn scsi_inquiry(&mut self) -> Option<[u8; 36]> {
        let mut buf = [0u8; 36];
        let mut cmd = [0u8; 16];
        cmd[0] = SCSI_INQUIRY;
        cmd[4] = 36;
        if self.bot_transfer(&cmd, true, &mut buf) {
            Some(buf)
        } else {
            None
        }
    }

    unsafe fn scsi_read_capacity(&mut self) -> Option<(u64, u16)> {
        let mut buf = [0u8; 8];
        let mut cmd = [0u8; 16];
        cmd[0] = SCSI_READ_CAPACITY;
        if !self.bot_transfer(&cmd, true, &mut buf) {
            return None;
        }
        let lba = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
        let blk_sz = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
        Some((lba as u64, blk_sz as u16))
    }

    unsafe fn scsi_test_unit_ready(&mut self) -> bool {
        let mut cmd = [0u8; 16];
        cmd[0] = SCSI_TEST_UNIT_READY;
        let mut empty = [0u8; 0];
        self.bot_transfer(&cmd, false, &mut empty)
    }

    unsafe fn scsi_request_sense(&mut self) -> bool {
        let mut buf = [0u8; 18];
        let mut cmd = [0u8; 16];
        cmd[0] = SCSI_REQUEST_SENSE;
        cmd[4] = 18;
        self.bot_transfer(&cmd, true, &mut buf)
    }

    /// SCSI SYNCHRONIZE_CACHE (0x35) — flushes device write cache to media.
    /// Essential after BOOT.LOG writes so data survives power loss.
    pub unsafe fn scsi_sync_cache(&mut self) -> bool {
        let mut cmd = [0u8; 16];
        cmd[0] = SCSI_SYNCHRONIZE_CACHE;
        cmd[1] = 0; // IMMED=0 (return after cache flush completes)
        cmd[2..6].copy_from_slice(&0u32.to_be_bytes()); // LBA=0 (entire device)
        cmd[6] = 0; // reserved
        // Number of Logical Blocks is 16-bit at bytes 7–8; byte 9 reserved.
        // copy_from_slice(&u32) em [7..10] (3 bytes) PANIC no flush do BOOT.LOG.
        cmd[7] = 0;
        cmd[8] = 0;
        cmd[9] = 0;
        let mut empty = [0u8; 0];
        self.bot_transfer(&cmd, false, &mut empty)
    }

    unsafe fn alloc_dma(size: usize) -> Option<u64> {
        let pages = (size + 4095) / 4096;
        let mut g = crate::memory::GLOBAL_ALLOCATOR.lock();
        let a = g.as_mut()?;
        let f = a.allocate_contiguous(pages)?;
        Some(f.start_address().as_u64())
    }
}
