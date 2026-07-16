//! JARBAS FB MMIO + double-buffer (ADR-0041 P4).
//! Contrata FB bootloader → Cap MAP_FB/WRITE_FB → backbuffer → present (vsync stub).

use alloc::vec::Vec;
use core::ptr::write_volatile;
use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use x86_64::registers::control::Cr3;
use x86_64::structures::paging::{PageTable, PageTableFlags, PhysFrame, Size4KiB};
use x86_64::VirtAddr;

use crate::address_space::{self, AddressSpace};
use crate::display::fb::GPU;
use crate::interrupts::TIMER_TICKS;
use crate::memory::PHYS_MEM_OFFSET;
use crate::serial_println;
use crate::syscall::{self, Cap, SYS_MAP_FB, SYS_PRESENT_FB};

/// VA base do FB no AddressSpace JARBAS (L4 idx 224+, após MVP C).
pub const JARBAS_FB_VA: u64 = 0x0000_7000_0010_0000;
/// Páginas FB mapeadas no PoC (prova MMIO; FB completo fica no VA kernel).
pub const DEMO_MAP_PAGES: usize = 8;
/// Retângulo demo no canto (evita alloc multi-MB no boot).
/// Cores (0,180,255)/(20,20,40) — o "xuvisco" azul/preto no canto superior ESQUERDO
/// na tela QEMU é este patch, não cursor/orb. `present()` copia DEMO_H scanlines;
/// após a prova Cap/AS, apagamos o residual + splash (ver fim de `demo_jarbas_fb`).
const DEMO_W: usize = 64;
const DEMO_H: usize = 64;

static PRESENT_COUNT: AtomicU64 = AtomicU64::new(0);
static VSYNC_WAITS: AtomicU64 = AtomicU64::new(0);
static P4_DEMO_OK: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);
/// P4 Cap-only path OK (sem GpuDevice físico) — aceite N5 em QEMU short boot.
static CAP_ONLY_OK: AtomicBool = AtomicBool::new(false);

pub fn cap_only_ok() -> bool {
    CAP_ONLY_OK.load(Ordering::Relaxed)
}

/// Contrato MMIO do framebuffer ativo (bootloader / GpuDevice).
#[derive(Clone, Copy, Debug)]
pub struct FbContract {
    pub virt_kernel: u64,
    pub phys_base: u64,
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    pub bpp: u32,
    pub rgb_order: bool,
}

fn phys_offset() -> u64 {
    PHYS_MEM_OFFSET.load(Ordering::Acquire)
}

unsafe fn frame_as_table(frame: PhysFrame<Size4KiB>) -> *mut PageTable {
    let virt = VirtAddr::new(phys_offset() + frame.start_address().as_u64());
    virt.as_mut_ptr()
}

/// Traduz VA → PA via page tables do CR3 atual (4K; rejeita huge no caminho final).
pub unsafe fn virt_to_phys(virt: u64) -> Result<u64, &'static str> {
    let va = VirtAddr::new(virt);
    let (l4_frame, _) = Cr3::read();
    let l4 = &*frame_as_table(l4_frame);
    let e4 = &l4[va.p4_index()];
    if !e4.flags().contains(PageTableFlags::PRESENT) {
        return Err("p4: L4 miss");
    }
    let l3 = &*frame_as_table(PhysFrame::containing_address(e4.addr()));
    let e3 = &l3[va.p3_index()];
    if !e3.flags().contains(PageTableFlags::PRESENT) {
        return Err("p4: L3 miss");
    }
    if e3.flags().contains(PageTableFlags::HUGE_PAGE) {
        let base = e3.addr().as_u64() & !((1u64 << 30) - 1);
        return Ok(base | (virt & ((1u64 << 30) - 1)));
    }
    let l2 = &*frame_as_table(PhysFrame::containing_address(e3.addr()));
    let e2 = &l2[va.p2_index()];
    if !e2.flags().contains(PageTableFlags::PRESENT) {
        return Err("p4: L2 miss");
    }
    if e2.flags().contains(PageTableFlags::HUGE_PAGE) {
        let base = e2.addr().as_u64() & !((1u64 << 21) - 1);
        return Ok(base | (virt & ((1u64 << 21) - 1)));
    }
    let l1 = &*frame_as_table(PhysFrame::containing_address(e2.addr()));
    let e1 = &l1[va.p1_index()];
    if !e1.flags().contains(PageTableFlags::PRESENT) {
        return Err("p4: L1 miss");
    }
    Ok(e1.addr().as_u64() | (virt & 0xFFF))
}

/// Obtém contrato do framebuffer UEFI/bootloader já probeado.
pub fn probe_contract() -> Result<FbContract, &'static str> {
    let guard = GPU.lock();
    let gpu = guard.as_ref().ok_or("p4: sem GpuDevice")?;
    if !gpu.present || gpu.fb_addr == 0 {
        return Err("p4: framebuffer ausente");
    }
    let virt = gpu.fb_addr;
    let phys = unsafe { virt_to_phys(virt)? };
    Ok(FbContract {
        virt_kernel: virt,
        phys_base: phys & !0xFFF,
        width: gpu.fb_width,
        height: gpu.fb_height,
        stride: gpu.fb_stride,
        bpp: gpu.fb_bpp,
        rgb_order: gpu.rgb_order,
    })
}

/// Mapeia primeiras `DEMO_MAP_PAGES` do FB no AS JARBAS (exige Cap::MAP_FB).
pub unsafe fn map_fb_pages(
    aspace: &mut AddressSpace,
    contract: &FbContract,
    held: Cap,
) -> Result<u64, &'static str> {
    if !held.contains(Cap::MAP_FB) {
        serial_println!(
            "[CapGate] DENY MAP_FB held=0x{:x}",
            held.bits()
        );
        return Err("EPERM: Cap::MAP_FB");
    }
    let _ = syscall::dispatch(SYS_MAP_FB, contract.phys_base, held)?;
    let flags = address_space::rw_flags();
    let base_phys = contract.phys_base;
    for i in 0..DEMO_MAP_PAGES {
        let va = VirtAddr::new(JARBAS_FB_VA + (i as u64) * 4096);
        let frame = PhysFrame::<Size4KiB>::containing_address(x86_64::PhysAddr::new(
            base_phys + (i as u64) * 4096,
        ));
        aspace.map_page(va, frame, flags)?;
    }
    Ok(JARBAS_FB_VA)
}

/// Backbuffer heap + flip para FB físico (gated por Cap::WRITE_FB).
pub struct JarbasDoubleBuffer {
    contract: FbContract,
    back: Vec<u8>,
    /// Altura efetiva do backbuffer (demo = DEMO_H, capped).
    buf_h: usize,
}

impl JarbasDoubleBuffer {
    pub fn new_demo(contract: FbContract) -> Self {
        let buf_h = DEMO_H.min(contract.height as usize);
        let len = buf_h * contract.stride as usize;
        Self {
            contract,
            back: alloc::vec![0u8; len],
            buf_h,
        }
    }

    pub fn draw_checker(&mut self, c0: (u8, u8, u8), c1: (u8, u8, u8)) {
        let bpp = self.contract.bpp as usize;
        let stride = self.contract.stride as usize;
        let w = DEMO_W.min(self.contract.width as usize);
        let rgb = self.contract.rgb_order;
        for y in 0..self.buf_h {
            for x in 0..w {
                let (r, g, b) = if ((x / 8) + (y / 8)) % 2 == 0 {
                    c0
                } else {
                    c1
                };
                let off = y * stride + x * bpp;
                if off + bpp > self.back.len() {
                    continue;
                }
                if rgb {
                    self.back[off] = r;
                    self.back[off + 1] = g;
                    self.back[off + 2] = b;
                } else {
                    self.back[off] = b;
                    self.back[off + 1] = g;
                    self.back[off + 2] = r;
                }
                if bpp > 3 {
                    self.back[off + 3] = 0xFF;
                }
            }
        }
    }

    /// Stub vsync: espera avanço de TIMER_TICKS (ou timeout busy-spin curto).
    pub fn wait_vsync_stub(&self) {
        let start = TIMER_TICKS.load(Ordering::Relaxed);
        let mut spins = 0u32;
        while TIMER_TICKS.load(Ordering::Relaxed) == start && spins < 50_000 {
            spins = spins.wrapping_add(1);
            core::hint::spin_loop();
        }
        VSYNC_WAITS.fetch_add(1, Ordering::Relaxed);
        unsafe {
            core::arch::asm!("sfence", options(nostack, preserves_flags));
        }
    }

    /// Present: copia back → FB kernel VA (canto superior). Cap::WRITE_FB.
    pub fn present(&mut self, held: Cap) -> Result<(), &'static str> {
        if !held.contains(Cap::WRITE_FB) {
            serial_println!(
                "[CapGate] DENY WRITE_FB held=0x{:x}",
                held.bits()
            );
            return Err("EPERM: Cap::WRITE_FB");
        }
        let _ = syscall::dispatch(SYS_PRESENT_FB, 0, held)?;
        self.wait_vsync_stub();
        let dst = self.contract.virt_kernel as *mut u8;
        let len = self.back.len();
        unsafe {
            for i in 0..len {
                write_volatile(dst.add(i), self.back[i]);
            }
        }
        PRESENT_COUNT.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}

pub fn present_count() -> u64 {
    PRESENT_COUNT.load(Ordering::Relaxed)
}

/// True se `demo_jarbas_fb` terminou OK (FB físico ou Cap-only sem GpuDevice).
pub fn p4_demo_ok() -> bool {
    P4_DEMO_OK.load(Ordering::Relaxed)
}

/// Demo non-fatal: Cap deny/allow + AS map + checker + present.
pub fn demo_jarbas_fb() -> Result<(), &'static str> {
    serial_println!("[P4] JARBAS FB MMIO + double-buffer demo");

    let contract = match probe_contract() {
        Ok(c) => {
            serial_println!(
                "[P4] FB contract {}x{} bpp={} stride={} virt={:x} phys={:x}",
                c.width,
                c.height,
                c.bpp,
                c.stride,
                c.virt_kernel,
                c.phys_base
            );
            c
        }
        Err(e) => {
            // Sem FB: ainda prova Cap deny/allow sem touch MMIO.
            serial_println!("[P4] {} — Cap-only path", e);
            if syscall::dispatch(SYS_MAP_FB, 0, Cap::EMPTY).is_ok() {
                return Err("p4: Cap vazia nao deveria MAP_FB");
            }
            syscall::dispatch(SYS_MAP_FB, 0, Cap::MAP_FB)?;
            if syscall::dispatch(SYS_PRESENT_FB, 0, Cap::EMPTY).is_ok() {
                return Err("p4: Cap vazia nao deveria PRESENT_FB");
            }
            syscall::dispatch(SYS_PRESENT_FB, 0, Cap::WRITE_FB)?;
            CAP_ONLY_OK.store(true, Ordering::Relaxed);
            P4_DEMO_OK.store(true, Ordering::Relaxed);
            serial_println!("[P4] SUCCESS Cap MAP_FB/WRITE_FB (sem FB fisico)");
            return Ok(());
        }
    };

    let (kernel_l4, kernel_flags) = address_space::kernel_cr3();
    let mut as_jarbas = AddressSpace::clone_current()?;
    if unsafe { map_fb_pages(&mut as_jarbas, &contract, Cap::EMPTY) }.is_ok() {
        return Err("p4: Cap vazia nao deveria mapear FB");
    }
    let mapped_va = unsafe { map_fb_pages(&mut as_jarbas, &contract, Cap::MAP_FB)? };

    // Prova escrita via VA JARBAS sob CR3 do AS (só primeiras páginas).
    let mark_ok = x86_64::instructions::interrupts::without_interrupts(|| unsafe {
        as_jarbas.activate();
        let p = mapped_va as *mut u32;
        let prev = p.read_volatile();
        p.write_volatile(0x00FF_00FF); // green-ish marker (BGRA/RGB depending)
        let got = p.read_volatile();
        p.write_volatile(prev);
        address_space::restore_cr3(kernel_l4, kernel_flags);
        got == 0x00FF_00FF
    });
    if !mark_ok {
        return Err("p4: escrita via JARBAS_FB_VA falhou");
    }

    let mut db = JarbasDoubleBuffer::new_demo(contract);
    if db.present(Cap::EMPTY).is_ok() {
        return Err("p4: Cap vazia nao deveria present");
    }
    db.draw_checker((0, 180, 255), (20, 20, 40));
    db.present(Cap::WRITE_FB.union(Cap::MAP_FB))?;

    // Apaga o checker residual (senão fica o bloco 64×64 no canto até o DisplayAgent).
    erase_present_region(&contract);
    crate::display::fb::boot_splash("AIOS");

    serial_println!(
        "[P4] SUCCESS MAP_FB+AS+present count={} vsync_waits={} (checker cleared+splash)",
        present_count(),
        VSYNC_WAITS.load(Ordering::Relaxed)
    );
    P4_DEMO_OK.store(true, Ordering::Relaxed);
    Ok(())
}

/// Zera as DEMO_H scanlines escritas por `present()` (stride completo).
fn erase_present_region(contract: &FbContract) {
    let h = DEMO_H.min(contract.height as usize);
    let len = h.saturating_mul(contract.stride as usize);
    if len == 0 || contract.virt_kernel == 0 {
        return;
    }
    unsafe {
        let dst = contract.virt_kernel as *mut u8;
        let mut i = 0usize;
        while i + 8 <= len {
            write_volatile(dst.add(i) as *mut u64, 0);
            i += 8;
        }
        while i < len {
            write_volatile(dst.add(i), 0);
            i += 1;
        }
    }
}
