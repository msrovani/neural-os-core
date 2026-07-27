//! ADR-0077 — Conectores do Ring3 Isolation Ring (ex-ADR-0059 F6).
//!
//! **Porto seguro:** HOJE este módulo **NÃO registra** nenhum ring nativo — o
//! ring Ring3 tem um BLOQUEADOR conhecido (habilitar o `iretq` real hoje causa
//! triple-fault → reboot loop; ver ADR-0077 §3/§4). Enquanto nada é registrado,
//! `hermes_crate::app_factory::isolation_ring_available()` = `false` e os
//! Caminhos B/C (código nativo) permanecem **gated** — só o Caminho A (wasmi)
//! executa.
//!
//! Quando o F6 for validado (ADR-0077 §6), `init_connectors()` chamará
//! `hermes_crate::app_factory::register_native_ring(ring3_run_native)` e
//! `ring3_run_native` passará a executar o blob nativo em CPL=3 isolado.
//! Este é o **site de implementação**.
//!
//! Blocos a reusar (já no kernel):
//! - `crate::exec_arena` → W^X codegen (mapear RX; futuro: página USER no AS do
//!   sandbox)
//! - `crate::user_mode` → `enter_user_mode` (iretq p/ CPL=3), `fault_abort`,
//!   `TRY_ENTER_RING3`
//! - `crate::address_space` → AS/CR3 isolado (kernel supervisor-only + páginas
//!   user)
//! - `crate::syscall` + `crate::capability_gate` → gate de syscall + CapGate
//! - `crate::interrupts` → GDT user segs + TSS RSP0 + IST + handlers de falta

/// Chamado no boot. **Não registra** o ring até o F6 passar o gate (ADR-0077 §6).
/// Mantém o invariante do porto seguro: nenhum código nativo não-confiável roda.
pub fn init_connectors() {
    // F6 pendente — ver ADR-0077. NÃO descomentar o register até §6 passar:
    //   hermes_crate::app_factory::register_native_ring(ring3_run_native);
    k_nano::slog_bin!(
        "ISO-RING",
        "info",
        "Ring3 isolation ring NAO pronto (ADR-0077 F6) — B/C nativo gated; wasmi (A) ativo"
    );
}

/// **Site futuro** da execução isolada de código nativo em Ring3 (ADR-0077 §7).
/// Assinatura casa com `hermes_crate::app_factory::NativeRingFn`.
///
/// Implementação futura (resumo §7):
///  1. montar código no `exec_arena` como página **USER RX** no AS do sandbox;
///  2. `AddressSpace` isolado (kernel supervisor-only; IST/IDT/handlers
///     alcançáveis);
///  3. `enter_user_mode` (iretq CPL=3) com `Cap` mínima;
///  4. syscalls do sandbox mediadas por `capability_gate` (DMA/MMIO negadas);
///  5. falta no sandbox → `fault_abort` (mata sandbox, kernel vive).
#[allow(dead_code)]
pub fn ring3_run_native(_code: &[u8], _caps: u32) -> Result<i64, &'static str> {
    Err("ADR-0077 F6: Ring3 isolation ring nao implementado (ver secoes 6/7)")
}
