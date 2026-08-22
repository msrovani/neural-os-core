//! Calibração da TSC contra referências de tempo FÍSICAS (HPET → PIT 8254) e
//! sleeps em tempo real baseados em `rdtsc`.
//!
//! Problema resolvido: o wake dos APs usava `busy_wait_us` — uma contagem FIXA
//! de spins (`us * 40`) cujo atraso real varia com a velocidade do emulador
//! (QEMU TCG/WHPX) e do CPU. A calibração mede a RELAÇÃO TSC ↔ tempo real
//! usando um relógio físico: HPET (via ACPI) ou o PIT 8254 (cristal fixo
//! 1.193182 MHz, presente em QEMU e em HW real). O RATIO independe da
//! velocidade de execução: em TCG lento, tanto o `rdtsc` quanto o PIT
//! desaceleram juntos — a frequência medida é a correta em qualquer ambiente.
//!
//! Ordem das fontes: HPET (precisão fs) → PIT (universal) → CPUID 0x15/0x16
//! (estimativa do firmware — último recurso; QEMU/TCG nem sempre reporta).
//! A medição física é `#[cfg(target_os = "none")]`: em host (testes) nunca se
//! faz port I/O (ring 3 → #GP); usa-se a estimativa CPUID.

use core::sync::atomic::{AtomicU8, AtomicU64, Ordering};

/// Frequência da TSC medida (0 = não calibrada).
pub static TSC_HZ: AtomicU64 = AtomicU64::new(0);
/// Fonte da calibração: 1=HPET, 2=PIT, 3=CPUID (estimativa), 4=host (stub).
pub static TSC_SOURCE: AtomicU8 = AtomicU8::new(0);

/// Faixa de sanidade da frequência (evita medir lixo de emulação quebrada).
const HZ_MIN: u64 = 100_000_000; // 100 MHz
const HZ_MAX: u64 = 10_000_000_000; // 10 GHz
/// Femtossegundos por segundo (1 fs = 1e-15 s).
const FS_PER_SEC: u128 = 1_000_000_000_000_000;
/// Frequência do PIT 8254 (cristal).
const PIT_HZ: u128 = 1_193_182;

fn sane(hz: u64) -> bool {
    (HZ_MIN..=HZ_MAX).contains(&hz)
}

#[inline(always)]
pub fn rdtsc() -> u64 {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        core::arch::x86_64::_rdtsc()
    }
    #[cfg(not(target_arch = "x86_64"))]
    {
        0
    }
}

/// Estimativa via CPUID leaf 0x15 (razão TSC/crystal) → leaf 0x16 (base MHz).
/// É o que o firmware/emulador DECLARA — não medido. Usado como último
/// recurso e como teto de segurança nas esperas de calibração.
pub fn cpuid_estimate() -> u64 {
    #[cfg(target_arch = "x86_64")]
    unsafe {
        let max = core::arch::x86_64::__cpuid(0).eax;
        if max >= 0x15 {
            let leaf = core::arch::x86_64::__cpuid(0x15);
            let num = leaf.ebx as u64;
            let den = leaf.eax as u64;
            let crystal = leaf.ecx as u64;
            if den > 0 {
                if crystal > 0 {
                    return crystal * num / den;
                }
                if max >= 0x16 {
                    let l16 = core::arch::x86_64::__cpuid(0x16);
                    let base_mhz = (l16.eax & 0xFFFF) as u64;
                    if base_mhz > 0 {
                        return base_mhz * 1_000_000 * num / den;
                    }
                }
            }
        }
        if max >= 0x16 {
            let l16 = core::arch::x86_64::__cpuid(0x16);
            let base_mhz = (l16.eax & 0xFFFF) as u64;
            if base_mhz > 0 {
                return base_mhz * 1_000_000;
            }
        }
    }
    2_000_000_000 // fallback conservador 2 GHz
}

/// Mede a TSC contra o HPET (ACPI "HPET" → MMIO). `None` se ausente/inválido
/// ou se o contador não avança (emulação quebrada — cai para PIT).
#[cfg(target_os = "none")]
fn measure_with_hpet(pmoff: u64) -> Option<u64> {
    let phys = crate::acpi::hpet_base_phys()?;
    // mapeia a página MMIO do HPET (padrão LAPIC) — sem mapeamento → #PF
    unsafe { crate::apic::map_page_uc(phys, pmoff) };
    let base = phys + pmoff;
    let cap_est = cpuid_estimate();
    // teto de segurança: ~50 ms em ticks TSC (evita hang se HPET parado)
    let t_anchor = rdtsc();
    let t_limit = cap_est / 20;

    unsafe {
        let caps = core::ptr::read_volatile((base + 0x00) as *const u64);
        let period_fs = (caps >> 32) as u128;
        if period_fs == 0 {
            return None;
        }
        // enable: general config (0x10) bit 0
        let cfg = core::ptr::read_volatile((base + 0x10) as *const u64);
        core::ptr::write_volatile((base + 0x10) as *mut u64, cfg | 1);

        // espera o contador principal (0xF0) se mover (HPETs lentos/QEMU)
        let c0 = core::ptr::read_volatile((base + 0xF0) as *const u64);
        let mut c1 = c0;
        while c1 == c0 {
            if rdtsc().wrapping_sub(t_anchor) > t_limit {
                return None;
            }
            core::hint::spin_loop();
            c1 = core::ptr::read_volatile((base + 0xF0) as *const u64);
        }

        // janela de ~5 ms de HPET: mede (tsc_delta, hpet_delta) na MESMA janela
        let ticks_5ms = (5_000_000_000_000_000u128 / period_fs) as u64;
        let c_start = c1;
        let t_start = rdtsc();
        let mut c_end = c_start;
        loop {
            let c = core::ptr::read_volatile((base + 0xF0) as *const u64);
            if c.wrapping_sub(c_start) >= ticks_5ms {
                c_end = c;
                break;
            }
            if rdtsc().wrapping_sub(t_anchor) > t_limit {
                return None;
            }
            core::hint::spin_loop();
        }
        let t_end = rdtsc();

        let tsc_delta = t_end.wrapping_sub(t_start) as u128;
        let hpet_delta = c_end.wrapping_sub(c_start) as u128;
        if hpet_delta == 0 || tsc_delta == 0 {
            return None;
        }
        // tempo_real = hpet_delta * period_fs (fs) → hz = tsc_delta / tempo_real
        let hz = ((tsc_delta * FS_PER_SEC) / (hpet_delta * period_fs)) as u64;
        if sane(hz) {
            Some(hz)
        } else {
            None
        }
    }
}

/// Mede a TSC contra o PIT 8254 (canal 2, modo 0 one-shot, count 0xFFFF ≈
/// 54.9 ms). Universal: presente em QEMU (TCG/WHPX) e em todo HW PC.
#[cfg(target_os = "none")]
fn measure_with_pit() -> Option<u64> {
    use x86_64::instructions::port::Port;
    const PIT_CMD: u16 = 0x43;
    const PIT_CH2: u16 = 0x42;
    const PIT_STATUS: u16 = 0x61;
    const COUNT: u64 = 0xFFFF; // 54925 µs @ 1.193182 MHz
    let t_anchor = rdtsc();
    let t_limit = cpuid_estimate() / 20;
    unsafe {
        // gate 2 HIGH (bit 0) + speaker data (bit 1) → canal 2 conta
        let ctrl: u8 = Port::new(PIT_STATUS).read();
        Port::new(PIT_STATUS).write(ctrl | 0x03);
        // canal 2, lobyte/hibyte, modo 0 (one-shot), binário
        Port::new(PIT_CMD).write(0xB0u8);
        Port::new(PIT_CH2).write((COUNT & 0xFF) as u8);
        Port::new(PIT_CH2).write(((COUNT >> 8) & 0xFF) as u8);
        // modo 0: OUT2 (bit 5 de 0x61) vai a HIGH quando o contador zera
        let t0 = rdtsc();
        while (Port::<u8>::new(PIT_STATUS).read() & 0x20) == 0 {
            if rdtsc().wrapping_sub(t_anchor) > t_limit {
                return None;
            }
            core::hint::spin_loop();
        }
        let t1 = rdtsc();
        let delta = t1.wrapping_sub(t0) as u128;
        if delta == 0 {
            return None;
        }
        // hz = tsc_delta / (COUNT / PIT_HZ)
        let hz = (delta * PIT_HZ / COUNT as u128) as u64;
        if sane(hz) {
            Some(hz)
        } else {
            None
        }
    }
}

/// Calibra e cacheia a frequência da TSC (HPET → PIT → CPUID). Sempre retorna
/// um valor usável; idempotente (2ª chamada devolve o cache).
pub fn calibrate_tsc() -> u64 {
    let cached = TSC_HZ.load(Ordering::Acquire);
    if cached != 0 {
        return cached;
    }
    #[cfg(target_os = "none")]
    {
        let pmoff = crate::memory::PHYS_MEM_OFFSET.load(Ordering::Acquire);
        if pmoff != 0 {
            if let Some(hz) = measure_with_hpet(pmoff) {
                TSC_HZ.store(hz, Ordering::Release);
                TSC_SOURCE.store(1, Ordering::Release);
                crate::slog_nano!(
                    "TSC",
                    "info",
                    "calibrado {} MHz via HPET (independente do emulador)",
                    hz / 1_000_000
                );
                return hz;
            }
            if let Some(hz) = measure_with_pit() {
                TSC_HZ.store(hz, Ordering::Release);
                TSC_SOURCE.store(2, Ordering::Release);
                crate::slog_nano!(
                    "TSC",
                    "info",
                    "calibrado {} MHz via PIT 8254 (independente do emulador)",
                    hz / 1_000_000
                );
                return hz;
            }
        }
    }
    #[cfg(not(target_os = "none"))]
    {
        TSC_SOURCE.store(4, Ordering::Release);
    }
    let hz = cpuid_estimate();
    TSC_HZ.store(hz, Ordering::Release);
    TSC_SOURCE.store(3, Ordering::Release);
    crate::slog_nano!("TSC", "info", "frequência estimada via CPUID: {} Hz", hz);
    hz
}

/// Frequência usável (calibra sob demanda na 1ª chamada).
pub fn tsc_hz() -> u64 {
    let h = TSC_HZ.load(Ordering::Acquire);
    if h != 0 {
        h
    } else {
        calibrate_tsc()
    }
}

/// Nome legível da fonte (telemetria/HUD).
pub fn tsc_source_name() -> &'static str {
    match TSC_SOURCE.load(Ordering::Acquire) {
        1 => "hpet",
        2 => "pit",
        3 => "cpuid",
        4 => "host",
        _ => "uncalibrated",
    }
}

/// Sleep real em microssegundos (spin em `rdtsc`). Calibra na 1ª chamada.
/// Em host (testes) o relógio é o do CPU mesmo — só não mede PIT/HPET.
pub fn sleep_us(us: u64) {
    let hz = tsc_hz();
    if hz == 0 || us == 0 {
        return;
    }
    let target = ((us as u128) * (hz as u128) / 1_000_000) as u64;
    let start = rdtsc();
    while rdtsc().wrapping_sub(start) < target {
        core::hint::spin_loop();
    }
}

/// Sleep real em milissegundos.
pub fn sleep_ms(ms: u64) {
    sleep_us(ms.saturating_mul(1000));
}

/// Relógio monotônico em microssegundos (para deadlines; usar deltas).
pub fn now_us() -> u64 {
    let hz = tsc_hz();
    if hz == 0 {
        return 0;
    }
    ((rdtsc() as u128) * 1_000_000 / (hz as u128)) as u64
}

/// Relógio monotônico em milissegundos (atalho para now_us()/1000).
pub fn now_ms() -> u64 {
    now_us() / 1000
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_calibrate_falls_back_to_cpuid_without_port_io() {
        // Em host nunca toca PIT/HPET (port I/O em ring 3 = #GP) — deve cair
        // na estimativa CPUID com sanidade de faixa.
        let hz = calibrate_tsc();
        assert!(sane(hz), "hz={} fora da faixa de sanidade", hz);
        assert!(TSC_SOURCE.load(Ordering::Acquire) == 4 || TSC_SOURCE.load(Ordering::Acquire) == 3);
        // 2ª chamada = cache (idempotente)
        assert_eq!(calibrate_tsc(), hz);
    }

    #[test]
    fn sleep_us_terminates_and_tracks_wall_time() {
        // sleep de 5 ms deve demorar "pelo menos" ~5 ms de TSC (não pode ser
        // instantâneo) e terminar (sem hang). Comparação relativa, nunca
        // absoluta (host sob carga varia).
        let hz = tsc_hz();
        let t0 = rdtsc();
        sleep_us(5_000);
        let elapsed = rdtsc().wrapping_sub(t0);
        assert!(
            elapsed >= hz / 500, // >= 2 ms (tolerância p/ granularidade)
            "sleep de 5ms terminou cedo demais ({} ticks)",
            elapsed
        );
    }

    #[test]
    fn now_us_is_monotonic() {
        let a = now_us();
        sleep_us(1_000);
        let b = now_us();
        assert!(b >= a, "now_us não é monotônico: {} → {}", a, b);
    }
}
