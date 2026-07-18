//! USB Mass Storage via Bulk-Only Transport (BOT) + SCSI.
//! Protocolo: CBW (31 bytes) -> Data (opcional) -> CSW (13 bytes)

use crate::xhci::{self, BulkEndpoint};

const CBW_SIGNATURE: u32 = 0x43425355;
const CSW_SIGNATURE: u32 = 0x53425355;
const SCSI_READ10: u8 = 0x28;
const SCSI_WRITE10: u8 = 0x2A;
const SCSI_INQUIRY: u8 = 0x12;
const SCSI_READ_CAPACITY: u8 = 0x25;
const SCSI_TEST_UNIT_READY: u8 = 0x00;

pub struct UsbMassStorage {
    slot: u8,
    bulk_in: BulkEndpoint,   // EP1 IN (device->host)
    bulk_out: BulkEndpoint,  // EP1 OUT (host->device)
    tag: u32,
    pub max_lba: u64,
    pub sector_size: u16,
    pub model: [u8; 36],
}

unsafe impl Send for UsbMassStorage {}

impl UsbMassStorage {
    pub unsafe fn probe() -> Option<Self> {
        // NÃO segurar XHCI_STATE: configure_msc_endpoints também faz lock (não-reentrante).
        {
            let state = xhci::XHCI_STATE.lock();
            if state.is_none() {
                return None;
            }
        }

        let slot = 2;
        let max_packet = 512;
        if let Some((ep_in, ep_out)) = xhci::configure_msc_endpoints(slot, max_packet) {
            crate::slog_nano!("USB", "msc", "Endpoints OK. probe...");

            let mut msc = UsbMassStorage {
                slot, tag: 1, max_lba: 0, sector_size: 512,
                bulk_in: ep_in, bulk_out: ep_out,
                model: [0u8; 36],
            };

            // SCSI INQUIRY
            if let Some(inq) = msc.scsi_inquiry() {
                msc.model = inq;
                let vendor = core::str::from_utf8(&inq[8..16]).unwrap_or("?");
                let product = core::str::from_utf8(&inq[16..32]).unwrap_or("?");
                crate::slog_nano!("USB", "msc", "{} {}", vendor.trim(), product.trim());
            }

            // SCSI READ CAPACITY
            if let Some((lba, blk_sz)) = msc.scsi_read_capacity() {
                msc.max_lba = lba;
                msc.sector_size = blk_sz;
                let gb = (lba as u128 * blk_sz as u128) / (1024*1024*1024);
                crate::slog_nano!("USB", "msc", "Capacidade: {} setores x {}B = {}GB", lba, blk_sz, gb);
            }

            Some(msc)
        } else {
            None
        }
    }

    unsafe fn bot_transfer(&mut self, cbw_data: &[u8], dir_in: bool, data_buf: &mut [u8]) -> bool {
        let tag = self.tag.wrapping_add(1);
        self.tag = tag;

        // Build CBW
        let mut cbw = [0u8; 31];
        cbw[0..4].copy_from_slice(&CBW_SIGNATURE.to_le_bytes());
        cbw[4..8].copy_from_slice(&tag.to_le_bytes());
        cbw[8..12].copy_from_slice(&(data_buf.len() as u32).to_le_bytes());
        cbw[12] = if dir_in { 0x80 } else { 0x00 };
        cbw[13] = 0; // LUN
        cbw[14] = cbw_data.len() as u8;
        cbw[15..15+cbw_data.len()].copy_from_slice(cbw_data);

        // Allocate DMA buffer for CBW + data
        let total = 31u32 + if dir_in { 0 } else { data_buf.len() as u32 };
        let cbw_pa = match Self::alloc_dma(total as usize) { Some(p) => p, None => return false };
        let pmoff = crate::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);

        // Copy CBW to DMA buffer
        let dma_va = (cbw_pa + pmoff) as *mut u8;
        core::ptr::copy_nonoverlapping(cbw.as_ptr(), dma_va, 31);
        if !dir_in && !data_buf.is_empty() {
            core::ptr::copy_nonoverlapping(data_buf.as_ptr(), dma_va.add(31), data_buf.len());
        }

        // Send CBW via bulk OUT
        if !xhci::bulk_transfer(self.slot, 1, &mut self.bulk_out, cbw_pa, 31, 0) {
            return false;
        }

        // Data phase
        if dir_in && !data_buf.is_empty() {
            if !xhci::bulk_transfer(self.slot, 1, &mut self.bulk_in, cbw_pa, data_buf.len() as u32, 1) {
                return false;
            }
            // Copy data back
            for i in 0..data_buf.len() {
                data_buf[i] = dma_va.add(i).read_volatile();
            }
        } else if !dir_in && !data_buf.is_empty() {
            if !xhci::bulk_transfer(self.slot, 1, &mut self.bulk_out, cbw_pa + 31, data_buf.len() as u32, 0) {
                return false;
            }
        }

        // Receive CSW
        let mut csw_raw = [0u8; 13];
        if !xhci::bulk_transfer(self.slot, 1, &mut self.bulk_in, cbw_pa, 13, 1) {
            return false;
        }
        for i in 0..13 { csw_raw[i] = dma_va.add(i).read_volatile(); }

        let csw_sig = u32::from_le_bytes([csw_raw[0], csw_raw[1], csw_raw[2], csw_raw[3]]);
        let csw_status = csw_raw[12];
        csw_sig == CSW_SIGNATURE && csw_status == 0
    }

    pub unsafe fn read_sector(&mut self, lba: u64, data: &mut [u8; 512]) -> bool {
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
        self.bot_transfer(&cmd, false, &mut data.to_vec())
    }

    unsafe fn scsi_inquiry(&mut self) -> Option<[u8; 36]> {
        let mut buf = [0u8; 36];
        let mut cmd = [0u8; 16];
        cmd[0] = SCSI_INQUIRY;
        cmd[4] = 36;
        if self.bot_transfer(&cmd, true, &mut buf) { Some(buf) } else { None }
    }

    unsafe fn scsi_read_capacity(&mut self) -> Option<(u64, u16)> {
        let mut buf = [0u8; 8];
        let mut cmd = [0u8; 16];
        cmd[0] = SCSI_READ_CAPACITY;
        if !self.bot_transfer(&cmd, true, &mut buf) { return None; }
        let lba = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
        let blk_sz = u32::from_be_bytes([buf[4], buf[5], buf[6], buf[7]]);
        Some((lba as u64, blk_sz as u16))
    }

    unsafe fn alloc_dma(size: usize) -> Option<u64> {
        let pages = (size + 4095) / 4096;
        let mut g = crate::memory::GLOBAL_ALLOCATOR.lock();
        let a = g.as_mut()?;
        let f = a.allocate_contiguous(pages)?;
        Some(f.start_address().as_u64())
    }
}
