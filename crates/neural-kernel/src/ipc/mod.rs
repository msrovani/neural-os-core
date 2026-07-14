//! IPC lock-free cross address-space — MVP C (ADR-0041).
//! Demo: 2 CR3 + ring SPSC em página compartilhada + Cap/syscall.

mod ring_buffer;

pub use ring_buffer::SharedSpscRing;

use crate::address_space::{self, AddressSpace, PRIVATE_PAGE_VA, SHARED_RING_VA};
use crate::serial_println;
use crate::syscall::{self, Cap, SYS_PING};
use x86_64::VirtAddr;

/// Prova de conceito: dois AS, troca CR3, ring shared, Cap::PING via int 0x90.
/// Non-fatal no boot — chamador loga WARN em Err.
pub fn demo_two_spaces() -> Result<(), &'static str> {
    serial_println!("[MVP-C] iniciando demo CR3 + ring + capability");

    let (kernel_l4, kernel_flags) = address_space::kernel_cr3();
    let mut as_a = AddressSpace::clone_current()?;
    let mut as_b = AddressSpace::clone_current()?;

    let shared = address_space::alloc_frame()?;
    let priv_a = address_space::alloc_frame()?;
    let priv_b = address_space::alloc_frame()?;

    let flags = address_space::rw_flags();
    let shared_va = VirtAddr::new(SHARED_RING_VA);
    let priv_va = VirtAddr::new(PRIVATE_PAGE_VA);

    unsafe {
        as_a.map_page(shared_va, shared, flags)?;
        as_b.map_page(shared_va, shared, flags)?;
        as_a.map_page(priv_va, priv_a, flags)?;
        as_b.map_page(priv_va, priv_b, flags)?;
        SharedSpscRing::init_at(address_space::hhdm_mut::<SharedSpscRing>(shared));
    }

    let magic: [u8; 3] = [0xC0, 0xFF, 0xEE];
    const PRIV_MARK: u64 = 0xAAAA_BBBB_CCCC_DDDD;

    let switch_result = x86_64::instructions::interrupts::without_interrupts(|| unsafe {
        as_a.activate();
        let ring_a = &*(SHARED_RING_VA as *const SharedSpscRing);
        let mut push_err: Option<&'static str> = None;
        for &b in &magic {
            if let Err(e) = ring_a.push(b) {
                push_err = Some(e);
                break;
            }
        }
        (PRIVATE_PAGE_VA as *mut u64).write_volatile(PRIV_MARK);

        as_b.activate();
        let ring_b = &*(SHARED_RING_VA as *const SharedSpscRing);
        let mut got = [0u8; 3];
        let mut pop_err: Option<&'static str> = None;
        for g in &mut got {
            match ring_b.pop() {
                Ok(b) => *g = b,
                Err(e) => {
                    pop_err = Some(e);
                    break;
                }
            }
        }
        let isolated = (PRIVATE_PAGE_VA as *const u64).read_volatile();

        address_space::restore_cr3(kernel_l4, kernel_flags);

        if let Some(e) = push_err {
            return Err(e);
        }
        if let Some(e) = pop_err {
            return Err(e);
        }
        if got != magic {
            return Err("mvp-c: ring shared divergiu");
        }
        if isolated == PRIV_MARK {
            return Err("mvp-c: isolamento privado falhou");
        }
        Ok(())
    });
    switch_result?;

    let n = syscall::soft_syscall(SYS_PING, 0, Cap::PING)?;
    if n == 0 {
        return Err("mvp-c: ping count zero");
    }
    if syscall::soft_syscall(SYS_PING, 0, Cap::EMPTY).is_ok() {
        return Err("mvp-c: Cap vazia nao deveria passar");
    }
    let _ = syscall::dispatch(
        SYS_PING,
        0,
        Cap::PING.union(Cap::WRITE_RING).union(Cap::READ_RING),
    )?;

    serial_println!(
        "[MVP-C] SUCCESS cr3-switch + shared-ring + Cap::PING (count={})",
        syscall::ping_count()
    );
    // TODO Ring3: stub user + iretq; PoC atual e Ring0↔Ring0 com CR3 distintos.
    Ok(())
}
