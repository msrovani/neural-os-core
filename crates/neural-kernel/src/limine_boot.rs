//! ADR-0065 / ADR-0062 E2 — entry Limine (feature `limine-boot`).
//! Os requests em `.requests` precisam estar aqui (linker section do bin).
//! O handoff é coletado pela crate k_nano e passado ao kernel_boot.

use core::ptr::null_mut;
use k_nano::limine::{
    self, BaseRevision, FramebufferRequest, HhdmRequest, MemmapRequest, ModuleRequest,
    RsdpRequest, StackSizeRequest, FRAMEBUFFER_ID, HHDM_ID, MEMMAP_ID, MODULE_ID,
    REQUESTS_END, REQUESTS_START, RSDP_ID, STACK_SIZE_ID,
};

// ─── Requests (secção .requests no linker) ────────────────────────────

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

// ─── Entry ────────────────────────────────────────────────────────────

/// Early serial string (ClaudioOS-style proof-of-life).
#[inline(always)]
fn early_serial(msg: &[u8]) {
    unsafe {
        for &b in msg {
            core::arch::asm!("out dx, al", in("dx") 0x3F8u16, in("al") b, options(nostack, preserves_flags));
        }
    }
}

/// ELF e_entry — Limine salta aqui.
#[no_mangle]
pub unsafe extern "C" fn _start() -> ! {
    limine_entry()
}

unsafe extern "C" fn limine_entry() -> ! {
    early_serial(b"[neural] Limine handoff\r\nL\r\nboot=limine\r\n");

    // 1. Coleta dados do handoff via crate k_nano
    let handoff = k_nano::limine::LimineHandoff::collect_from_requests(
        &LIMINE_HHDM,
        &LIMINE_MEMMAP,
        &LIMINE_RSDP,
    );

    // 2. Aplica HHDM offset globalmente
    limine::apply_hhdm(handoff.pm_offset);

    // 3. RSDP
    crate::acpi::set_boot_rsdp(handoff.rsdp);

    // 4. Framebuffer
    if !LIMINE_FB.response.is_null() {
        let fb_resp = &*LIMINE_FB.response;
        if fb_resp.framebuffer_count > 0 && !fb_resp.framebuffers.is_null() {
            let fbp = *fb_resp.framebuffers;
            if !fbp.is_null() {
                let f = &*fbp;
                jarbas_crate::display::fb::probe_raw_framebuffer(
                    f.address as u64,
                    f.width as u32,
                    f.height as u32,
                    f.pitch as u32,
                    f.bpp,
                    f.red_mask_shift == 0,
                );
            }
        }
    }

    // 5. Log módulos
    if !LIMINE_MODULES.response.is_null() {
        let mod_n = (*LIMINE_MODULES.response).module_count;
        early_serial(b"modules=");
        let d = b'0' + (mod_n.min(9) as u8);
        early_serial(&[d, b'\r', b'\n']);
    }

    // 6. Boot comum via handoff
    crate::kernel_boot(&handoff)
}
