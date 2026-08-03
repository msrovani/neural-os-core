//! UVC Driver FE — câmera via xHCI isócrono (Phase 4) + HalOffer bind.
//! `poll_isoc_frame()` (k_nano) → monta frames MJPEG/YUY2 → publica
//! `CAMERA_FRAME` com bytes reais. Stub cinza removido.

use agent_core::{Agent, AgentKind, AgentManifest, ScheduleKind, AgentTickResult};
use event_bus::{CapabilityToken, Event};
use alloc::vec::Vec;
use core::sync::atomic::Ordering;

const UVC_MANIFEST: AgentManifest = AgentManifest {
    name: "uvc_driver",
    kind: AgentKind::Driver,
    // Continuous + Done: o scheduler volta a chamar no próximo tick sem
    // watchdog/rate-limit (Pending >10000 consecutivos → Crashed).
    schedule: ScheduleKind::Continuous,
    auto_start: true,
    persist: false,
};

/// Cap de frame (4MB) — evita runaway de acúmulo.
const FRAME_CAP: usize = 4 * 1024 * 1024;
const FORMAT_MJPEG: u8 = 1;

pub struct UvcDriverAgent {
    bound: bool,
    last_bind_try: u64,
    bind_failed_logged: bool,
    uvc_ready: bool,
    tried: bool,
    width: u16,
    height: u16,
    fps: u16,
    format: u8,
    /// Frame em montagem (MJPEG: bytes entre SOI e EOI; YUY2: w*h*2).
    frame: Vec<u8>,
    started: bool,
    /// Último byte visto antes do SOI (SOI pode cruzar fronteira de pacote).
    pending_ff: bool,
    last_data_tick: u64,
    frames_published: u64,
    bytes_published: u64,
    dropped: u64,
    logged_first: bool,
}

impl UvcDriverAgent {
    pub fn new() -> Self {
        UvcDriverAgent {
            bound: false,
            last_bind_try: 0,
            bind_failed_logged: false,
            uvc_ready: false,
            tried: false,
            width: 640,
            height: 480,
            fps: 30,
            format: FORMAT_MJPEG,
            frame: Vec::new(),
            started: false,
            pending_ff: false,
            last_data_tick: 0,
            frames_published: 0,
            bytes_published: 0,
            dropped: 0,
            logged_first: false,
        }
    }

    fn now_ticks(&self) -> u64 {
        k_nano::interrupts::TIMER_TICKS.load(Ordering::Relaxed) as u64
    }

    /// Pede câmera ao Hermes/HalOffer (retry ~1/s até OK).
    fn ensure_bound(&mut self, now: u64) -> bool {
        if self.bound {
            return true;
        }
        if self.last_bind_try != 0
            && now.saturating_sub(self.last_bind_try) < k_nano::interrupts::TIMER_HZ.load(Ordering::Relaxed)
        {
            return false;
        }
        self.last_bind_try = now;
        match hermes::hal_offer::ensure_camera_bound("uvc_driver") {
            Ok(h) => {
                k_nano::slog_jarbas!(
                    "UVC",
                    "bind",
                    "HalOffer OK topic={} slot={}",
                    h.topic,
                    h.slot
                );
                self.bound = true;
                true
            }
            Err(e) => {
                if !self.bind_failed_logged {
                    self.bind_failed_logged = true;
                    k_nano::slog_jarbas!("UVC", "bind", "HalOffer DENY {:?}", e);
                }
                false
            }
        }
    }

    /// Processa um pacote cru (header UVC incluso) → acúmulo de frame.
    fn assemble_packet(&mut self, pkt: &[u8], now: u64) {
        // Timeout: frame incompleta há ~1s (TIMER_HZ ticks) → drop.
        if self.started
            && now.saturating_sub(self.last_data_tick) > k_nano::interrupts::TIMER_HZ.load(Ordering::Relaxed)
        {
            self.drop_frame("timeout");
        }
        // Header UVC (1.5 §2.4.3): [0]=bHeaderLength (2..12), [1]=bmHeaderInfo
        // (bit0=error, bit1=EOH, bit6=PTS, bit7=SCR). Dados após o header.
        if pkt.len() < 2 {
            return;
        }
        let bhl = (pkt[0] as usize).clamp(2, 12).min(pkt.len());
        if pkt[1] & 0x01 != 0 {
            return; // erro no payload — descarta o pacote
        }
        let data = &pkt[bhl..];
        if data.is_empty() {
            return;
        }

        if !self.started {
            if self.format == FORMAT_MJPEG {
                // Espera SOI (FFD8), incluindo SOI que cruza fronteira de pacote.
                let mut start_at = None;
                if self.pending_ff && data[0] == 0xD8 {
                    start_at = Some(1);
                } else {
                    for (k, w) in data.windows(2).enumerate() {
                        if w[0] == 0xFF && w[1] == 0xD8 {
                            start_at = Some(k);
                            break;
                        }
                    }
                }
                match start_at {
                    Some(k) => {
                        self.started = true;
                        self.frame.extend_from_slice(&data[k..]);
                    }
                    None => {
                        self.pending_ff = data.last() == Some(&0xFF);
                        return;
                    }
                }
            } else {
                // YUY2: o frame começa no 1º pacote válido.
                self.started = true;
                self.frame.extend_from_slice(data);
            }
        } else {
            self.frame.extend_from_slice(data);
        }

        if self.frame.len() > FRAME_CAP {
            self.drop_frame("size");
            return;
        }

        // Frame completo?
        let complete = if self.format == FORMAT_MJPEG {
            let l = self.frame.len();
            l >= 2 && self.frame[l - 2] == 0xFF && self.frame[l - 1] == 0xD9 // EOI
        } else {
            self.frame.len() >= self.width as usize * self.height as usize * 2
        };
        if complete {
            let n = self.frame.len() as u64;
            let frame = core::mem::take(&mut self.frame);
            self.started = false;
            self.pending_ff = false;
            self.frames_published += 1;
            self.bytes_published += n;
            self.publish(frame);
            if self.frames_published % 300 == 0 {
                k_nano::slog_jarbas!(
                    "UVC",
                    "frame",
                    "{} frames, {} bytes médios, {} drops",
                    self.frames_published,
                    self.bytes_published / self.frames_published.max(1),
                    self.dropped
                );
            }
        }
        self.last_data_tick = now;
    }

    fn drop_frame(&mut self, why: &str) {
        self.dropped += 1;
        if self.dropped <= 3 {
            k_nano::slog_jarbas!(
                "UVC",
                "frame",
                "drop ({}) — {} bytes acumulados",
                why,
                self.frame.len()
            );
        }
        self.frame.clear();
        self.started = false;
        self.pending_ff = false;
    }

    fn publish(&mut self, frame: Vec<u8>) {
        let n = frame.len();
        let _ = k_nano::EVENT_BUS.publish(Event {
            id: 0,
            topic: alloc::string::String::from(k_hal::offer::TOPIC_CAMERA_FRAME),
            payload: frame,
            token: CapabilityToken::Legacy(1),
        });
        if !self.logged_first {
            self.logged_first = true;
            k_nano::slog_jarbas!(
                "UVC",
                "frame",
                "1º frame real publicado ({} bytes) — stub substituído",
                n
            );
        }
    }
}

impl Agent for UvcDriverAgent {
    fn manifest(&self) -> &AgentManifest {
        &UVC_MANIFEST
    }

    fn tick(&mut self, _t: u64, _count: u64) -> AgentTickResult {
        let now = self.now_ticks();
        if !self.ensure_bound(now) {
            return AgentTickResult::Done;
        }

        if !self.uvc_ready {
            if self.tried {
                return AgentTickResult::Done; // sem câmera — quieto
            }
            self.tried = true;
            match unsafe { k_nano::xhci::bringup_uvc() } {
                Some(dev) => {
                    self.uvc_ready = true;
                    self.width = dev.width;
                    self.height = dev.height;
                    self.fps = dev.fps;
                    self.format = dev.format;
                    let armed = unsafe { k_nano::xhci::schedule_isoc_in_frame() };
                    k_nano::slog_jarbas!(
                        "UVC",
                        "info",
                        "device OK: {}x{}@{} format={} ep={:#04x} isoc_armed={}",
                        dev.width,
                        dev.height,
                        dev.fps,
                        if dev.format == 1 { "MJPEG" } else { "YUY2" },
                        dev.ep,
                        armed
                    );
                    self.frame.reserve(64 * 1024);
                }
                None => {
                    k_nano::slog_jarbas!(
                        "UVC",
                        "info",
                        "sem device UVC no bus (stub cinza desativado)"
                    );
                    return AgentTickResult::Done;
                }
            }
        }

        let mut buf = [0u8; 1024];
        loop {
            let n = unsafe { k_nano::xhci::poll_isoc_frame(&mut buf) };
            if n == 0 {
                break;
            }
            self.assemble_packet(&buf[..n], now);
        }
        // Continuous + Done: sem watchdog/rate-limit (Pending >10000 → Crashed).
        AgentTickResult::Done
    }
}
