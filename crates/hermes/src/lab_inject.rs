//! Lab inject via QEMU-loader (ADR lab clima / mesh).
//! Magic `LINJ` @ phys 0x0210_0000 → publica USER_INTENT uma vez e consome o flag.
//! sendkey/HID continua o path de produto; este e o path de lab deterministico.

use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use event_bus::{CapabilityToken, Event};
use k_nano::EVENT_BUS;

/// Phys baixo (junto do netmode @0x0200_0000) — cabe em guest >=32MB.
pub const LAB_INJECT_PHYS: u64 = 0x0210_0000;
const MAGIC: &[u8; 4] = b"LINJ";
const MAX_TEXT: usize = 240;

static CONSUMED: AtomicBool = AtomicBool::new(false);

/// Se houver blob LINJ+texto no loader, publica USER_INTENT e zera o magic.
/// Retorna Some(texto) quando injectou neste tick.
pub fn poll_and_publish() -> Option<String> {
    if CONSUMED.load(Ordering::Relaxed) {
        return None;
    }
    // Esperar LLM — senao intent cai em "AI indisponivel" antes do Falcon3.
    if !cortex::cortex::model_is_loaded() {
        return None;
    }
    let pmoff = k_nano::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    if pmoff == 0 {
        return None;
    }
    unsafe {
        k_nano::apic::map_page_uc(LAB_INJECT_PHYS, pmoff);
        let p = (LAB_INJECT_PHYS + pmoff) as *mut u8;
        let mut hdr = [0u8; 4];
        for i in 0..4 {
            hdr[i] = core::ptr::read_volatile(p.add(i));
        }
        if &hdr != MAGIC {
            return None;
        }
        let mut raw = Vec::with_capacity(MAX_TEXT);
        for i in 4..(4 + MAX_TEXT) {
            let b = core::ptr::read_volatile(p.add(i));
            if b == 0 {
                break;
            }
            // so ASCII imprimivel
            if (0x20..=0x7E).contains(&b) {
                raw.push(b);
            }
        }
        // consumir: zera magic (evita re-inject)
        for i in 0..4 {
            core::ptr::write_volatile(p.add(i), 0);
        }
        CONSUMED.store(true, Ordering::Release);
        if raw.is_empty() {
            k_nano::slog_hermes!("LAB", "warn", "LINJ vazio @ {:#x}", LAB_INJECT_PHYS);
            return None;
        }
        let text = String::from_utf8_lossy(&raw).into_owned();
        k_nano::slog_hermes!(
            "LAB",
            "ok",
            "inject USER_INTENT (LINJ): \"{}\"",
            text
        );
        let _ = EVENT_BUS.publish(Event {
            id: 0,
            topic: String::from(crate::hermes::TOPIC_USER_INTENT),
            payload: text.as_bytes().to_vec(),
            token: CapabilityToken::Legacy(1),
        });
        Some(text)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn magic_is_linj() {
        assert_eq!(super::MAGIC, b"LINJ");
        assert_eq!(super::LAB_INJECT_PHYS, 0x0210_0000);
    }
}
