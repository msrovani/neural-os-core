//! HalOffer bridge — Hermes pede **qualquer** HW ao k-hal sem MMIO (ADR-0041).
//! Intent / PnP / agente → query/bind → EventBus HW_OFFER + HW_BOUND (+ CAMERA_BOUND).

use alloc::format;
use alloc::string::String;
use event_bus::{CapabilityToken, Event};
use k_hal::device_cap::DeviceClass;
use k_hal::offer::{self, BindHandle, OfferError, OfferStatus};

/// Resultado legível para agentes / slog.
pub struct DeviceRequestResult {
    pub ok: bool,
    pub class: DeviceClass,
    pub status: OfferStatus,
    pub topic: Option<&'static str>,
    pub ack: String,
}

fn publish(topic: &str, payload: String) {
    let _ = k_nano::EVENT_BUS.publish(Event {
        id: 0,
        topic: String::from(topic),
        payload: payload.into_bytes(),
        token: CapabilityToken::Legacy(1),
    });
}

/// Pedido genérico: query + bind se Available (qualquer DeviceClass).
pub fn request_device(class: DeviceClass, agent_name: &str) -> DeviceRequestResult {
    let st = offer::query(class);
    match st {
        OfferStatus::Absent => {
            let ack = format!(
                "HalOffer Absent class={} agent={}",
                class.as_str(),
                agent_name
            );
            k_nano::slog_hermes!("HalOffer", "request", "{}", ack);
            publish(
                offer::TOPIC_HW_OFFER,
                format!("status=Absent;class={};agent={}", class.as_str(), agent_name),
            );
            DeviceRequestResult {
                ok: false,
                class,
                status: st,
                topic: None,
                ack,
            }
        }
        OfferStatus::Quarantined => {
            let ack = format!("HalOffer Quarantined class={}", class.as_str());
            k_nano::slog_hermes!("HalOffer", "request", "{}", ack);
            publish(
                offer::TOPIC_HW_OFFER,
                format!("status=Quarantined;class={}", class.as_str()),
            );
            DeviceRequestResult {
                ok: false,
                class,
                status: st,
                topic: None,
                ack,
            }
        }
        OfferStatus::Available | OfferStatus::Bound => match offer::bind(class, agent_name) {
            Ok(h) => {
                let ack = format!(
                    "HalOffer Bound class={} agent={} topic={}",
                    class.as_str(),
                    agent_name,
                    h.topic
                );
                k_nano::slog_hermes!("HalOffer", "request", "{}", ack);
                let wire = format!(
                    "status=Bound;class={};agent={};topic={}",
                    class.as_str(),
                    agent_name,
                    h.topic
                );
                publish(offer::TOPIC_HW_OFFER, wire.clone());
                publish(offer::TOPIC_HW_BOUND, wire);
                if class == DeviceClass::Video {
                    publish(
                        offer::TOPIC_CAMERA_BOUND,
                        format!("agent={};topic={}", agent_name, h.topic),
                    );
                }
                DeviceRequestResult {
                    ok: true,
                    class,
                    status: OfferStatus::Bound,
                    topic: Some(h.topic),
                    ack,
                }
            }
            Err(e) => {
                let status = match e {
                    OfferError::CapDenied => OfferStatus::Quarantined,
                    _ => st,
                };
                let ack = format!(
                    "HalOffer bind fail class={} err={:?}",
                    class.as_str(),
                    e
                );
                k_nano::slog_hermes!("HalOffer", "request", "{}", ack);
                if matches!(e, OfferError::CapDenied) {
                    publish(
                        offer::TOPIC_HW_OFFER,
                        format!("status=Quarantined;class={};reason=CapDenied", class.as_str()),
                    );
                }
                DeviceRequestResult {
                    ok: false,
                    class,
                    status,
                    topic: None,
                    ack,
                }
            }
        },
    }
}

/// Usa agente FE padrão da classe.
pub fn request_class(class: DeviceClass) -> DeviceRequestResult {
    request_device(class, offer::default_agent(class))
}

pub fn request_video(agent_name: &str) -> DeviceRequestResult {
    request_device(DeviceClass::Video, agent_name)
}

/// Bind idempotente para qualquer classe.
pub fn ensure_bound(class: DeviceClass, agent_name: &str) -> Result<BindHandle, OfferError> {
    let r = request_device(class, agent_name);
    if r.ok {
        offer::bind(class, agent_name)
    } else {
        match r.status {
            OfferStatus::Quarantined => Err(OfferError::Quarantined),
            _ => Err(OfferError::Absent),
        }
    }
}

pub fn ensure_camera_bound(agent_name: &str) -> Result<BindHandle, OfferError> {
    ensure_bound(DeviceClass::Video, agent_name)
}

/// Inferir DeviceClass a partir de texto de intent (pt/en).
pub fn class_from_intent(text: &str) -> Option<DeviceClass> {
    let t = text.to_ascii_lowercase();
    // ordem: mais específico primeiro
    if t.contains("wifi")
        || t.contains("wlan")
        || t.contains("wireless")
        || t.contains("iwlwifi")
        || t.contains("rede sem fio")
    {
        return Some(DeviceClass::Wifi);
    }
    if t.contains("camera")
        || t.contains("câmera")
        || t.contains("webcam")
        || t.contains("uvc")
        || t.contains("visao")
        || t.contains("visão")
        || t.contains("video")
    {
        return Some(DeviceClass::Video);
    }
    if t.contains("gpu")
        || t.contains("cuda")
        || t.contains("vulkan")
        || t.contains("compute")
        || t.contains("nvidia")
        || t.contains("radeon")
        || t.contains("intel gpu")
    {
        return Some(DeviceClass::Gpu);
    }
    if t.contains("display")
        || t.contains("framebuffer")
        || t.contains("tela")
        || t.contains("monitor")
        || t.contains("compositor")
    {
        return Some(DeviceClass::Display);
    }
    if t.contains("audio")
        || t.contains("som")
        || t.contains("hda")
        || t.contains("microfone")
        || t.contains("speaker")
        || t.contains("voz")
        || t.contains("tts")
        || t.contains("stt")
    {
        return Some(DeviceClass::Snd);
    }
    if t.contains("ethernet")
        || t.contains("e1000")
        || t.contains("rtl8139")
        || t.contains("nic")
        || t.contains("rede ")
        || t.contains("network")
    {
        return Some(DeviceClass::Net);
    }
    if t.contains("disco")
        || t.contains("disk")
        || t.contains("nvme")
        || t.contains("ahci")
        || t.contains("ata")
        || t.contains("storage")
        || t.contains("ssd")
        || t.contains("hdd")
    {
        return Some(DeviceClass::Block);
    }
    if t.contains("teclado")
        || t.contains("keyboard")
        || t.contains("mouse")
        || t.contains("hid")
        || t.contains("input")
    {
        return Some(DeviceClass::Input);
    }
    // "usar hardware X" / "preciso de hw"
    if let Some(rest) = t.strip_prefix("hw ") {
        return offer::class_from_str(rest.split_whitespace().next().unwrap_or(""));
    }
    if t.contains("haloffer") || t.contains("hal offer") {
        for part in t.split_whitespace() {
            if let Some(c) = offer::class_from_str(part) {
                return Some(c);
            }
        }
    }
    None
}

/// Intent → HalOffer (qualquer HW). Retorna None se o texto não pedir dispositivo.
pub fn request_from_intent(text: &str) -> Option<DeviceRequestResult> {
    let class = class_from_intent(text)?;
    let agent = offer::default_agent(class);
    Some(request_device(class, agent))
}

/// Mapear ação PnP → classe HalOffer.
pub fn class_from_pnp_next(next: &str) -> Option<DeviceClass> {
    match next {
        "bind_network" => Some(DeviceClass::Net),
        "bind_wifi_scan" => Some(DeviceClass::Wifi),
        "bind_gpu_compute" => Some(DeviceClass::Gpu),
        "bind_usb_host" => Some(DeviceClass::Video),
        "bind_audio" => Some(DeviceClass::Snd),
        "bind_storage" => Some(DeviceClass::Block),
        "ready" => None, // display já up — sem bind obrigatório
        _ => None,
    }
}

pub fn request_from_pnp_next(next: &str, agent_hint: &str) -> Option<DeviceRequestResult> {
    let class = class_from_pnp_next(next)?;
    let agent = if agent_hint.is_empty() || agent_hint == "?" || agent_hint == "-" {
        offer::default_agent(class)
    } else {
        agent_hint
    };
    Some(request_device(class, agent))
}

pub fn list_offers_text() -> String {
    let mut out = String::from("HalOffer catalog:\n");
    for e in offer::list() {
        out.push_str(&format!(
            "  {} {:?} name={} agent={:?} topic={}\n",
            e.class.as_str(),
            e.status,
            e.name,
            e.bound_agent,
            e.topic
        ));
    }
    out
}

pub fn release_device(handle: BindHandle) {
    offer::release(handle);
}
