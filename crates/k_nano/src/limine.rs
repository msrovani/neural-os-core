//! Limine boot protocol (ADR-0065) — requests HHDM / memmap / FB / RSDP.
//! Spike dual-boot: structs + helpers; entry fica no bin com feature `limine-boot`.

#![allow(dead_code)]

/// Magic comum a todos os request IDs (protocolo Limine).
pub const COMMON_MAGIC: [u64; 2] = [0xc7b1dd30_df4c8b88, 0x0a82e883_a194f07b];

pub const HHDM_ID: [u64; 4] = [
    COMMON_MAGIC[0],
    COMMON_MAGIC[1],
    0x48dcf1cb_8ad2b852,
    0x63984e95_9a98244b,
];
pub const MEMMAP_ID: [u64; 4] = [
    COMMON_MAGIC[0],
    COMMON_MAGIC[1],
    0x67cf3d9d_378a806f,
    0xe304acdf_c50c3c62,
];
pub const FRAMEBUFFER_ID: [u64; 4] = [
    COMMON_MAGIC[0],
    COMMON_MAGIC[1],
    0x9d5827dc_d881dd75,
    0xa3148604_f6fab11b,
];
pub const RSDP_ID: [u64; 4] = [
    COMMON_MAGIC[0],
    COMMON_MAGIC[1],
    0xc5e77b6b_397e7b43,
    0x27637845_accdcf3c,
];
pub const STACK_SIZE_ID: [u64; 4] = [
    COMMON_MAGIC[0],
    COMMON_MAGIC[1],
    0x224ef046_0a8e8926,
    0xe1cb0fc2_5f46ea3d,
];
pub const ENTRY_POINT_ID: [u64; 4] = [
    COMMON_MAGIC[0],
    COMMON_MAGIC[1],
    0x13d86c03_5a1cd3e1,
    0x2b0caa89_d8f3026a,
];
pub const MODULE_ID: [u64; 4] = [
    COMMON_MAGIC[0],
    COMMON_MAGIC[1],
    0x3e7e2797_02be32af,
    0xca1c4f3b_d1280cee,
];
/// MP/SMP — **opt-in** (`limine-smp`); default OFF (ADR-0055 SIPI sequencial).
pub const MP_ID: [u64; 4] = [
    COMMON_MAGIC[0],
    COMMON_MAGIC[1],
    0x95a67b81_9a1b857e,
    0xa0b61b72_3b6a73e0,
];

pub const REQUESTS_START: [u64; 4] = [
    0xf6b8f4b3_9de7d1ae,
    0xfab91a69_40fcb9cf,
    0x785c6ed0_15d3e316,
    0x181e920a_7852b9d9,
];
pub const REQUESTS_END: [u64; 2] = [0xadc0e053_1bb10d03, 0x9572709f_31764c62];

/// Base revision tag: magic + N. ADR-0065 pede rev 2.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct BaseRevision {
    pub magic0: u64,
    pub magic1: u64,
    pub revision: u64,
}

impl BaseRevision {
    pub const fn new(n: u64) -> Self {
        Self {
            magic0: 0xf9562b2d_5c95a6c8,
            magic1: 0x6a7b3849_44536bdc,
            revision: n,
        }
    }

    /// Bootloader zera `revision` se suportado; senão deixa N e magic1 vira loaded rev.
    #[inline]
    pub fn supported(&self) -> bool {
        self.revision == 0
    }
}

#[repr(C)]
pub struct RequestHeader {
    pub id: [u64; 4],
    pub revision: u64,
    pub response: *mut u8,
}

#[repr(C)]
pub struct HhdmResponse {
    pub revision: u64,
    pub offset: u64,
}

#[repr(C)]
pub struct HhdmRequest {
    pub id: [u64; 4],
    pub revision: u64,
    pub response: *mut HhdmResponse,
}

#[repr(C)]
pub struct MemmapEntry {
    pub base: u64,
    pub length: u64,
    pub entry_type: u64,
}

pub const MEMMAP_USABLE: u64 = 0;
pub const MEMMAP_BOOTLOADER_RECLAIMABLE: u64 = 5;

#[repr(C)]
pub struct MemmapResponse {
    pub revision: u64,
    pub entry_count: u64,
    pub entries: *mut *mut MemmapEntry,
}

#[repr(C)]
pub struct MemmapRequest {
    pub id: [u64; 4],
    pub revision: u64,
    pub response: *mut MemmapResponse,
}

pub const FRAMEBUFFER_RGB: u8 = 1;

#[repr(C)]
pub struct Framebuffer {
    pub address: *mut u8,
    pub width: u64,
    pub height: u64,
    pub pitch: u64,
    pub bpp: u16,
    pub memory_model: u8,
    pub red_mask_size: u8,
    pub red_mask_shift: u8,
    pub green_mask_size: u8,
    pub green_mask_shift: u8,
    pub blue_mask_size: u8,
    pub blue_mask_shift: u8,
    pub unused: [u8; 7],
    pub edid_size: u64,
    pub edid: *mut u8,
}

#[repr(C)]
pub struct FramebufferResponse {
    pub revision: u64,
    pub framebuffer_count: u64,
    pub framebuffers: *mut *mut Framebuffer,
}

#[repr(C)]
pub struct FramebufferRequest {
    pub id: [u64; 4],
    pub revision: u64,
    pub response: *mut FramebufferResponse,
}

#[repr(C)]
pub struct RsdpResponse {
    pub revision: u64,
    /// Base rev ≤2 / ≥4: virtual (HHDM). Rev 3: physical.
    pub address: *mut u8,
}

#[repr(C)]
pub struct RsdpRequest {
    pub id: [u64; 4],
    pub revision: u64,
    pub response: *mut RsdpResponse,
}

#[repr(C)]
pub struct StackSizeResponse {
    pub revision: u64,
}

#[repr(C)]
pub struct StackSizeRequest {
    pub id: [u64; 4],
    pub revision: u64,
    pub response: *mut StackSizeResponse,
    pub stack_size: u64,
}

pub type EntryPointFn = unsafe extern "C" fn() -> !;

#[repr(C)]
pub struct EntryPointResponse {
    pub revision: u64,
}

#[repr(C)]
pub struct EntryPointRequest {
    pub id: [u64; 4],
    pub revision: u64,
    pub response: *mut EntryPointResponse,
    pub entry: Option<EntryPointFn>,
}

/// `struct limine_file` (rev 0 subset) — modules / executable.
#[repr(C)]
pub struct File {
    pub revision: u64,
    pub address: *mut u8,
    pub size: u64,
    pub path: *const u8,
    pub cmdline: *const u8,
    pub media_type: u32,
    pub unused: u32,
    pub tftp_ip: u32,
    pub tftp_port: u32,
    pub partition_index: u32,
    pub mbr_disk_id: u32,
    pub gpt_disk_uuid: [u8; 16],
    pub gpt_part_uuid: [u8; 16],
    pub part_uuid: [u8; 16],
}

#[repr(C)]
pub struct ModuleResponse {
    pub revision: u64,
    pub module_count: u64,
    pub modules: *mut *mut File,
}

#[repr(C)]
pub struct ModuleRequest {
    pub id: [u64; 4],
    pub revision: u64,
    pub response: *mut ModuleResponse,
    pub internal_module_count: u64,
    pub internal_modules: *mut *mut u8,
}

#[repr(C)]
pub struct MpResponse {
    pub revision: u64,
    pub flags: u32,
    pub bsp_lapic_id: u32,
    pub cpu_count: u64,
    pub cpus: *mut *mut u8,
}

#[repr(C)]
pub struct MpRequest {
    pub id: [u64; 4],
    pub revision: u64,
    pub response: *mut MpResponse,
    pub flags: u64,
}

/// Aplica HHDM offset em `PHYS_MEM_OFFSET`. Retorna offset ou 0.
pub fn apply_hhdm(offset: u64) -> u64 {
    crate::memory::PHYS_MEM_OFFSET.store(offset, core::sync::atomic::Ordering::Release);
    offset
}

/// Converte ponteiro RSDP Limine → físico (rev 2: address é HHDM virt).
pub fn rsdp_phys(hhdm: u64, addr: *mut u8, base_rev_requested: u64) -> Option<u64> {
    if addr.is_null() {
        return None;
    }
    let a = addr as u64;
    // Rev 3 only returns physical; we request rev 2 → virt = phys + hhdm.
    if base_rev_requested == 3 {
        Some(a)
    } else if hhdm != 0 && a >= hhdm {
        Some(a - hhdm)
    } else {
        Some(a)
    }
}
