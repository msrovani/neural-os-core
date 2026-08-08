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
// Limine (protocolo): 0=USABLE 1=RESERVED 2=ACPI_RECLAIMABLE 3=ACPI_NVS
// 4=BAD_MEMORY 5=BOOTLOADER_RECLAIMABLE 6=KERNEL_AND_MODULES.
pub const MEMMAP_KERNEL_AND_MODULES: u64 = 6;
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
    /// Endereço físico onde o Limine alocou a stack (limine_stack_size_response).
    pub address: u64,
}

#[repr(C)]
pub struct StackSizeRequest {
    pub id: [u64; 4],
    pub revision: u64,
    pub response: *mut StackSizeResponse,
    pub stack_size: u64,
}

pub type EntryPointFn = unsafe extern "C" fn() -> !;

/// `struct limine_kernel_address_response` — onde o Limine carregou o kernel.
#[repr(C)]
pub struct KernelAddressResponse {
    pub revision: u64,
    /// Endereço físico onde o kernel foi carregado (page-aligned).
    pub physical_base: u64,
    /// Endereço virtual (higher-half) onde o kernel foi carregado.
    pub virtual_base: u64,
}

/// `struct limine_kernel_address_request` — pede para o Limine reportar onde
/// o kernel vive. SEM este request o memmap pode reportar a RAM do kernel
/// como USABLE → frame allocator entrega frames do kernel/.bss.heap para DMA
/// (e1000 RX buffer) → NIC sobrescreve o heap (conn.buf) → corrupção com
/// tamanho exato (SESSION_252/ora-1). Com ele, o Limine marca o kernel como
/// KernelAndModules (tipo 1) — o filtro MEMMAP_USABLE exclui naturalmente.
#[repr(C)]
pub struct KernelAddressRequest {
    pub id: [u64; 4],
    pub revision: u64,
    pub response: *mut KernelAddressResponse,
}

/// limine_kernel_address_request id (limine.h).
pub const KERNEL_ADDRESS_ID: [u64; 4] = [
    0x4d7a142ed453c958,
    0xedab48064cbade10,
    0x0f1f0f2b44cab7dc,
    0xac4e0519e23c6c2e,
];

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

// ─── BootHandoff impl (ADR-0062 E2) ──────────────────────────────────────
use crate::boot_handoff::{BootHandoff, MemRegion};

/// Coleta os dados do handoff Limine para o trait `BootHandoff`.
/// Copia as informações dos requests; as seções `.requests` ficam no bin.
pub struct LimineHandoff {
    pub pm_offset: u64,
    pub rsdp: Option<u64>,
    pub regions: [MemRegion; 64],
    pub region_count: usize,
    /// Endereço físico onde o Limine carregou o kernel (KernelAddressRequest).
    /// Usado para marcar a imagem do kernel (incl. .bss.heap) como OCUPADA no
    /// frame allocator — SESSION_252/ora-1: sem isso o allocator pode entregar
    /// frames do kernel para DMA (e1000 RX) e o NIC sobrescreve o heap.
    pub kernel_phys: u64,
    /// Região KernelAndModules (tipo 1 do memmap) — fallback quando o
    /// KernelAddressRequest não é processado (response null nesta build).
    /// SESSION_252: o memmap SEMPRE reporta o kernel como tipo 1; sem este
    /// fallback a imagem do kernel fica marcada como USABLE → DMA corrompe.
    pub kernel_region: (u64, u64),
}

impl LimineHandoff {
    pub const fn new() -> Self {
        Self {
            pm_offset: 0,
            rsdp: None,
            regions: [MemRegion { base: 0, len: 0 }; 64],
            region_count: 0,
            kernel_phys: 0,
            kernel_region: (0, 0),
        }
    }

    /// Lê os responses dos requests HHDM, memmap, RSDP e KernelAddress.
    /// `apply_hhdm` e `set_boot_rsdp` continuam a cargo do entry.
    pub fn collect_from_requests(
        hhdm: &HhdmRequest,
        memmap: &MemmapRequest,
        rsdp: &RsdpRequest,
        kaddr: &KernelAddressRequest,
    ) -> Self {
        let mut h = Self::new();

        // HHDM offset — com safety check p/ ponteiro físico vs HHDM-virtual
        let offset = if !hhdm.response.is_null() {
            let ptr = hhdm.response as u64;
            // Rev 2+: response ptr é HHDM-virtual (> 0xFFFF800000000000).
            // Rev 0: response ptr é físico (< 1MB). Sem HHDM conhecido, tentamos
            // deref direto (bootloader pode ter identity-map baixa memória).
            unsafe {
                if ptr >= 0xFFFF800000000000 {
                    // HHDM-virtual — deref direto
                    (*hhdm.response).offset
                } else {
                    // Possível físico — tenta deref (pode ser identity-mapped)
                    // ponytail: se #PF aqui, bootloader não identity-maps baixa memória
                    let phys_ptr = ptr as *const HhdmResponse;
                    (*phys_ptr).offset
                }
            }
        } else {
            0
        };
        h.pm_offset = offset;

        // Memmap — apenas usable (e guarda a região do kernel, tipo 1)
        if !memmap.response.is_null() {
            let mm = unsafe { &*memmap.response };
            let count = mm.entry_count as usize;
            for i in 0..core::cmp::min(count, 64) {
                let e = unsafe { *mm.entries.add(i) };
                if !e.is_null() {
                    let ent = unsafe { &*e };
                    if ent.entry_type == MEMMAP_USABLE {
                        h.regions[h.region_count] = MemRegion {
                            base: ent.base,
                            len: ent.length,
                        };
                        h.region_count += 1;
                    } else if ent.entry_type == MEMMAP_KERNEL_AND_MODULES && ent.length > 0 {
                        // SESSION_252: fallback do KernelAddressRequest (response null
                        // nesta build do Limine). Guarda a MAIOR região tipo 6 (kernel
                        // com .bss.heap ~522MB; entradas menores são módulos/ACPI).
                        if ent.length > h.kernel_region.1 {
                            h.kernel_region = (ent.base, ent.length);
                        }
                    }
                }
            }
        }

        // RSDP — subtrai HHDM para obter físico
        if !rsdp.response.is_null() {
            let r = unsafe { &*rsdp.response };
            let virt = r.address as u64;
            h.rsdp = if virt != 0 {
                if offset != 0 && virt >= offset {
                    Some(virt - offset)
                } else {
                    Some(virt)
                }
            } else {
                None
            };
        }

        // KernelAddress — onde o Limine carregou o kernel (físico).
        // SESSION_252: debug do request — se response null, o Limine não
        // processou o KernelAddressRequest (ID/versão da build).
        crate::slog_nano!(
            "LIMINE",
            "info",
            "kaddr_response_ptr={:#x} (null={})",
            kaddr.response as u64,
            kaddr.response.is_null() as u8
        );
        if !kaddr.response.is_null() {
            let r = unsafe { &*kaddr.response };
            crate::slog_nano!(
                "LIMINE",
                "info",
                "kernel_phys={:#x} kernel_virt={:#x} rev={}",
                r.physical_base,
                r.virtual_base,
                r.revision
            );
            h.kernel_phys = r.physical_base;
        }

        h
    }
}

impl BootHandoff for LimineHandoff {
    fn phys_mem_offset(&self) -> u64 {
        self.pm_offset
    }
    fn rsdp_addr(&self) -> Option<u64> {
        self.rsdp
    }
    fn boot_tag(&self) -> &'static str {
        "limine"
    }
    fn usable_regions(&self) -> &[MemRegion] {
        &self.regions[..self.region_count]
    }
    fn kernel_phys(&self) -> Option<u64> {
        if self.kernel_phys != 0 {
            Some(self.kernel_phys)
        } else {
            None
        }
    }
    fn kernel_region(&self) -> (u64, u64) {
        self.kernel_region
    }
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
