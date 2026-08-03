//! ADR-0059 F7 — Arena de memória executável W^X (base do JIT nativo Cranelift).
//!
//! Primitivo de **execução de código nativo gerado on-device**: aloca frame,
//! mapeia RW num VA dedicado, escreve o código, **vira para RX** (remove
//! WRITABLE) e retorna um ponteiro de função. Garante que uma página **nunca é
//! escrita e executável ao mesmo tempo** (W^X pela flag WRITABLE).
//!
//! ⚠️ Isolamento: rodar código nativo aqui é em **Ring 0** (privilégio de
//! kernel). Para código **não-confiável** (IA), a execução SÓ é liberada com o
//! **ring de isolamento Ring3** (F6, ADR-0041) — por isso
//! `app_factory::isolation_ring_available()` permanece `false` até o Ring3.
//! Este arena é a peça de codegen; a de isolamento é o Ring3.
//!
//! Endurecimento futuro: NX na fase de escrita (W^X pleno), guard pages,
//! múltiplas páginas, e execução em Ring3/AS isolada.

use core::sync::atomic::Ordering;
use x86_64::registers::control::Cr3;
use x86_64::structures::paging::{PageTable, PageTableFlags, PhysFrame, Size4KiB};
use x86_64::VirtAddr;

use crate::memory::{alloc_physical_frame, PHYS_MEM_OFFSET};

/// VA base do arena — índice L4 dedicado (fora de heap=128 / MVP=224).
const ARENA_VA: u64 = 0x0000_5000_0000_0000;

fn phys_offset() -> u64 {
    PHYS_MEM_OFFSET.load(Ordering::Acquire)
}

unsafe fn table_mut(frame: PhysFrame<Size4KiB>) -> *mut PageTable {
    VirtAddr::new(phys_offset() + frame.start_address().as_u64()).as_mut_ptr()
}

unsafe fn alloc_zeroed() -> Option<PhysFrame<Size4KiB>> {
    let f = alloc_physical_frame()?;
    core::ptr::write_bytes((phys_offset() + f.start_address().as_u64()) as *mut u8, 0, 4096);
    Some(f)
}

/// Desce/cria o subtree do L4 ativo até a folha e a mapeia com `flags`.
/// Só cria subtree novo (bail se HUGE ou folha já presente) — não mexe em
/// tabelas compartilhadas do kernel.
unsafe fn map_leaf(
    virt: VirtAddr,
    frame: PhysFrame<Size4KiB>,
    flags: PageTableFlags,
) -> Result<(), &'static str> {
    let (l4f, _) = Cr3::read();
    let parent = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
    let idxs = [virt.p4_index(), virt.p3_index(), virt.p2_index()];
    let mut cur = l4f;
    for &idx in idxs.iter() {
        let t = &mut *table_mut(cur);
        let e = &mut t[idx];
        if e.flags().contains(PageTableFlags::HUGE_PAGE) {
            return Err("exec_arena: huge page no caminho");
        }
        if !e.flags().contains(PageTableFlags::PRESENT) {
            let nf = alloc_zeroed().ok_or("exec_arena: sem frame PT")?;
            e.set_addr(nf.start_address(), parent);
            cur = nf;
        } else {
            cur = PhysFrame::containing_address(e.addr());
        }
    }
    let l1 = &mut *table_mut(cur);
    let leaf = &mut l1[virt.p1_index()];
    if leaf.flags().contains(PageTableFlags::PRESENT) {
        return Err("exec_arena: VA ja mapeada");
    }
    leaf.set_addr(frame.start_address(), flags);
    x86_64::instructions::tlb::flush(virt);
    Ok(())
}

/// Atualiza as flags da folha (para o flip RW→RX).
unsafe fn set_leaf_flags(virt: VirtAddr, flags: PageTableFlags) -> Result<(), &'static str> {
    let (l4f, _) = Cr3::read();
    let idxs = [virt.p4_index(), virt.p3_index(), virt.p2_index()];
    let mut cur = l4f;
    for &idx in idxs.iter() {
        let t = &*table_mut(cur);
        let e = &t[idx];
        if !e.flags().contains(PageTableFlags::PRESENT) || e.flags().contains(PageTableFlags::HUGE_PAGE) {
            return Err("exec_arena: caminho inválido no flip");
        }
        cur = PhysFrame::containing_address(e.addr());
    }
    let l1 = &mut *table_mut(cur);
    let leaf = &mut l1[virt.p1_index()];
    let frame = PhysFrame::<Size4KiB>::containing_address(leaf.addr());
    leaf.set_addr(frame.start_address(), flags);
    x86_64::instructions::tlb::flush(virt);
    Ok(())
}

/// Aloca 1 página, escreve `code` (RW), vira RX e retorna o VA executável.
/// `code` deve caber em 4KiB. W^X: nunca WRITABLE+executável no fim.
pub unsafe fn jit_write_exec(code: &[u8]) -> Result<u64, &'static str> {
    if code.is_empty() || code.len() > 4096 {
        return Err("exec_arena: código vazio/grande demais");
    }
    // Guard: índice L4 do arena deve estar livre (não clobber).
    let virt = VirtAddr::new(ARENA_VA);
    let (l4f, _) = Cr3::read();
    {
        let l4 = &*table_mut(l4f);
        if l4[virt.p4_index()].flags().contains(PageTableFlags::PRESENT) {
            return Err("exec_arena: índice L4 ocupado (arena indisponível)");
        }
    }
    let frame = alloc_physical_frame().ok_or("exec_arena: sem frame código")?;
    // Fase escrita: RW (PRESENT|WRITABLE).
    map_leaf(virt, frame, PageTableFlags::PRESENT | PageTableFlags::WRITABLE)?;
    let dst = ARENA_VA as *mut u8;
    core::ptr::copy_nonoverlapping(code.as_ptr(), dst, code.len());
    core::sync::atomic::fence(Ordering::SeqCst);
    // Fase execução: RX (remove WRITABLE → W^X).
    set_leaf_flags(virt, PageTableFlags::PRESENT)?;
    Ok(ARENA_VA)
}

/// Self-test (Ring 0, código PRÓPRIO e confiável): monta `mov eax,42; ret`,
/// escreve no arena, vira RX, executa e confere 42. Prova execução nativa
/// gerada on-device (base do JIT Cranelift dos Caminhos B/C).
pub fn self_test() -> bool {
    // x86-64: B8 2A 00 00 00 (mov eax, 42) ; C3 (ret)
    let code: [u8; 6] = [0xB8, 0x2A, 0x00, 0x00, 0x00, 0xC3];
    let va = match unsafe { jit_write_exec(&code) } {
        Ok(v) => v,
        Err(e) => {
            k_nano::slog_bin!("EXEC-ARENA", "warn", "self-test FAIL setup: {}", e);
            return false;
        }
    };
    let f: unsafe extern "C" fn() -> u32 = unsafe { core::mem::transmute::<*const (), unsafe extern "C" fn() -> u32>(va as *const ()) };
    let r = unsafe { f() };
    if r == 42 {
        k_nano::slog_bin!(
            "EXEC-ARENA",
            "info",
            "W^X JIT self-test PASS (native mov eax,42;ret -> {}) — ADR-0059 F7",
            r
        );
        true
    } else {
        k_nano::slog_bin!("EXEC-ARENA", "warn", "self-test resultado inesperado: {}", r);
        false
    }
}

/// ADR-0082 F3.1 — arena W^X **USER** dentro do sandbox AS.
///
/// Igual ao `jit_write_exec` (RW→RX) mas mapeia a página com
/// `USER_ACCESSIBLE` no `AddressSpace` isolado (Ring3) — o código resultante
/// é executável em CPL=3, NÃO em Ring 0. Base para o Caminho B/C (Cranelift)
/// e para o `ring3_run_native` de blobs nativos.
///
/// W^X: fase escrita RW (USER|WRITABLE), flip para RX (remove WRITABLE).
/// Retorna o VA USER do código no sandbox.
pub unsafe fn jit_write_exec_user(
    aspace: &mut crate::address_space::AddressSpace,
    code: &[u8],
) -> Result<u64, &'static str> {
    if code.is_empty() || code.len() > 4096 {
        return Err("exec_arena: código vazio/grande demais");
    }
    // VA do arena (índice L4 dedicado, < 256 → range user) — mesmo base do
    // arena Ring 0; no sandbox AS este índice está LIVRE (create_sandbox_as
    // só copia P4[≥256]).
    let virt = VirtAddr::new(ARENA_VA);
    let frame = alloc_physical_frame().ok_or("exec_arena: sem frame código")?;
    // Fase escrita: USER RW (CPL=3 pode escrever).
    let write_flags = PageTableFlags::PRESENT
        | PageTableFlags::WRITABLE
        | PageTableFlags::USER_ACCESSIBLE;
    aspace.map_user_page(virt, frame, write_flags)?;
    // Escreve via HHDM no FRAME (o VA do arena só existe no sandbox AS —
    // escrever em ARENA_VA aqui, com o CR3 do kernel, daria #PF).
    let dst = crate::address_space::hhdm_mut::<u8>(frame) as *mut u8;
    core::ptr::copy_nonoverlapping(code.as_ptr(), dst, code.len());
    core::sync::atomic::fence(Ordering::SeqCst);
    // Fase execução: USER RX (remove WRITABLE → W^X). Preserva USER via set_user_leaf_flags.
    aspace.set_user_leaf_flags(virt, PageTableFlags::PRESENT)?;
    Ok(ARENA_VA)
}

/// Self-test do arena USER (ADR-0082 F3.1): monta sandbox, escreve
/// `mov eax,42; ret` USER RX e valida a folha + bytes via HHDM.
/// NÃO executa em Ring 0 (o VA do arena só existe no sandbox AS — executar
/// aqui com o CR3 do kernel daria #PF; a execução real é do
/// `ring3_run_native` em CPL=3).
pub fn user_arena_self_test() -> bool {
    let mut aspace = match crate::address_space::create_sandbox_as() {
        Ok(a) => a,
        Err(e) => {
            k_nano::slog_bin!("EXEC-ARENA", "warn", "user selftest: sandbox fail {}", e);
            return false;
        }
    };
    let code: [u8; 6] = [0xB8, 0x2A, 0x00, 0x00, 0x00, 0xC3];
    let va = match unsafe { jit_write_exec_user(&mut aspace, &code) } {
        Ok(v) => v,
        Err(e) => {
            k_nano::slog_bin!("EXEC-ARENA", "warn", "user selftest: jit fail {}", e);
            return false;
        }
    };
    // Valida 1) folha USER mapeada no sandbox; 2) bytes escritos (via HHDM).
    let frame = match aspace.frame_for_virt(VirtAddr::new(va)) {
        Some(f) => f,
        None => {
            k_nano::slog_bin!("EXEC-ARENA", "warn", "user selftest: folha nao mapeada");
            return false;
        }
    };
    let ptr = crate::address_space::hhdm_mut::<u8>(frame) as *const u8;
    let mut ok_bytes = true;
    for (i, &b) in code.iter().enumerate() {
        if unsafe { core::ptr::read_volatile(ptr.add(i)) } != b {
            ok_bytes = false;
        }
    }
    if !ok_bytes {
        k_nano::slog_bin!("EXEC-ARENA", "warn", "user selftest: bytes corrompidos");
        return false;
    }
    k_nano::slog_bin!(
        "EXEC-ARENA",
        "info",
        "W^X USER arena self-test PASS ({} bytes USER RX @{:#x} no sandbox) — ADR-0082 F3.1",
        code.len(),
        va
    );
    true
}
