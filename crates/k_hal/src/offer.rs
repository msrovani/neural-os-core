//! HalOffer — API de alto nível R3→R1 (ADR-0041).
//! “Tem dispositivo X? Conecte o agente neste port/tópico.”
//! Serve **qualquer** DeviceClass (não só câmera). VirtIO = só transporte BE.

use crate::device_cap::DeviceClass;
use crate::discovery;
use crate::{audio_port, compute_port, display_port, net_port, video_port};
use alloc::string::String;
use alloc::vec::Vec;
use spin::Mutex;

/// Resultado de oferta (Hermes / FE).
pub const TOPIC_HW_OFFER: &str = "HW_OFFER";
/// Bind concluído (qualquer classe) — payload: status=Bound;class=…;agent=…;topic=…
pub const TOPIC_HW_BOUND: &str = "HW_BOUND";

pub const TOPIC_CAMERA_FRAME: &str = "CAMERA_FRAME";
pub const TOPIC_CAMERA_BOUND: &str = "CAMERA_BOUND";
pub const TOPIC_DISPLAY_FRAME: &str = "DISPLAY_FRAME";
pub const TOPIC_COMPUTE_JOB: &str = "COMPUTE_JOB";
pub const TOPIC_AUDIO_PCM: &str = "AUDIO_PCM";
pub const TOPIC_NET_RX: &str = "NET_RX";
pub const TOPIC_BLOCK_IO: &str = "BLOCK_IO";
pub const TOPIC_INPUT_EVENT: &str = "INPUT_EVENT";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfferStatus {
    Absent,
    Available,
    Bound,
    Quarantined,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfferError {
    Absent,
    Quarantined,
    AlreadyBound,
    Full,
    InvalidClass,
    /// CapGate DENY (R3 sem Cap / FE sem bind).
    CapDenied,
    /// ADR-0056: recipe rebelde sem FW / hash
    NeedsFw,
    /// ADR-0056: recipe unsigned ou ausente (Escalate)
    RecipeEscalate,
}

#[derive(Debug, Clone, Copy)]
pub struct BindHandle {
    pub class: DeviceClass,
    pub slot: u8,
    pub topic: &'static str,
}

#[derive(Debug, Clone)]
pub struct OfferEntry {
    pub class: DeviceClass,
    pub status: OfferStatus,
    pub name: String,
    pub bound_agent: Option<String>,
    pub topic: &'static str,
}

struct OfferSlot {
    class: DeviceClass,
    status: OfferStatus,
    name: String,
    bound_agent: Option<String>,
    topic: &'static str,
}

const MAX_BINDS: usize = 32;

static OFFERS: Mutex<Vec<OfferSlot>> = Mutex::new(Vec::new());
static NEXT_SLOT: Mutex<u8> = Mutex::new(1);

// ─── Absent backoff (ADR-0041 — silenciamento de polling) ──────────────
// LinkWatcher/virtio_gpu/hal_offer consultam `query()` a cada tick; sem
// dispositivo na classe, cada consulta lockava OFFERS e logava "status=Absent"
// (poluição do log/EventBus + CPU desperdiçada). Backoff exponencial POR
// CLASSE: 50 → 100 → … → 3200 ticks entre consultas EFETIVAS; no meio,
// `query()` retorna Absent silencioso. Reset imediato quando a classe volta
// a Available/Bound (bind/release).
const ABSENT_BACKOFF_BASE: u64 = 50;
const ABSENT_BACKOFF_CAP: u64 = 3200;

#[derive(Clone, Copy)]
struct AbsentBackoff {
    /// 0 = nunca esteve Absent (próxima consulta efetiva imediata).
    delay: u64,
    /// Tick em que a próxima consulta EFETIVA pode rodar.
    next_effective: u64,
}

const ABSENT_BACKOFF_DEFAULT: AbsentBackoff = AbsentBackoff {
    delay: 0,
    next_effective: 0,
};

/// Um slot por DeviceClass (discriminants 0..=10, ver device_cap.rs).
static ABSENT_BACKOFF: Mutex<[AbsentBackoff; 11]> =
    Mutex::new([ABSENT_BACKOFF_DEFAULT; 11]);

fn now_ticks() -> u64 {
    k_nano::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64
}

/// Reseta o backoff da classe (bind/release — dispositivo presente de novo).
fn absent_backoff_reset(class: DeviceClass) {
    ABSENT_BACKOFF.lock()[class as usize] = ABSENT_BACKOFF_DEFAULT;
}

pub fn topic_for(class: DeviceClass) -> &'static str {
    match class {
        DeviceClass::Video => TOPIC_CAMERA_FRAME,
        DeviceClass::Display => TOPIC_DISPLAY_FRAME,
        DeviceClass::Gpu => TOPIC_COMPUTE_JOB,
        DeviceClass::Snd => TOPIC_AUDIO_PCM,
        DeviceClass::Net | DeviceClass::Wifi => TOPIC_NET_RX,
        DeviceClass::Block => TOPIC_BLOCK_IO,
        DeviceClass::Input => TOPIC_INPUT_EVENT,
        DeviceClass::UsbHost | DeviceClass::Bluetooth | DeviceClass::Unknown => TOPIC_HW_OFFER,
    }
}

/// Agente FE padrão por classe (Hermes / fleet).
pub fn default_agent(class: DeviceClass) -> &'static str {
    match class {
        DeviceClass::Video => "uvc_driver",
        DeviceClass::Display => "display",
        DeviceClass::Gpu => "gpu_driver",
        DeviceClass::Snd => "hda_audio",
        DeviceClass::Net => "net",
        DeviceClass::Wifi => "wifi",
        DeviceClass::Block => "disk",
        DeviceClass::Input => "input",
        DeviceClass::UsbHost => "usb_driver",
        DeviceClass::Bluetooth => "observe",
        DeviceClass::Unknown => "observe",
    }
}

fn port_sync_bound(class: DeviceClass, bound: bool) {
    match class {
        DeviceClass::Video => {
            video_port::set_status(if bound {
                video_port::VideoPortStatus::Bound
            } else {
                video_port::VideoPortStatus::NotBound
            });
        }
        DeviceClass::Display => {
            display_port::set_status(if bound {
                display_port::DisplayPortStatus::Bound
            } else {
                display_port::DisplayPortStatus::NotBound
            });
        }
        DeviceClass::Gpu => {
            compute_port::set_status(compute_port::ComputeStatus {
                status: if bound {
                    compute_port::PortStatus::Bound
                } else {
                    compute_port::PortStatus::NotBound
                },
                device: None,
            });
        }
        DeviceClass::Snd => {
            audio_port::set_status(if bound {
                audio_port::AudioPortStatus::Bound
            } else {
                audio_port::AudioPortStatus::NotBound
            });
        }
        DeviceClass::Net | DeviceClass::Wifi => {
            net_port::set_status(if bound {
                net_port::NetPortStatus::Bound
            } else {
                net_port::NetPortStatus::NotBound
            });
        }
        _ => {}
    }
}

/// Reconstrói ofertas a partir do DeviceTree (chamar após populate PCI).
pub fn refresh_from_tree() {
    let tree = discovery::device_tree();
    let mut offers = OFFERS.lock();
    let prev_binds: Vec<(DeviceClass, Option<String>, OfferStatus)> = offers
        .iter()
        .filter(|o| o.status == OfferStatus::Bound)
        .map(|o| (o.class, o.bound_agent.clone(), o.status))
        .collect();
    offers.clear();

    let mut seen_video = false;
    for cap in &tree {
        let status = if cap.bound {
            OfferStatus::Bound
        } else {
            OfferStatus::Available
        };
        let mut agent = None;
        let mut st = status;
        for (c, a, ps) in &prev_binds {
            if *c == cap.id.class {
                agent = a.clone();
                st = *ps;
                break;
            }
        }
        if cap.id.class == DeviceClass::Video {
            seen_video = true;
        }
        offers.push(OfferSlot {
            class: cap.id.class,
            status: st,
            name: String::from(cap.name),
            bound_agent: agent,
            topic: topic_for(cap.id.class),
        });
    }

    // UVC continua ofertável quando há UsbHost (xHCI), sem confundir classes.
    if !seen_video {
        let has_xhci = tree.iter().any(|c| {
            c.id.class == DeviceClass::UsbHost
                || c.name.contains("xHCI")
                || c.name.contains("USB")
        });
        if has_xhci {
            offers.push(OfferSlot {
                class: DeviceClass::Video,
                status: OfferStatus::Available,
                name: String::from("UVC host (via xHCI)"),
                bound_agent: None,
                topic: TOPIC_CAMERA_FRAME,
            });
        }
    }

    let n = offers.len();
    k_nano::slog_hal!("HalOffer", "refresh", "offers={}", n);
    for class in [
        DeviceClass::Gpu,
        DeviceClass::Net,
        DeviceClass::Wifi,
        DeviceClass::Block,
        DeviceClass::Snd,
        DeviceClass::Video,
        DeviceClass::Display,
        DeviceClass::Input,
        DeviceClass::UsbHost,
        DeviceClass::Bluetooth,
    ] {
        let st = offers
            .iter()
            .filter(|o| o.class == class)
            .map(|o| o.status)
            .max_by_key(|s| match s {
                OfferStatus::Bound => 3u8,
                OfferStatus::Available => 2,
                OfferStatus::Quarantined => 1,
                OfferStatus::Absent => 0,
            })
            .unwrap_or(OfferStatus::Absent);
        if st != OfferStatus::Absent {
            k_nano::slog_hal!(
                "HalOffer",
                "query",
                "class={} status={:?}",
                class.as_str(),
                st
            );
        }
    }
}

pub fn query(class: DeviceClass) -> OfferStatus {
    let now = now_ticks();
    let idx = class as usize;
    // Silenciamento: classe em backoff (última consulta efetiva foi Absent e o
    // tempo ainda não expirou) → retorna Absent SEM lock nem log.
    let silenced = {
        let b = ABSENT_BACKOFF.lock();
        let t = &b[idx];
        t.delay != 0 && now < t.next_effective
    };
    if silenced {
        return OfferStatus::Absent;
    }
    let offers = OFFERS.lock();
    let st = offers
        .iter()
        .filter(|o| o.class == class)
        .map(|o| o.status)
        .max_by_key(|s| match s {
            OfferStatus::Bound => 3u8,
            OfferStatus::Available => 2,
            OfferStatus::Quarantined => 1,
            OfferStatus::Absent => 0,
        })
        .unwrap_or(OfferStatus::Absent);
    drop(offers);
    let mut b = ABSENT_BACKOFF.lock();
    let t = &mut b[idx];
    if st == OfferStatus::Absent {
        // Primeira vez: inicia o backoff na base; depois dobra até o cap.
        if t.delay == 0 {
            t.delay = ABSENT_BACKOFF_BASE;
        }
        t.next_effective = now + t.delay;
        t.delay = t.delay.saturating_mul(2).min(ABSENT_BACKOFF_CAP);
        k_nano::slog_hal!(
            "HalOffer",
            "query",
            "class={} status=Absent (backoff efetivo, próxima em {} ticks)",
            class.as_str(),
            t.delay
        );
    } else {
        // Dispositivo presente — reset do backoff + log normal.
        *t = ABSENT_BACKOFF_DEFAULT;
        k_nano::slog_hal!(
            "HalOffer",
            "query",
            "class={} status={:?}",
            class.as_str(),
            st
        );
    }
    st
}

pub fn bind(class: DeviceClass, agent_name: &str) -> Result<BindHandle, OfferError> {
    if class == DeviceClass::Unknown {
        return Err(OfferError::InvalidClass);
    }
    // ADR-0056 H1: rebelde só com recipe trusted + FW (quando exige)
    match crate::device_recipe::gate_bind_class(class) {
        Ok(pkg) => {
            k_nano::slog_hal!(
                "HalOffer",
                "recipe",
                "gate=ALLOW class={} pkg={}",
                class.as_str(),
                pkg
            );
        }
        Err(crate::device_recipe::RecipePromote::NeedsFw) => {
            k_nano::slog_hal!(
                "HalOffer",
                "bind",
                "DENY class={} agent={} — NeedsFw",
                class.as_str(),
                agent_name
            );
            return Err(OfferError::NeedsFw);
        }
        Err(crate::device_recipe::RecipePromote::Escalate)
        | Err(crate::device_recipe::RecipePromote::None) => {
            k_nano::slog_hal!(
                "HalOffer",
                "bind",
                "DENY class={} agent={} — RecipeEscalate",
                class.as_str(),
                agent_name
            );
            return Err(OfferError::RecipeEscalate);
        }
        Err(crate::device_recipe::RecipePromote::Ok) => {}
    }

    let mut offers = OFFERS.lock();
    let Some(slot) = offers.iter_mut().find(|o| {
        o.class == class && matches!(o.status, OfferStatus::Available | OfferStatus::Bound)
    }) else {
        k_nano::slog_hal!(
            "HalOffer",
            "bind",
            "DENY class={} agent={} — Absent",
            class.as_str(),
            agent_name
        );
        return Err(OfferError::Absent);
    };
    if slot.status == OfferStatus::Quarantined {
        return Err(OfferError::Quarantined);
    }
    if slot.status == OfferStatus::Bound && slot.bound_agent.as_deref() == Some(agent_name) {
        // Re-grant Cap (idempotent) — FE Allow após bind
        if let Some(cap) = crate::cap_gate::fe_for_class(class) {
            crate::cap_gate::grant_fe(cap);
        }
        let handle = BindHandle {
            class,
            slot: 0,
            topic: slot.topic,
        };
        k_nano::slog_hal!(
            "HalOffer",
            "bind",
            "idempotent class={} agent={} topic={}",
            class.as_str(),
            agent_name,
            slot.topic
        );
        return Ok(handle);
    }

    let mut next = NEXT_SLOT.lock();
    if (*next as usize) >= MAX_BINDS {
        return Err(OfferError::Full);
    }
    let sid = *next;
    *next = next.wrapping_add(1);

    slot.status = OfferStatus::Bound;
    slot.bound_agent = Some(String::from(agent_name));
    let topic = slot.topic;
    drop(next);
    drop(offers);
    // Dispositivo voltou a existir — backoff de Absent zera imediatamente.
    absent_backoff_reset(class);

    // H5+: HalOffer bind granta Cap lógica ao agent
    if let Some(cap) = crate::cap_gate::fe_for_class(class) {
        crate::cap_gate::grant_fe(cap);
    }
    port_sync_bound(class, true);
    k_nano::slog_hal!(
        "HalOffer",
        "bind",
        "OK class={} agent={} slot={} topic={}",
        class.as_str(),
        agent_name,
        sid,
        topic
    );
    Ok(BindHandle {
        class,
        slot: sid,
        topic,
    })
}

pub fn release(handle: BindHandle) {
    let mut offers = OFFERS.lock();
    if let Some(slot) = offers.iter_mut().find(|o| o.class == handle.class) {
        slot.status = OfferStatus::Available;
        slot.bound_agent = None;
        k_nano::slog_hal!(
            "HalOffer",
            "release",
            "class={} slot={}",
            handle.class.as_str(),
            handle.slot
        );
    }
    drop(offers);
    // Available de novo — próxima query efetiva imediata.
    absent_backoff_reset(handle.class);
    if let Some(cap) = crate::cap_gate::fe_for_class(handle.class) {
        crate::cap_gate::revoke_fe(cap);
    }
    port_sync_bound(handle.class, false);
}

pub fn list() -> Vec<OfferEntry> {
    OFFERS
        .lock()
        .iter()
        .map(|o| OfferEntry {
            class: o.class,
            status: o.status,
            name: o.name.clone(),
            bound_agent: o.bound_agent.clone(),
            topic: o.topic,
        })
        .collect()
}

/// Pedido genérico: query + bind para qualquer classe.
pub fn request(class: DeviceClass, agent_name: &str) -> Result<BindHandle, OfferError> {
    match query(class) {
        OfferStatus::Absent => Err(OfferError::Absent),
        OfferStatus::Quarantined => Err(OfferError::Quarantined),
        OfferStatus::Available | OfferStatus::Bound => bind(class, agent_name),
    }
}

pub fn request_video(agent_name: &str) -> Result<BindHandle, OfferError> {
    request(DeviceClass::Video, agent_name)
}

/// Parse nome de classe (`video`, `gpu`, `wifi`, …).
#[cfg(test)]
mod tests {
    use super::*;

    fn reset_state() {
        *OFFERS.lock() = Vec::new();
        let mut b = ABSENT_BACKOFF.lock();
        for t in b.iter_mut() {
            *t = ABSENT_BACKOFF_DEFAULT;
        }
    }

    /// 1ª query efetiva (Absent) arma backoff; a 2ª no mesmo tick é SILENCIADA
    /// (delay NÃO dobra de novo) — prova o silenciamento sem depender de log.
    #[test]
    fn absent_query_silences_within_window() {
        reset_state();
        // OFFERS vazio → Gpu sempre Absent. 1ª consulta: efetiva, delay 0→50,
        // armazenado 100 (dobrado p/ a próxima), next = tick0 + 50.
        assert_eq!(query(DeviceClass::Gpu), OfferStatus::Absent);
        {
            let b = ABSENT_BACKOFF.lock();
            let t = &b[DeviceClass::Gpu as usize];
            assert_eq!(t.delay, 100);
            assert_eq!(t.next_effective, 50);
        }
        // 2ª consulta no mesmo tick (0 < 50): silenciada — estado intacto.
        assert_eq!(query(DeviceClass::Gpu), OfferStatus::Absent);
        {
            let b = ABSENT_BACKOFF.lock();
            let t = &b[DeviceClass::Gpu as usize];
            assert_eq!(t.delay, 100, "2ª query silenciada não deve dobrar o backoff");
            assert_eq!(t.next_effective, 50);
        }
    }

    /// Reset (bind/release) zera o backoff → próxima query volta a ser efetiva.
    #[test]
    fn backoff_reset_restores_immediate_query() {
        reset_state();
        let _ = query(DeviceClass::Gpu);
        {
            let b = ABSENT_BACKOFF.lock();
            assert!(b[DeviceClass::Gpu as usize].delay > 0);
        }
        absent_backoff_reset(DeviceClass::Gpu);
        {
            let b = ABSENT_BACKOFF.lock();
            assert_eq!(b[DeviceClass::Gpu as usize].delay, 0);
        }
        // Pós-reset: query é efetiva de novo → backoff rearmado (delay 100).
        assert_eq!(query(DeviceClass::Gpu), OfferStatus::Absent);
        {
            let b = ABSENT_BACKOFF.lock();
            assert_eq!(b[DeviceClass::Gpu as usize].delay, 100);
        }
    }
}

pub fn class_from_str(s: &str) -> Option<DeviceClass> {
    match s {
        "video" | "camera" | "uvc" | "webcam" => Some(DeviceClass::Video),
        "gpu" | "compute" | "cuda" => Some(DeviceClass::Gpu),
        "display" | "fb" | "screen" => Some(DeviceClass::Display),
        "snd" | "audio" | "hda" | "sound" => Some(DeviceClass::Snd),
        "net" | "ethernet" | "nic" => Some(DeviceClass::Net),
        "wifi" | "wlan" | "wireless" => Some(DeviceClass::Wifi),
        "block" | "disk" | "storage" | "nvme" | "ata" => Some(DeviceClass::Block),
        "input" | "hid" | "keyboard" | "mouse" => Some(DeviceClass::Input),
        "usbhost" | "usb" | "xhci" => Some(DeviceClass::UsbHost),
        "bluetooth" | "bt" => Some(DeviceClass::Bluetooth),
        _ => None,
    }
}
