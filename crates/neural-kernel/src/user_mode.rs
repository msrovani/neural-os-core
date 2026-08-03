//! P6 — Ring3 user-mode real (ADR-0041): GDT user + iretq + stub + return via int 0x90.
//! Demo boot non-fatal; Cap::ENTER_USER gated. Só páginas USER dedicadas (não o heap).

use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use x86_64::registers::control::Cr3Flags;
use x86_64::structures::paging::{PhysFrame, Size4KiB};
use x86_64::VirtAddr;

use crate::address_space::self;
use crate::interrupts_ext::{user_code_selector, user_data_selector};
use crate::syscall::{self, Cap, SYS_EXIT_USER};

/// Região user isolada (após Cortex weights VA).
pub const USER_CODE_VA: u64 = 0x0000_7000_0030_0000;
pub const USER_STACK_VA: u64 = 0x0000_7000_0030_1000;
pub const USER_MARKER_VA: u64 = 0x0000_7000_0030_2000;
/// Marcador escrito pelo stub em CPL=3.
pub const RING3_MAGIC: u64 = 0x0033_5249_4E47_0001;

static DEMO_ACTIVE: AtomicBool = AtomicBool::new(false);
/// Evita storm: nested #PF durante abort não faz return/retry.
static ABORTING: AtomicBool = AtomicBool::new(false);
/// 1=ok 2=cap-deny-on-exit 3=fault 0=unset
static EXIT_OK: AtomicU64 = AtomicU64::new(0);
static SAVED_CR3: AtomicU64 = AtomicU64::new(0);
static SAVED_CR3_FLAGS: AtomicU64 = AtomicU64::new(0);

// Continuação kernel (CLI; single-threaded na demo P6).
// Gated by feature=ring3 (default on); cfg-d out when off since these are only
// accessed from enter_user_mode() and jump_back_to_kernel() (both TRY_ENTER_RING3-gated).
#[cfg(feature = "ring3")]
static mut SAVED_RIP: u64 = 0;
#[cfg(feature = "ring3")]
static mut SAVED_RSP: u64 = 0;
/// Callee-saved regs (rbx, rbp, r12-r15) salvos ANTES do iretq e restaurados
/// em "2:" (SESSION_233). O syscall handler (extern "x86-interrupt") salva
/// esses regs na stack do handler (RSP0); jump_back_to_kernel faz `jmp` direto
/// a "2:" PULANDO o epilogue do handler -> regs lixo -> epilogue de
/// enter_user_mode usa RBP corrompido -> ret para 0 -> #PF storm.
#[cfg(feature = "ring3")]
static mut SAVED_CALLEE: [u64; 6] = [0; 6];

/// Feature: `iretq` real. Default off — QEMU UEFI storm #PF (CR2=ip, err=0x10).
/// Cap deny path still runs; re-enable when clone CR3 maps kernel text.
pub const TRY_ENTER_RING3: bool = false; // Ring3 disabled — pages USER with USER_ACCESSIBLE, safe HHDM

#[inline]
pub fn demo_active() -> bool {
    DEMO_ACTIVE.load(Ordering::SeqCst) || ABORTING.load(Ordering::SeqCst)
}

/// Abort non-fatal de #GP/#PF durante a demo — restaura kernel e continua boot.
pub fn fault_abort(msg: &'static str) -> ! {
    // Idempotent: nested faults durante restore não reentram.
    if ABORTING.swap(true, Ordering::SeqCst) {
        // Já abortando — não retry (handler não deve return).
        loop {
            x86_64::instructions::hlt();
        }
    }
    DEMO_ACTIVE.store(false, Ordering::SeqCst);
    EXIT_OK.store(3, Ordering::SeqCst);
    // Lock-free: serial_println pode deadlock no IRQ path.
    crate::interrupts_ext::puts(b"[P6] WARN fault abort - restore CR3 + skip iretq\n");
    let _ = msg;
    unsafe { jump_back_to_kernel() }
}

/// Retorno do int 0x90 (SYS_EXIT_USER) — nunca volta ao epílogo de interrupt.
pub fn return_from_user(ok: bool) -> ! {
    EXIT_OK.store(if ok { 1 } else { 2 }, Ordering::SeqCst);
    unsafe { jump_back_to_kernel() }
}

unsafe fn jump_back_to_kernel() -> ! {
    DEMO_ACTIVE.store(false, Ordering::SeqCst);
    let cr3_addr = SAVED_CR3.load(Ordering::SeqCst);
    let cr3_flags = SAVED_CR3_FLAGS.load(Ordering::SeqCst);
    if cr3_addr != 0 {
        let frame = PhysFrame::<Size4KiB>::containing_address(x86_64::PhysAddr::new(cr3_addr));
        let flags = Cr3Flags::from_bits_truncate(cr3_flags);
        address_space::restore_cr3(frame, flags);
    }
    #[cfg(feature = "ring3")]
    let rip = SAVED_RIP;
    #[cfg(not(feature = "ring3"))]
    let rip = 0u64;
    #[cfg(feature = "ring3")]
    let rsp = SAVED_RSP;
    #[cfg(not(feature = "ring3"))]
    let rsp = 0u64;
    // Sem return point salvo (fault pré-asm) → não jmp 0 (nova storm).
    if rip == 0 || rsp == 0 {
        ABORTING.store(false, Ordering::SeqCst);
        crate::interrupts_ext::puts(b"[P6] WARN no SAVED_RIP - spin (no return)\n");
        loop {
            x86_64::instructions::hlt();
        }
    }
    ABORTING.store(false, Ordering::SeqCst);
    // CRITICO (SESSION_233): NAO zerar ds/es/ss aqui! Em long mode esses
    // segmentos tem base ignorada (SS.RPL ja = 0 vindo do TSS no int 0x90).
    // "xor ax, ax" para zerar os segmentos CLOBBERAVA o registro que o
    // compilador escolheu para o operando {rsp} (RAX) -> mov rsp, rax com
    // RAX=0 -> RSP=0 -> ret para RIP=0 -> #PF storm (CR2=rodata).
    //
    // Restaura callee-saved (rbx/rbp/r12-r15) que o syscall handler
    // (extern "x86-interrupt") clobberou na stack RSP0 e cujo epilogue
    // foi pulado pelo jmp. Aqui estamos em CPL=0 + kernel CR3 — statics
    // acessiveis. Se NAO restaurar, o epilogue de enter_user_mode usa
    // RBP lixo -> ret para 0 -> #PF storm.
    #[cfg(feature = "ring3")]
    {
        let sc = core::ptr::addr_of_mut!(SAVED_CALLEE) as u64;
        core::arch::asm!(
            "mov rbx, qword ptr [{sc} + 0*8]",
            "mov rbp, qword ptr [{sc} + 1*8]",
            "mov r12, qword ptr [{sc} + 2*8]",
            "mov r13, qword ptr [{sc} + 3*8]",
            "mov r14, qword ptr [{sc} + 4*8]",
            "mov r15, qword ptr [{sc} + 5*8]",
            sc = in(reg) sc,
            options(nostack)
        );
    }
    core::arch::asm!(
        "mov rsp, {rsp}",
        "jmp {rip}",
        rsp = in(reg) rsp,
        rip = in(reg) rip,
        options(noreturn)
    );
}

/// Enter CPL=3 via `iretq`. Exige Cap::ENTER_USER. Retorna após SYS_EXIT_USER ou fault.
pub unsafe fn enter_user_mode(
    entry: u64,
    user_stack: u64,
    user_l4: PhysFrame<Size4KiB>,
    held: Cap,
) -> Result<(), &'static str> {
    if !held.contains(Cap::ENTER_USER) {
        k_nano::slog_bin!("CapGate", "info", "DENY ENTER_USER held=0x{:x}", held.bits());
        return Err("EPERM: Cap::ENTER_USER");
    }
    if !TRY_ENTER_RING3 {
        return Err("P6: TRY_ENTER_RING3=false");
    }

    let (k_l4, k_flags) = address_space::kernel_cr3();
    SAVED_CR3.store(k_l4.start_address().as_u64(), Ordering::SeqCst);
    SAVED_CR3_FLAGS.store(k_flags.bits(), Ordering::SeqCst);
    EXIT_OK.store(0, Ordering::SeqCst);

    syscall::stage_syscall(SYS_EXIT_USER, 0, Cap::ENTER_USER);

    let ucs = user_code_selector().0 as u64;
    let uds = user_data_selector().0 as u64;

    let mut rflags: u64;
    core::arch::asm!("pushfq; pop {}", out(reg) rflags, options(nostack));
    rflags &= !0x200; // IF=0; int software ainda funciona

    // --- PHASE 0: CR3 switch ANTES do iretq (Moros pattern) ---
    // 1. Salva kernel RSP para o return path (jump_back_to_kernel restaura)
    crate::interrupts_ext::puts(b"[P6] A: save rsp\n");
    let rsp_val: u64;
    core::arch::asm!("mov {}, rsp", out(reg) rsp_val, options(nostack));
    #[cfg(feature = "ring3")]
    { SAVED_RSP = rsp_val; }

    // 2. Salva callee-saved (rbx/rbp/r12-r15) que o syscall handler
    //    clobbera na stack RSP0 e o jump_back_to_kernel nao restaura.
    #[cfg(feature = "ring3")]
    {
        let p = core::ptr::addr_of_mut!(SAVED_CALLEE) as u64;
        core::arch::asm!(
            "mov qword ptr [{p} + 0*8], rbx",
            "mov qword ptr [{p} + 1*8], rbp",
            "mov qword ptr [{p} + 2*8], r12",
            "mov qword ptr [{p} + 3*8], r13",
            "mov qword ptr [{p} + 4*8], r14",
            "mov qword ptr [{p} + 5*8], r15",
            p = in(reg) p,
            options(nostack)
        );
    }

    // 3. Switch para page table do user enquanto ainda CPL=0.
    //    Kernel text em P4[511] é compartilhado (clone raso) → ainda executável.
    DEMO_ACTIVE.store(true, Ordering::SeqCst);
    crate::interrupts_ext::puts(b"[P6] B: cr3->user\n");
    address_space::restore_cr3(user_l4, Cr3Flags::empty());
    crate::interrupts_ext::puts(b"[P6] C: cr3 switched (CPL0)\n");

    // 4. IRETQ para CPL=3. CR3 já é a page table do user.
    //    A label "2:" é o return point — jump_back_to_kernel restaura CR3,
    //    restaura callee-saved (lê SAVED_CALLEE em CPL=0/kernel CR3) e salta
    //    pra cá; então o epílogo do compilador retorna ao caller.
    #[cfg(feature = "ring3")]
    let rip_ptr = core::ptr::addr_of_mut!(SAVED_RIP);
    #[cfg(not(feature = "ring3"))]
    let rip_ptr = core::ptr::null_mut::<u64>();
    crate::interrupts_ext::puts(b"[P6] D: iretq->CPL3\n");
    core::arch::asm!(
        "lea {tmp}, [rip + 2f]",
        "mov qword ptr [{rip_ptr}], {tmp}",
        "mov ax, {uds:x}",
        "mov ds, ax",
        "mov es, ax",
        "push {uds}",
        "push {stack}",
        "push {rflags}",
        "push {ucs}",
        "push {entry}",
        "iretq",
        "2:",
        tmp = out(reg) _,
        rip_ptr = in(reg) rip_ptr,
        uds = in(reg) uds,
        ucs = in(reg) ucs,
        stack = in(reg) user_stack,
        rflags = in(reg) rflags,
        entry = in(reg) entry,
    );
    crate::interrupts_ext::puts(b"[P6] E: returned from CPL3\n");

    DEMO_ACTIVE.store(false, Ordering::SeqCst);
    // CRITICO (SESSION_233): NAO chamar restore_cr3(k_l4, k_flags) aqui!
    // k_l4/k_flags sao locals em registros callee-saved que o
    // jump_back_to_kernel NAO restaura (clobbered pelo handler
    // extern "x86-interrupt") — usar lixo aqui = CR3 invalido = triple fault.
    // O CR3 do kernel JÁ foi restaurado pelo jump_back_to_kernel via SAVED_CR3.
    core::arch::asm!("mov ss, ax", in("ax") 0u16, options(nostack, preserves_flags));

    match EXIT_OK.load(Ordering::SeqCst) {
        1 => Ok(()),
        2 => Err("EPERM: Cap::ENTER_USER (exit)"),
        3 => Err("P6: fault during Ring3"),
        _ => Err("P6: enter_user sem EXIT"),
    }
}

fn write_stub(code: PhysFrame<Size4KiB>) {
    // movabs rax, MARKER; movabs rcx, MAGIC; mov [rax], rcx; int 0x90; hlt
    let mut buf = [0u8; 40];
    let mut o = 0usize;
    buf[o] = 0x48;
    buf[o + 1] = 0xB8;
    o += 2;
    buf[o..o + 8].copy_from_slice(&USER_MARKER_VA.to_le_bytes());
    o += 8;
    buf[o] = 0x48;
    buf[o + 1] = 0xB9;
    o += 2;
    buf[o..o + 8].copy_from_slice(&RING3_MAGIC.to_le_bytes());
    o += 8;
    buf[o] = 0x48;
    buf[o + 1] = 0x89;
    buf[o + 2] = 0x08;
    o += 3;
    buf[o] = 0xCD;
    buf[o + 1] = 0x90;
    o += 2;
    buf[o] = 0xF4;
    o += 1;
    let dst = address_space::hhdm_mut::<u8>(code);
    unsafe {
        core::ptr::copy_nonoverlapping(buf.as_ptr(), dst, o);
    }
}

/// Run a userspace process at Ring3 (CPL=3).
/// Loads the process's AddressSpace, switches CR3, and enters user mode via iretq.
/// Returns Ok(()) on clean exit (SYS_EXIT_USER), Err on fault or Cap deny.
pub fn run_process(pid: u64) -> Result<(), &'static str> {
    let (entry, stack_top, l4_frame) = {
        let pm = crate::process::PROCESS_MANAGER.lock();
        let p = pm.get(pid).ok_or("process: PID not found")?;
        (p.entry, p.stack_top, p.address_space.l4_frame)
    };

    {
        let mut pm = crate::process::PROCESS_MANAGER.lock();
        if let Some(p) = pm.get_mut(pid) {
            p.state = crate::process::ProcessState::Running;
        }
    }

    // ADR-0082 F1.2: per-process TSS — seleciona slot com kernel stack dedicada
    // (RSP0 para traps CPL=3→0). Slot = pid % MAX_PROCS.
    crate::interrupts_ext::switch_to_proc_tss((pid % 8) as usize);

    let result = unsafe {
        x86_64::instructions::interrupts::without_interrupts(|| {
            enter_user_mode(entry, stack_top, l4_frame, Cap::ENTER_USER)
        })
    };

    {
        let mut pm = crate::process::PROCESS_MANAGER.lock();
        if let Some(p) = pm.get_mut(pid) {
            p.state = crate::process::ProcessState::Exited(if result.is_ok() { 0 } else { 1 });
        }
    }

    result
}

/// ADR-0082 F2.2: executa um ELF64 em Ring3 dentro de um sandbox isolado.
/// Carrega via `elf_loader` (create_sandbox_as + RX/RW + relocations),
/// entra em CPL=3 com `enter_user_mode`, e retorna após SYS_EXIT_USER/fault.
pub fn run_elf(data: &[u8]) -> Result<(), &'static str> {
    let loaded = crate::elf_loader::load_and_spawn(data, "ring3-elf")?;
    // load_and_spawn registra no PROCESS_MANAGER; executa o processo recém-criado.
    run_process(loaded)
}

/// Demo non-fatal: deny Cap → map stub USER → iretq → marker → EXIT → SUCCESS.
    pub fn demo_ring3() -> Result<(), &'static str> {
        // Fixed: now uses create_sandbox_as() instead of clone_current() (no higher-half overflow)
        if !TRY_ENTER_RING3 {
        return Ok(());
    }
    k_nano::slog_bin!("P6", "info", "Ring3 user-mode demo");

    // Higher-half safety check: PHYS_MEM_OFFSET must be valid (non-zero).
    // Without it, HHDM access and pointer arithmetic will overflow isize::MAX.
    let pm_off = k_nano::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
    if pm_off == 0 {
        k_nano::slog_bin!("P6", "info", "PHYS_MEM_OFFSET=0 — Ring3 demo SKIP (higher-half req)");
        return Ok(());
    }

    let deny = unsafe {
        enter_user_mode(
            USER_CODE_VA,
            USER_STACK_VA + 0x1000,
            address_space::kernel_cr3().0,
            Cap::EMPTY,
        )
    };
    if deny.is_ok() {
        return Err("P6: Cap vazia nao deveria entrar");
    }
    k_nano::slog_bin!("P6", "info", "Cap::ENTER_USER deny OK");

    let mut as_user = address_space::create_sandbox_as()?;
    let code_frame = address_space::alloc_frame()?;
    let stack_frame = address_space::alloc_frame()?;
    let marker_frame = address_space::alloc_frame()?;

    write_stub(code_frame);
    unsafe {
        as_user.map_user_page(
            VirtAddr::new(USER_CODE_VA),
            code_frame,
            address_space::user_code_flags(),
        )?;
        as_user.map_user_page(
            VirtAddr::new(USER_STACK_VA),
            stack_frame,
            address_space::user_data_flags(),
        )?;
        as_user.map_user_page(
            VirtAddr::new(USER_MARKER_VA),
            marker_frame,
            address_space::user_data_flags(),
        )?;
        address_space::hhdm_mut::<u64>(marker_frame).write_volatile(0);
    }

    let stack_top = USER_STACK_VA + 0x1000;
    x86_64::instructions::interrupts::without_interrupts(|| unsafe {
        enter_user_mode(
            USER_CODE_VA,
            stack_top,
            as_user.l4_frame,
            Cap::ENTER_USER,
        )
    })?;

    let marker = unsafe { address_space::hhdm_mut::<u64>(marker_frame).read_volatile() };
    if marker != RING3_MAGIC {
        return Err("P6: marker Ring3 nao escrito");
    }

    k_nano::slog_bin!("P6", "info", "SUCCESS iretq+CPL3 marker={:x} Cap::ENTER_USER", marker);
    Ok(())
}
