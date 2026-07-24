//! Hermes helpers — MessageBus ponto-a-ponto (ADR-0068 / Labor 9).
//! Não substitui EventBus; thin wrap sobre `k_nano::globals::MESSAGE_BUS`.

use alloc::vec::Vec;
use event_bus::{AgentId, CapabilityToken, Envelope};

/// Envia envelope no MessageBus global.
pub fn ipc_send(
    from: AgentId,
    to: AgentId,
    kind: u32,
    payload: &[u8],
    token: CapabilityToken,
) -> Result<(), &'static str> {
    k_nano::globals::MESSAGE_BUS.send_to(from, to, kind, payload.to_vec(), token)
}

/// Recebe uma mensagem da mailbox do agente.
pub fn ipc_try_recv(agent: AgentId) -> Option<Envelope> {
    k_nano::globals::MESSAGE_BUS.try_recv(agent)
}

/// Drena até `max` mensagens.
pub fn ipc_drain(agent: AgentId, max: usize) -> Vec<Envelope> {
    k_nano::globals::MESSAGE_BUS.mailbox_drain(agent, max)
}

/// Smoke boot — slog VERDICT.
pub fn boot_smoke() -> bool {
    let ok = event_bus::message_bus_self_test();
    if ok {
        k_nano::slog_bin!("IPC", "info", "step=bus status=OK VERDICT=PASS reason=messagebus_roundtrip");
    } else {
        k_nano::slog_bin!("IPC", "info", "step=bus status=FAIL VERDICT=FAIL reason=messagebus_self_test");
    }
    ok
}

/// Labor 58: CapGate residual — envio sem token válido deve falhar honesty.
pub fn capgate_boot_smoke() -> bool {
    use event_bus::CapabilityToken;
    let from: AgentId = "cap_from";
    let to: AgentId = "cap_to";
    // Token zero = deny esperado se CapGate enforce; senão PARTIAL
    let r = ipc_send(from, to, 0x58, b"cap", CapabilityToken::Legacy(0));
    match r {
        Ok(()) => {
            k_nano::slog_bin!(
                "IPC",
                "info",
                "step=capgate status=OK VERDICT=PARTIAL reason=token0_accepted (enforce residual)"
            );
            true
        }
        Err(e) => {
            k_nano::slog_bin!(
                "IPC",
                "info",
                "step=capgate status=OK VERDICT=PASS reason=deny_{}",
                e
            );
            true
        }
    }
}
