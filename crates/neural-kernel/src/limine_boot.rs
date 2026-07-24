//! ADR-0065 — entry Limine (feature `limine-boot`). Dual-boot; path 0.11 intacto sem feature.
//! Layout de requests alinhado ao ClaudioOS / limine-rust-template.

use core::ptr::null_mut;
use k_nano::limine::{
    self, BaseRevision, FramebufferRequest, HhdmRequest, MemmapRequest, ModuleRequest,
    RsdpRequest, StackSizeRequest, FRAMEBUFFER_ID, HHDM_ID, MEMMAP_ID, MEMMAP_USABLE, MODULE_ID,
    REQUESTS_END, REQUESTS_START, RSDP_ID, STACK_SIZE_ID,
};

const MAX_USABLE: usize = 64;

static mut USABLE_RANGES: [(u64, u64); MAX_USABLE] = [(0, 0); MAX_USABLE];
static mut USABLE_COUNT: usize = 0;
static mut HHDM_OFFSET: u64 = 0;

#[used]
#[link_section = ".requests_start_marker"]
static LIMINE_REQUESTS_START: [u64; 4] = REQUESTS_START;

#[used]
#[link_section = ".requests"]
static mut LIMINE_BASE_REV: BaseRevision = BaseRevision::new(2);

#[used]
#[link_section = ".requests"]
static mut LIMINE_STACK: StackSizeRequest = StackSizeRequest {
    id: STACK_SIZE_ID,
    revision: 0,
    response: null_mut(),
    stack_size: 2 * 1024 * 1024,
};

#[used]
#[link_section = ".requests"]
static mut LIMINE_HHDM: HhdmRequest = HhdmRequest {
    id: HHDM_ID,
    revision: 0,
    response: null_mut(),
};

#[used]
#[link_section = ".requests"]
static mut LIMINE_MEMMAP: MemmapRequest = MemmapRequest {
    id: MEMMAP_ID,
    revision: 0,
    response: null_mut(),
};

#[used]
#[link_section = ".requests"]
static mut LIMINE_FB: FramebufferRequest = FramebufferRequest {
    id: FRAMEBUFFER_ID,
    revision: 0,
    response: null_mut(),
};

#[used]
#[link_section = ".requests"]
static mut LIMINE_RSDP: RsdpRequest = RsdpRequest {
    id: RSDP_ID,
    revision: 0,
    response: null_mut(),
};

/// Labor 18: Modules request — list only (BitNet load residual).
#[used]
#[link_section = ".requests"]
static mut LIMINE_MODULES: ModuleRequest = ModuleRequest {
    id: MODULE_ID,
    revision: 0,
    response: null_mut(),
    internal_module_count: 0,
    internal_modules: null_mut(),
};

#[used]
#[link_section = ".requests_end_marker"]
static LIMINE_REQUESTS_END_MARK: [u64; 2] = REQUESTS_END;

/// Early serial string (ClaudioOS-style proof-of-life).
#[inline(always)]
fn early_serial(msg: &[u8]) {
    unsafe {
        for &b in msg {
            core::arch::asm!("out dx, al", in("dx") 0x3F8u16, in("al") b, options(nostack, preserves_flags));
        }
    }
}

/// ELF e_entry — Limine salta aqui (sem Entry Point request; igual ClaudioOS).
#[no_mangle]
pub unsafe extern "C" fn _start() -> ! {
    limine_entry()
}

unsafe extern "C" fn limine_entry() -> ! {
    early_serial(b"[neural] Limine handoff\r\n");
    // Byte unico 'L' (smoke grep) + tag boot=limine (Labor 18)
    early_serial(b"L\r\nboot=limine\r\n");

    let hhdm_resp = LIMINE_HHDM.response;
    let offset = if !hhdm_resp.is_null() {
        (*hhdm_resp).offset
    } else {
        0
    };
    HHDM_OFFSET = limine::apply_hhdm(offset);

    // Labor 18: modules stub — count only
    let mod_resp = LIMINE_MODULES.response;
    let mod_n = if !mod_resp.is_null() {
        (*mod_resp).module_count
    } else {
        0
    };
    // early serial digits for modules=
    early_serial(b"modules=");
    {
        let d = b'0' + (mod_n.min(9) as u8);
        early_serial(&[d, b'\r', b'\n']);
    }

    let mut n = 0usize;
    let mm = LIMINE_MEMMAP.response;
    if !mm.is_null() {
        let count = (*mm).entry_count as usize;
        let entries = (*mm).entries;
        if !entries.is_null() {
            for i in 0..count {
                if n >= MAX_USABLE {
                    break;
                }
                let e = *entries.add(i);
                if e.is_null() {
                    continue;
                }
                let ent = &*e;
                if ent.entry_type == MEMMAP_USABLE {
                    USABLE_RANGES[n] = (ent.base, ent.length);
                    n += 1;
                }
            }
        }
    }
    USABLE_COUNT = n;

    let rsdp_phys = {
        let r = LIMINE_RSDP.response;
        if r.is_null() {
            None
        } else {
            limine::rsdp_phys(HHDM_OFFSET, (*r).address, 2)
        }
    };
    crate::acpi::set_boot_rsdp(rsdp_phys);

    let fb = LIMINE_FB.response;
    if !fb.is_null() && (*fb).framebuffer_count > 0 && !(*fb).framebuffers.is_null() {
        let fbp = *(*fb).framebuffers;
        if !fbp.is_null() {
            let f = &*fbp;
            let rgb = f.red_mask_shift == 0;
            crate::display::fb::probe_raw_framebuffer(
                f.address as u64,
                f.width as u32,
                f.height as u32,
                f.pitch as u32,
                f.bpp,
                rgb,
            );
        }
    }

    crate::kernel_boot(None)
}

pub fn usable_ranges() -> (&'static [(u64, u64)], usize) {
    unsafe {
        let n = USABLE_COUNT;
        (&USABLE_RANGES[..n], n)
    }
}

pub fn hhdm_offset() -> u64 {
    unsafe { HHDM_OFFSET }
}
