//! Consumidor do tópico `HINTS` (ADR-0047-HMI §6.4 — H3-revisit, s420c).
//!
//! O `DisplayAgent` drena o EventBus (produtor: `render_hints()` 2 Hz, pesos
//! do MLP residentes na VRAM via BAR — hint_render.rs) e alimenta ESTE
//! snapshot; orb/dock/cards leem no paint. Hints são AUMENTATIVOS: energia 0
//! (ou stale) = cor clássica intacta — o tint só soma, nunca substitui tema.
//!
//! Contrato do payload (24 B): 8 regiões × (reservado, energia 0..255,
//! matiz 0..255). Regiões: 0=orb 1=header 2=dock 3..6=cards 7=HUD.
//!
//! Higiene (hot path): zero alloc, zero divisão por pixel — `sample_region`
//! é uma leitura de static + comparação de idade (TSC).

use core::sync::atomic::{AtomicU64, Ordering};
use spin::Mutex;

/// Idade máxima do snapshot: além disso a energia decai a 0 (modo clássico).
/// Produtor corre a 2 Hz (500 ms) — 2.5 s = 5 feeds perdidos.
pub const STALE_US: u64 = 2_500_000;

/// (energia, matiz) por região — energia 0 = sem tint.
static REGIONS: Mutex<[(u8, u8); 8]> = Mutex::new([(0u8, 0u8); 8]);
static LAST_US: AtomicU64 = AtomicU64::new(0);
/// Override de relógio p/ testes host (sem TSC): 0 = usa `k_nano::tsc`.
static CLOCK_OVERRIDE_US: AtomicU64 = AtomicU64::new(0);

fn now_us() -> u64 {
    let o = CLOCK_OVERRIDE_US.load(Ordering::Relaxed);
    if o != 0 {
        o
    } else {
        k_nano::tsc::now_us()
    }
}

/// DisplayAgent: aceita o payload cru do tópico HINTS (24 B). Payload
/// inválido é ignorado honestamente (não zera o snapshot vigente).
pub fn accept_payload(payload: &[u8]) {
    if payload.len() != 24 {
        return;
    }
    let mut next = [(0u8, 0u8); 8];
    for (r, chunk) in payload.chunks_exact(3).enumerate() {
        // chunk[0] = reservado; chunk[1] = energia; chunk[2] = matiz.
        next[r] = (chunk[1], chunk[2]);
    }
    *REGIONS.lock() = next;
    LAST_US.store(now_us(), Ordering::Release);
}

/// Testes: injeta relógio.
#[cfg(test)]
pub fn set_clock_override(us: u64) {
    CLOCK_OVERRIDE_US.store(us, Ordering::Relaxed);
}

/// Testes: zera statics (compartilhados — lição SESSION_346).
#[cfg(test)]
pub fn reset_for_test() {
    *REGIONS.lock() = [(0u8, 0u8); 8];
    LAST_US.store(0, Ordering::Relaxed);
    CLOCK_OVERRIDE_US.store(0, Ordering::Relaxed);
}

/// Leitura no paint: energia da região com decaimento por idade.
/// 0 = sem tint (clássico). `hue_out` recebe o matiz válido.
pub fn sample_region(region: usize, hue_out: &mut u8) -> u8 {
    let g = REGIONS.lock();
    let (e, h) = g[region % 8];
    if e == 0 {
        return 0;
    }
    let last = LAST_US.load(Ordering::Acquire);
    let age = now_us().saturating_sub(last);
    if last == 0 || age >= STALE_US {
        return 0;
    }
    // Decaimento linear até 0 na borda do stale (fade suave, sem pulso).
    let faded = ((e as u64 * (STALE_US - age)) / STALE_US) as u8;
    if faded > 0 {
        *hue_out = h;
    }
    faded
}

/// Energia + matiz de uma região em uma chamada (0 = clássico).
pub fn sample(region: usize) -> (u8, u8) {
    let mut h = 0u8;
    let e = sample_region(region, &mut h);
    (e, h)
}

/// Matiz 0..255 → RGB "glass" (satura em ciano; compatível com a paleta).
/// [hue+128] pega a família ciano/azul/menta; com energia baixa o caller
/// lerpa para a cor base, então o hue só domina com hints vivos.
pub fn hue_to_rgb(hue: u8) -> (u8, u8, u8) {
    // 6 setores de 42.7 — roda de cor inteira (sem f32, soft-float).
    let sector = (hue as u16 * 6 / 256) as u8; // 0..6
    let frac = ((hue as u16 * 6 % 256) as u32) as u8; // 0..255 dentro do setor
    let (r, g, b) = match sector & 7 {
        0 => (255u8, frac, 0),     // vermelho → amarelo
        1 => (255 - frac, 255, 0), // amarelo → verde
        2 => (0, 255, frac),       // verde → ciano
        3 => (0, 255 - frac, 255), // ciano → azul
        4 => (frac, 0, 255),       // azul → magenta
        _ => (255, 0, 255 - frac), // magenta → vermelho
    };
    // Dessatura 40% + clareia p/ fundo escuro (glass): mix com cinza-azulado.
    let mix = |c: u8, base: u16| -> u8 { ((c as u16 * 153 + base * 103) / 256) as u8 };
    (mix(r, 120), mix(g, 180), mix(b, 220))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Static global — testes serializam (lição SESSION_346).
    static TEST_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn payload_invalido_nao_substitui_snapshot() {
        let _g = TEST_LOCK.lock();
        reset_for_test();
        set_clock_override(1_000_000);
        let mut p = [0u8; 24];
        p[1] = 200; // região 0 energia
        p[2] = 42; // região 0 matiz
        accept_payload(&p);
        assert_eq!(sample(0), (200, 42));
        // Payload torto: snapshot vigente permanece.
        accept_payload(&p[..10]);
        accept_payload(&[0u8; 30]);
        assert_eq!(sample(0), (200, 42));
    }

    #[test]
    fn freshness_decai_e_expira() {
        let _g = TEST_LOCK.lock();
        reset_for_test();
        let mut p = [0u8; 24];
        p[4] = 255; // região 1 energia cheia
        p[5] = 128;
        set_clock_override(10_000_000);
        accept_payload(&p);
        // Metade da janela: ~metade da energia.
        set_clock_override(10_000_000 + STALE_US / 2);
        let (e, h) = sample(1);
        assert_eq!(h, 128);
        assert!((110..=130).contains(&e), "energia na metade: {e}");
        // Expirou: 0 (clássico), mesmo com snapshot presente.
        set_clock_override(10_000_000 + STALE_US + 1);
        assert_eq!(sample(1), (0, 0));
    }

    #[test]
    fn sem_feed_energia_zero() {
        let _g = TEST_LOCK.lock();
        reset_for_test();
        set_clock_override(2_000_000);
        // Sem accept: nunca tint.
        for r in 0..8 {
            assert_eq!(sample(r), (0, 0));
        }
    }

    #[test]
    fn hue_to_rgb_deterministico_e_glass() {
        // Mesma entrada → mesma saída; saída é "glass" (canal B alto).
        for hue in [0u8, 64, 128, 200, 255] {
            let (r, _g_, b) = hue_to_rgb(hue);
            assert!(b >= 85, "hue {hue}: B baixo ({b}) — fora da paleta glass");
            let _ = r;
        }
        assert_eq!(hue_to_rgb(128), hue_to_rgb(128));
    }
}
