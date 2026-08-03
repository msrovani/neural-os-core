//! ADR-0077 — Conectores do Ring3 Isolation Ring (ex-ADR-0059 F6, ex-ADR-0060).
//!
//! **Porto seguro:** HOJE este módulo **NÃO registra** o ring nativo se o
//! hypervisor não for confiável (Phase 5). O ring Ring3 tem um BLOQUEADOR
//! conhecido em certos ambientes (TCG, WHPX instável). Enquanto nada é
//! registrado, `hermes_crate::app_factory::isolation_ring_available()` = `false`
//! e os Caminhos B/C (código nativo) permanecem **gated** — só o Caminho A
//! (wasmi) executa.
//!
//! Blocos a reusar (já no kernel):
//! - `crate::exec_arena` → W^X codegen (mapear RX; futuro: página USER no AS do sandbox)
//! - `crate::user_mode` → `enter_user_mode` (iretq p/ CPL=3), `fault_abort`, `TRY_ENTER_RING3`
//! - `crate::address_space` → AS/CR3 isolado (kernel supervisor-only + páginas user)
//! - `crate::syscall` + `crate::capability_gate` → gate de syscall + CapGate
//! - `crate::interrupts` → GDT user segs + TSS RSP0 + IST + handlers de falta

/// Chamado no boot. Registra o ring NATIVO se o ambiente for confiável
/// (Phase 5: hypervisor-aware gating).
///
/// Mantém o invariante do porto seguro: em TCG/desconhecido, nenhum código
/// nativo não-confiável roda (só wasmi A).
pub fn init_connectors() {
    if ring3_is_safe() {
        k_nano::slog_bin!("ISO-RING", "info", "Ring3 environment SAFE — registering native ring (B/C liberado sob HITL)");
        hermes_crate::app_factory::register_native_ring(ring3_run_native);
    } else {
        k_nano::slog_bin!(
            "ISO-RING",
            "info",
            "Ring3 environment UNSAFE — native ring NOT registered; wasmi (A) ativo"
        );
    }
}

/// Phase 5: Hypervisor-aware gating para Ring3.
/// Só libera Ring3 onde foi testado e é estável.
pub fn ring3_is_safe() -> bool {
    // Phase 0-4 completas: CR3 switch fix, TSS mutável, ABI registrador, AS sandbox
    // Phase 5: gating por hypervisor
    match k_nano::platform_probe::hypervisor() {
        k_nano::platform_probe::HypervisorKind::None => {
            // HW real — ideal, mas requer teste em HW específico
            // ponytail: gated até testar em HW real (AWAITING_HW)
            false
        }
        k_nano::platform_probe::HypervisorKind::Kvm => {
            // KVM é confiável para teste de Ring3
            true
        }
        k_nano::platform_probe::HypervisorKind::MicrosoftHv => {
            // WHPX: testar primeiro
            // ponytail: false até teste passar
            false
        }
        _ => {
            // TCG, VBox, VMware, Unknown → não testado
            false
        }
    }
}

/// **Execução isolada de código nativo em Ring3** (ADR-0077 §7 / ADR-0082 F3.2).
/// Assinatura casa com `hermes_crate::app_factory::NativeRingFn`.
///
/// Dual path:
///  1. `code` é ELF64 (`\x7fELF`) → `elf_loader::load_and_spawn` (create_sandbox_as
///     + RX/RW por segmento + relocations RELATIVE + stack USER);
///  2. `code` é blob nativo (Cranelift B/C) → `exec_arena::jit_write_exec_user`
///     (página USER RX no sandbox AS, W^X) + stack USER + `enter_user_mode`;
///  3. `user_mode::run_process` / `enter_user_mode` (iretq CPL=3) com Cap mínima;
///  4. fault no sandbox → `fault_abort` (mata sandbox, kernel vive).
pub fn ring3_run_native(code: &[u8], _caps: u32) -> Result<i64, &'static str> {
    // Path 1: ELF64 completo (loader com relocations).
    if crate::elf_loader::ElfLoader::is_valid_elf(code) {
        let pid = crate::elf_loader::load_and_spawn(code, "sandbox")?;
        k_nano::slog_bin!("ISO-RING", "info", "ring3_run_native: ELF sandbox pid={}", pid);
        return match crate::user_mode::run_process(pid) {
            Ok(()) => Ok(0),
            Err(e) => Err(e),
        };
    }

    // Path 2: blob nativo (Cranelift B/C) — arena USER RX no sandbox AS.
    if code.is_empty() {
        return Err("ring3: code vazio");
    }
    let mut aspace = crate::address_space::create_sandbox_as()?;
    let entry = unsafe { crate::exec_arena::jit_write_exec_user(&mut aspace, code) }?;

    // Stack USER RW no sandbox (4 páginas, constante do elf_loader).
    let stack_pages = (crate::elf_loader::USER_STACK_SIZE / 4096) as usize;
    for j in 0..stack_pages {
        let va = crate::elf_loader::USER_STACK_BASE + (j as u64) * 4096;
        let frame = crate::address_space::alloc_frame()?;
        unsafe {
            aspace.map_user_page(
                x86_64::VirtAddr::new(va),
                frame,
                crate::address_space::user_data_flags(),
            )?;
        }
    }
    let stack_top = crate::elf_loader::USER_STACK_BASE + crate::elf_loader::USER_STACK_SIZE;

    k_nano::slog_bin!("ISO-RING", "info", "ring3_run_native: blob @{:#x} stack @{:#x}", entry, stack_top);
    let result = unsafe {
        x86_64::instructions::interrupts::without_interrupts(|| {
            crate::user_mode::enter_user_mode(entry, stack_top, aspace.l4_frame, crate::syscall::Cap::ENTER_USER)
        })
    };
    match result {
        Ok(()) => Ok(0),
        Err(e) => Err(e),
    }
}
