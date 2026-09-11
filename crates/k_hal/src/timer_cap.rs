//! ADR-0104 — adaptive tick-rate (R1 sanciona a banda; R0 programa).
//!
//! R0 (`k_nano::apic`) mede o LAPIC e tem o **único** ponto de mutação
//! (`set_tick_hz`). R1 sanção: a cadência vive numa banda derivada dos fatos
//! medidos e só muda por rails, com dwell + confirmação + rollback.
//!
//! Doctrine: auto só pode MANTER ou BAIXAR (default 60 Hz). Subir acima do
//! default exige HITL (`pin_tick_hz`) — Escalate ≠ Auto.

use alloc::string::String;
use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;

use k_nano::apic::{
    tick_hz as r0_tick_hz, TICK_HZ_DEFAULT, TICK_HZ_MAX, TICK_HZ_MIN, INIT_FLOOR,
};

pub const TOPIC_TIMER_CAP: &str = "TIMER_CAP";

/// Rails de decisão (Hz). Auto anda no máximo um degrau por mudança.
const RAILS: [u64; 3] = [30, 60, 120];
/// Janela mínima entre mudanças reais (ms).
const DWELL_MS: u64 = 5_000;
/// Jitter acima disso (ppm) dispara rollback dentro do dwell (~5%).
const JITTER_ROLLBACK_PPM: u32 = 50_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TickSource {
    Lapic,
    Pit,
    Soft,
    Unknown,
}

#[derive(Debug, Clone, Copy)]
pub struct TimerCap {
    pub source: TickSource,
    pub lapic_counts_hz: u64,
    pub tsc_hz: u64,
    pub hz_min: u64,
    pub hz_default: u64,
    pub hz_max: u64,
    pub jitter_ppm: u32,
    pub trusted: bool,
}

/// Cache da sanção (banda + fatos medidos). `cap()` refresca os campos
/// voláteis (jitter/trusted) a cada chamada, então o boot (IF=0) pode
/// detectar cedo sem congelar `trusted=false` para sempre.
static CAP: Mutex<Option<TimerCap>> = Mutex::new(None);

struct Policy {
    pin: Option<u64>,
    pin_src: TickSource,
    pending_hz: u64,
    pending_count: u32,
    last_change_ms: u64,
    last_good_hz: u64,
    in_dwell: bool,
}

static POLICY: Mutex<Policy> = Mutex::new(Policy {
    pin: None,
    pin_src: TickSource::Unknown,
    pending_hz: 0,
    pending_count: 0,
    last_change_ms: 0,
    last_good_hz: TICK_HZ_DEFAULT,
    in_dwell: false,
});

static FRAME_COST_US: AtomicU32 = AtomicU32::new(0);

// ─── Sanção pura (testável) ─────────────────────────────────────────────────

/// Banda do silício a partir dos fatos medidos. `counts_hz` é a taxa do
/// contador LAPIC; `u32::MAX`/`INIT_FLOOR` traduzem em cadências limite.
pub fn sanction_core(counts_hz: u64, tsc_hz: u64, jitter_ppm: u32, alive: bool) -> (u64, u64, u64, bool) {
    let hz_min = TICK_HZ_MIN.max(counts_hz / u32::MAX as u64);
    let mut hz_max = if counts_hz == 0 {
        TICK_HZ_MAX
    } else {
        TICK_HZ_MAX.min(counts_hz / INIT_FLOOR as u64)
    };
    if hz_max < hz_min {
        hz_max = hz_min;
    }
    let hz_default = TICK_HZ_DEFAULT.clamp(hz_min, hz_max);
    let trusted = counts_hz != 0
        && tsc_hz != 2_000_000_000
        && jitter_ppm <= 50_000
        && alive;
    (hz_min, hz_default, hz_max, trusted)
}

fn nearest_rail_in_band(hz: u64, min: u64, max: u64) -> u64 {
    let mut best = hz.clamp(min, max);
    let mut bestd = u64::MAX;
    for &r in RAILS.iter() {
        if r < min || r > max {
            continue;
        }
        let d = r.abs_diff(hz);
        if d < bestd {
            bestd = d;
            best = r;
        }
    }
    best
}

fn rail_index_in_band(cur: u64, c: &TimerCap) -> usize {
    let mut best = 1usize;
    let mut bestd = u64::MAX;
    for (i, &r) in RAILS.iter().enumerate() {
        if r < c.hz_min || r > c.hz_max {
            continue;
        }
        let d = r.abs_diff(cur);
        if d < bestd {
            bestd = d;
            best = i;
        }
    }
    best
}

fn dwell_ok(last_change_ms: u64, now_ms: u64) -> bool {
    now_ms.saturating_sub(last_change_ms) >= DWELL_MS
}

/// Recomendação **pura** (sem I/O): auto só mantém/baixa. Subir acima do
/// default é permitido na recomendação, mas `request_tick_hz` barra sem HITL.
fn recommend_core(c: &TimerCap, cur: u64, frame_cost_us: u32, throttled: bool) -> u64 {
    if !c.trusted {
        return c.hz_default;
    }
    let budget_us = 500_000u64 / cur.max(1);
    let over = frame_cost_us as u64 > budget_us;
    let idx = rail_index_in_band(cur, c);
    let dir: i32 = if over || throttled {
        -1
    } else if (frame_cost_us as u64).saturating_mul(4) < budget_us {
        1
    } else {
        0
    };
    let ni = (idx as i32 + dir).clamp(0, RAILS.len() as i32 - 1) as usize;
    let mut target = RAILS[ni];
    if (over || throttled) && target > c.hz_default {
        target = c.hz_default;
    }
    target = target.clamp(c.hz_min, c.hz_max);
    nearest_rail_in_band(target, c.hz_min, c.hz_max)
}

// ─── API pública ────────────────────────────────────────────────────────────

fn build_cap(counts_hz: u64, tsc_hz: u64, jitter_ppm: u32, alive: bool) -> TimerCap {
    let (hz_min, hz_default, hz_max, trusted) = sanction_core(counts_hz, tsc_hz, jitter_ppm, alive);
    let source = if counts_hz > 0 {
        TickSource::Lapic
    } else if k_nano::apic::USING_APIC.load(Ordering::Relaxed) {
        TickSource::Soft
    } else {
        TickSource::Pit
    };
    TimerCap {
        source,
        lapic_counts_hz: counts_hz,
        tsc_hz,
        hz_min,
        hz_default,
        hz_max,
        jitter_ppm,
        trusted,
    }
}

/// Mede/sanção e cacheia. Chamar no boot (pós-`init_apic`) e sempre que
/// quiser re-derivar a banda dos fatos atuais.
pub fn detect() -> TimerCap {
    let m = k_nano::apic::timer_measured();
    let c = build_cap(m.lapic_counts_hz, m.tsc_hz, m.jitter_ppm, m.alive);
    *CAP.lock() = Some(c);
    note_and_publish(&c);
    c
}

/// Cache da sanção; refresca `jitter_ppm`/`trusted` dos fatos vivos a cada
/// chamada (liveness não pode ser congelada no boot com IF=0).
pub fn cap() -> TimerCap {
    let cached = *CAP.lock();
    match cached {
        None => detect(),
        Some(mut c) => {
            let m = k_nano::apic::timer_measured();
            let (_, _, _, trusted) = sanction_core(m.lapic_counts_hz, m.tsc_hz, m.jitter_ppm, m.alive);
            c.jitter_ppm = m.jitter_ppm;
            c.trusted = trusted;
            c
        }
    }
}

/// Cadência efetiva atual (Hz).
pub fn current_tick_hz() -> u64 {
    r0_tick_hz()
}

/// Recomendação do scheduler/compositor. Default se a medição não é confiável.
pub fn recommend(frame_cost_us: u32, throttled: bool) -> u64 {
    recommend_core(&cap(), current_tick_hz(), frame_cost_us, throttled)
}

/// Aplica uma decisão de cadência. Auto só mantém/baixa; subir acima do default
/// exige HITL (a menos que o operador já tenha pinado — tratado no `pin`).
/// Confirmação: 2 decisões iguais + dwell. Rollback one-shot se regredir.
pub fn request_tick_hz(hz: u64, reason: &'static str) -> u64 {
    let c = cap();
    let now = k_nano::tsc::now_ms();
    let cur = current_tick_hz();
    let mut st = POLICY.lock();

    // Pin HITL: sempre honrado — auto não mexe nem faz rollback por cima.
    if st.pin.is_some() {
        return cur;
    }

    // Rollback one-shot: mudou e regrediu (timer morto / jitter spike).
    if st.in_dwell {
        if !dwell_ok(st.last_change_ms, now) {
            let m = k_nano::apic::timer_measured();
            if !m.alive || m.jitter_ppm > JITTER_ROLLBACK_PPM {
                let good = st.last_good_hz.clamp(c.hz_min, c.hz_max);
                unsafe { k_nano::apic::set_tick_hz(good) };
                st.in_dwell = false;
                st.pending_count = 0;
                st.last_change_ms = now;
                drop(st);
                k_nano::slog_hal!(
                    "TimerCap",
                    "warn",
                    "src=rollback hz={} reason={} (regressao no dwell)",
                    good,
                    reason
                );
                return current_tick_hz();
            }
        } else {
            st.in_dwell = false;
        }
    }

    let candidate = nearest_rail_in_band(hz.clamp(c.hz_min, c.hz_max), c.hz_min, c.hz_max);
    if candidate == cur {
        st.pending_count = 0;
        return cur;
    }

    // Subir acima do default = HITL obrigatório (Escalate ≠ Auto).
    if candidate > c.hz_default {
        st.pending_count = 0;
        drop(st);
        k_nano::slog_hal!(
            "TimerCap",
            "warn",
            "HITL: raise {} -> {} bloqueado (reason={})",
            cur,
            candidate,
            reason
        );
        return cur;
    }

    // Confirmação: 2 decisões iguais consecutivas.
    if st.pending_hz != candidate {
        st.pending_hz = candidate;
        st.pending_count = 1;
        return cur;
    }
    st.pending_count = st.pending_count.saturating_add(1);
    if st.pending_count < 2 {
        return cur;
    }
    if !dwell_ok(st.last_change_ms, now) {
        return cur;
    }

    let eff = unsafe { k_nano::apic::set_tick_hz(candidate) };
    if eff == 0 {
        st.pending_count = 0;
        return cur;
    }
    st.last_good_hz = cur;
    st.last_change_ms = now;
    st.pending_hz = eff;
    st.pending_count = 0;
    st.in_dwell = true;
    drop(st);
    note_and_publish(&c);
    k_nano::slog_hal!(
        "TimerCap",
        "ok",
        "hz={} from={} src=auto reason={} jitter={}ppm trusted={}",
        eff,
        cur,
        reason,
        c.jitter_ppm,
        c.trusted
    );
    current_tick_hz()
}

/// HITL: fixa a cadência (`Some`) ou devolve ao auto (`None`). Sempre honrado,
/// mas clampado à banda medida — fatos do silício não se sobrescrevem.
pub fn pin_tick_hz(hz: Option<u64>, src: TickSource) -> u64 {
    let c = cap();
    let mut st = POLICY.lock();
    match hz {
        None => {
            st.pin = None;
            st.pending_count = 0;
            st.in_dwell = false;
            let cur = current_tick_hz();
            drop(st);
            k_nano::slog_hal!("TimerCap", "ok", "pin=auto hz={} src={:?}", cur, src);
            cur
        }
        Some(h) => {
            let target = h.clamp(c.hz_min, c.hz_max);
            let cur = current_tick_hz();
            let eff = unsafe { k_nano::apic::set_tick_hz(target) };
            let applied = if eff == 0 { cur } else { eff };
            st.pin = Some(target);
            st.pin_src = src;
            st.pending_count = 0;
            st.last_good_hz = cur;
            st.last_change_ms = k_nano::tsc::now_ms();
            st.in_dwell = eff != 0;
            drop(st);
            note_and_publish(&c);
            k_nano::slog_hal!(
                "TimerCap",
                "ok",
                "pin hz={} band=[{},{}] src={:?}",
                applied,
                c.hz_min,
                c.hz_max,
                src
            );
            applied
        }
    }
}

/// Última amostra de custo de frame repassada por outras camadas (µs).
pub fn note_frame_cost_us(us: u32) {
    FRAME_COST_US.store(us, Ordering::Relaxed);
}

fn note_and_publish(c: &TimerCap) {
    let chosen = current_tick_hz();
    let line = alloc::format!(
        "{}Hz src={:?} trusted={} jitter={}ppm band=[{},{}]",
        chosen,
        c.source,
        c.trusted,
        c.jitter_ppm,
        c.hz_min,
        c.hz_max
    );
    k_nano::boot_report::note_timer(&line, c.trusted);
    let payload = line.into_bytes();
    let _ = k_nano::EVENT_BUS.publish(event_bus::Event {
        id: 0,
        topic: String::from(TOPIC_TIMER_CAP),
        payload,
        token: event_bus::CapabilityToken::Legacy(1),
    });
    persist_decision(chosen, c);
}

/// Seam SGDB existente (`k_nano::storage::tickv`, mesmo usado por
/// `k_nano::sgdb`) — sem inventar storage novo; sem flash, só log.
fn persist_decision(hz: u64, c: &TimerCap) {
    let inputs = alloc::format!(
        "jitter={}ppm,trusted={},band=[{},{}]",
        c.jitter_ppm,
        c.trusted,
        c.hz_min,
        c.hz_max
    );
    let json = alloc::format!(
        "{{\"hz\":{},\"source\":\"{:?}\",\"inputs\":\"{}\",\"policy_version\":\"0104\",\"ts\":{}}}",
        hz,
        c.source,
        inputs,
        k_nano::tsc::now_ms()
    );
    let written = if k_nano::storage::is_ready() {
        k_nano::storage::put_blob("hw/timer/decision", json.as_bytes()).is_ok()
    } else {
        false
    };
    k_nano::slog_hal!(
        "TimerCap",
        "ok",
        "/hw/timer/decision={} persisted={}",
        json,
        written
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mk(trusted: bool, hz_min: u64, hz_default: u64, hz_max: u64) -> TimerCap {
        TimerCap {
            source: TickSource::Lapic,
            lapic_counts_hz: 100_000_000,
            tsc_hz: 3_000_000_000,
            hz_min,
            hz_default,
            hz_max,
            jitter_ppm: 0,
            trusted,
        }
    }

    #[test]
    fn sanction_counts_zero_untrusted_and_default_kept() {
        let (mn, d, mx, t) = sanction_core(0, 3_000_000_000, 0, true);
        assert!(!t, "counts=0 é untrusted");
        assert_eq!(d, TICK_HZ_DEFAULT);
        assert!(mn <= d && d <= mx);
    }

    #[test]
    fn sanction_tsc_fallback_untrusted() {
        let (_, _, _, t) = sanction_core(100_000_000, 2_000_000_000, 0, true);
        assert!(!t, "TSC fallback 2 GHz não confia");
    }

    #[test]
    fn sanction_jitter_threshold() {
        let (_, _, _, t_hi) = sanction_core(100_000_000, 3_000_000_000, 50_001, true);
        let (_, _, _, t_ok) = sanction_core(100_000_000, 3_000_000_000, 50_000, true);
        assert!(!t_hi, "jitter >5% untrusted");
        assert!(t_ok, "jitter ==5% ainda trusted");
    }

    #[test]
    fn sanction_band_contains_default() {
        let (mn, d, mx, t) = sanction_core(100_000_000, 3_000_000_000, 0, true);
        assert!(t);
        assert!(TICK_HZ_MIN <= mn);
        assert!(mx <= TICK_HZ_MAX);
        assert!(mn <= d && d <= mx);
    }

    #[test]
    fn recommend_untrusted_is_default() {
        let c = mk(false, 30, 60, 240);
        assert_eq!(recommend_core(&c, 60, 0, false), 60);
    }

    #[test]
    fn recommend_throttled_caps_at_default() {
        let c = mk(true, 30, 60, 240);
        assert!(recommend_core(&c, 60, 0, true) <= 60);
    }

    #[test]
    fn recommend_over_budget_caps_at_default() {
        let c = mk(true, 30, 60, 240);
        assert!(recommend_core(&c, 60, 100_000, false) <= 60);
    }

    #[test]
    fn recommend_steps_one_rail_only() {
        let c = mk(true, 30, 60, 240);
        // frame barato, não throttled → sobe no máximo um degrau (60 → 120)
        assert_eq!(recommend_core(&c, 60, 1, false), 120);
    }

    #[test]
    fn recommend_respects_band_floor() {
        let c = mk(true, 60, 60, 120); // rail 30 fora da banda
        assert_eq!(recommend_core(&c, 60, 100_000, false), 60);
    }

    #[test]
    fn dwell_gate_boundary() {
        assert!(!dwell_ok(0, DWELL_MS - 1));
        assert!(dwell_ok(0, DWELL_MS));
        assert!(dwell_ok(10, 10 + DWELL_MS));
    }
}
