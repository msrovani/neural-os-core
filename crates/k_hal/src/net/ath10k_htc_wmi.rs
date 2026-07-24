//! ath10k A4 — HTC mínimo + WMI connect/ready stub (pós-fw_ready).
//! Honesty: sem SoftMAC/scan RF; PARTIAL se timeout ou sem beacons.

use crate::net::ath10k_ce_bmi::CeBmi;

/// HTC_MSG_READY_ID
const HTC_MSG_READY: u16 = 1;
/// HTC_MSG_CONNECT_SERVICE_ID
const HTC_MSG_CONNECT_SVC: u16 = 2;
/// HTC_MSG_CONNECT_SERVICE_RESP_ID
const HTC_MSG_CONNECT_SVC_RESP: u16 = 3;
/// HTC_MSG_SETUP_COMPLETE_EX_ID
const HTC_MSG_SETUP_COMPLETE_EX: u16 = 5;

/// ATH10K_HTC_SVC_ID_WMI_CONTROL = SVC(WMI, 0) = 0x0100
const HTC_SVC_WMI_CONTROL: u16 = 0x0100;

/// Resultado A4 para slog / WifiAgent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum A4Status {
    /// HTC ready + WMI connect medidos; scan ainda AWAITING.
    HtcWmiOkScanAwaiting,
    /// HTC OK; WMI não respondeu.
    HtcOkWmiAwaiting,
    /// HTC ready não chegou (pipe/CE/timeout).
    HtcAwaiting,
}

impl A4Status {
    pub fn as_str(self) -> &'static str {
        match self {
            A4Status::HtcWmiOkScanAwaiting => "htc_wmi_ok_scan_awaiting_note",
            A4Status::HtcOkWmiAwaiting => "htc_ok_wmi_awaiting_note",
            A4Status::HtcAwaiting => "htc_awaiting_note",
        }
    }

    pub fn verdict_reason(self) -> &'static str {
        match self {
            A4Status::HtcWmiOkScanAwaiting => "scan_awaiting_note",
            A4Status::HtcOkWmiAwaiting => "wmi_awaiting_note",
            A4Status::HtcAwaiting => "htc_awaiting_note",
        }
    }
}

/// Poll CE1 por HTC_MSG_READY (FW empurra após boot; sem TX).
fn wait_htc_ready(ce: &mut CeBmi) -> Result<(u16, u16), &'static str> {
    let mut resp = [0u8; 64];
    let n = ce.recv_poll(&mut resp)?;
    if n < 12 {
        return Err("htc_ready_short");
    }
    // htc_hdr (8) + msg_hdr(2) + ready body
    let eid = resp[0];
    let plen = u16::from_le_bytes([resp[2], resp[3]]) as usize;
    if eid != 0 {
        return Err("htc_ready_eid");
    }
    if plen < 6 || n < 8 + 6 {
        return Err("htc_ready_len");
    }
    let msg_id = u16::from_le_bytes([resp[8], resp[9]]);
    if msg_id != HTC_MSG_READY {
        return Err("htc_not_ready_msg");
    }
    let credit_count = u16::from_le_bytes([resp[10], resp[11]]);
    let credit_size = if n >= 14 {
        u16::from_le_bytes([resp[12], resp[13]])
    } else {
        0
    };
    k_nano::slog_hal!(
        "ATH10K",
        "info",
        "step=htc status=OK credits={} credit_sz={}",
        credit_count,
        credit_size
    );
    Ok((credit_count, credit_size))
}

fn htc_ctrl_send_recv(
    ce: &mut CeBmi,
    payload: &[u8],
    resp: &mut [u8],
) -> Result<usize, &'static str> {
    // HTC hdr EP0 + payload
    let mut frame = [0u8; 256];
    if payload.len() + 8 > frame.len() {
        return Err("htc_frame_too_big");
    }
    frame[0] = 0; // eid
    frame[1] = 0; // flags
    let plen = payload.len() as u16;
    frame[2..4].copy_from_slice(&plen.to_le_bytes());
    frame[4] = 0;
    frame[5] = 0;
    frame[6] = 0;
    frame[7] = 0;
    frame[8..8 + payload.len()].copy_from_slice(payload);
    ce.exchange(&frame[..8 + payload.len()], resp)
}

fn connect_wmi_control(ce: &mut CeBmi) -> Result<u8, &'static str> {
    // msg: CONNECT_SERVICE + service_id + flags
    let mut payload = [0u8; 8];
    payload[0..2].copy_from_slice(&HTC_MSG_CONNECT_SVC.to_le_bytes());
    payload[2..4].copy_from_slice(&HTC_SVC_WMI_CONTROL.to_le_bytes());
    // flags: recv_alloc in high byte — modest 8 credits
    let flags: u16 = 8u16 << 8;
    payload[4..6].copy_from_slice(&flags.to_le_bytes());

    let mut resp = [0u8; 64];
    let n = htc_ctrl_send_recv(ce, &payload, &mut resp)?;
    if n < 16 {
        return Err("wmi_conn_short");
    }
    let msg_id = u16::from_le_bytes([resp[8], resp[9]]);
    if msg_id != HTC_MSG_CONNECT_SVC_RESP {
        return Err("wmi_conn_not_resp");
    }
    // conn_svc_resp: service_id(2) status(1) eid(1) max_msg(2) @ offset 10
    let status = resp[12];
    let eid = resp[13];
    if status != 0 {
        return Err("wmi_conn_failed");
    }
    k_nano::slog_hal!(
        "ATH10K",
        "info",
        "step=wmi status=OK eid={} (connect_service)",
        eid
    );
    Ok(eid)
}

fn setup_complete(ce: &mut CeBmi) -> Result<(), &'static str> {
    // SETUP_COMPLETE_EX: msg_id + pad + flags(u32) + max_msgs + pad
    let mut payload = [0u8; 12];
    payload[0..2].copy_from_slice(&HTC_MSG_SETUP_COMPLETE_EX.to_le_bytes());
    // rest zero = no bundle
    let mut empty = [];
    htc_ctrl_send_recv(ce, &payload, &mut empty)?;
    k_nano::slog_hal!("ATH10K", "info", "step=htc status=setup_complete");
    Ok(())
}

/// Poll curto por qualquer RX pós-WMI (service ready / echo). Não parseia TLV pleno.
fn poll_wmi_ready_echo(ce: &mut CeBmi) -> Result<(), &'static str> {
    let mut resp = [0u8; 128];
    match ce.recv_poll(&mut resp) {
        Ok(n) if n >= 8 => {
            k_nano::slog_hal!(
                "ATH10K",
                "info",
                "step=wmi status=OK detail=rx_echo bytes={}",
                n
            );
            Ok(())
        }
        Ok(_) => Err("wmi_ready_short"),
        Err(e) => Err(e),
    }
}

/// A4 bring-up após `fw_ready=1`. Retorna (status, wmi_eid se connect OK).
pub fn a4_htc_wmi_bringup(ce: &mut CeBmi) -> (A4Status, Option<u8>) {
    k_nano::slog_hal!("ATH10K", "info", "step=htc status=START");

    if let Err(e) = wait_htc_ready(ce) {
        k_nano::slog_hal!(
            "ATH10K",
            "info",
            "step=htc status=FAIL reason={}",
            e
        );
        return (A4Status::HtcAwaiting, None);
    }

    // Ordem Linux: connect services → setup_complete → wait WMI service ready
    k_nano::slog_hal!("ATH10K", "info", "step=wmi status=START");
    match connect_wmi_control(ce) {
        Ok(eid) => {
            if let Err(e) = setup_complete(ce) {
                k_nano::slog_hal!(
                    "ATH10K",
                    "info",
                    "step=htc status=PARTIAL reason=setup_{}",
                    e
                );
            }
            let st = match poll_wmi_ready_echo(ce) {
                Ok(()) => A4Status::HtcWmiOkScanAwaiting,
                Err(e) => {
                    k_nano::slog_hal!(
                        "ATH10K",
                        "info",
                        "step=wmi status=PARTIAL reason={}",
                        e
                    );
                    A4Status::HtcWmiOkScanAwaiting
                }
            };
            (st, Some(eid))
        }
        Err(e) => {
            k_nano::slog_hal!(
                "ATH10K",
                "info",
                "step=wmi status=FAIL reason={}",
                e
            );
            (A4Status::HtcOkWmiAwaiting, None)
        }
    }
}
