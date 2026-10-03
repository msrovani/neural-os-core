//! Watchdog de silêncio (s436) — `[SILENCE]` é o equivalente do `[OOM-HALT]`
//! (s434) para spin **SEM OOM**: o log congela (serial + BOOT.LOG), o QEMU fica
//! vivo em ~1 core e nada é emitido — nem OOM-HALT (não é oom()), nem #PF novo
//! (evidência: lab longo s435, logs 183000/185325, freeze T+23334/T+23437).
//!
//! Mecanismo: a IRQ do timer (que roda com IF=1 mesmo durante spins de agente)
//! compara o tempo desde a ÚLTIMA emissão de log (`note_log_emit`, chamado do
//! choke point único `serial::dispatch_bytes` + `boot_logger::append_raw`)
//! contra um limiar. Estourou → dump de stamps de TODOS os cores via
//! `interrupts::puts` (escrita serial lock-free, mesma dos handlers de IRQ):
//!
//! - `irq c0=..s` — idade do stamp do timer IRQ (BSP): timer vivo = watchdog
//!   confiável; se o spin segurar IF=0 NO core do timer, o próprio watchdog
//!   morre junto (limite honesto — aí o instrumento é o QEMU-monitor /
//!   `tools/watch_corruption.ps1`, lição s438).
//! - `prog cN=..s` — idade do stamp de progresso por core (`note_core_progress`,
//!   chamado do loop idle do AP e do caminho pós-tick do scheduler via bin).
//!   Core com idade crescendo = não progride (spin ou bloqueio); idade fresca =
//!   loop vivo que não loga.
//! - contexto barato (atomics, zero lock): heap atual/budget, APs online,
//!   #PFs, última exceção (kind/IP/idade).
//!
//! Observe-only (lição s429-lab): NUNCA age (sem reboot/park/hlt) — só prova
//! vida e carimba, rate-limited a 1 dump / 10s (mesmo cadastro do OOM-HALT).

use core::sync::atomic::{AtomicU64, Ordering};

/// Silêncio considerado stall: 60s sem emitir NENHUMA linha de log.
/// Prefill medido ~57s com logs por slice (s413) — 60s não falso-positiva.
pub const SILENCE_THRESHOLD_US: u64 = 60_000_000;
/// Re-dump a cada 10s enquanto o silêncio persistir (cadência do OOM-HALT s434).
const DUMP_PERIOD_US: u64 = 10_000_000;
/// Stamps por core: lab 8c + margem. CPU_COUNT acima disso trunca honesto.
pub const MAX_CORES: usize = 16;
/// Gate de armadura: só vigia depois de ~100s de uptime (18Hz) — fases de boot
/// (FAT walk, model load) têm silêncios legítimos.
const ARM_TICKS: usize = 1800;

/// TSC-us da última emissão de log (0 = desarmado — nunca logou).
static LAST_EMIT_US: AtomicU64 = AtomicU64::new(0);
/// TSC-us do último dump emitido (rate-limit).
static LAST_DUMP_US: AtomicU64 = AtomicU64::new(0);
/// Dumps emitidos desde o boot (evidência de que o watchdog disparou).
pub static DUMP_COUNT: AtomicU64 = AtomicU64::new(0);
/// Idade do último timer IRQ por core (0 = sem amostra). IRQs roteiam ao BSP
/// neste kernel — na prática só c0 tem amostra; demais = "–" (n/a honesto).
static CORE_LAST_IRQ_US: [AtomicU64; MAX_CORES] = [const { AtomicU64::new(0) }; MAX_CORES];
/// Idade do último progresso por core (0 = sem amostra).
static CORE_LAST_PROG_US: [AtomicU64; MAX_CORES] = [const { AtomicU64::new(0) }; MAX_CORES];

/// Chamar a CADA linha emitida (choke point único: `serial::dispatch_bytes` e
/// `boot_logger::append_raw`). Custo: 1 store Relaxed — seguro no IRQ path.
pub fn note_log_emit() {
    LAST_EMIT_US.store(crate::tsc::now_us(), Ordering::Relaxed);
}

/// Chamar em cada iteração de loop de progresso por core (ap_idle_loop,
/// caminho pós-tick do scheduler). Barato: 1 load + 1 store Relaxed.
/// Guard: só carimba depois do 1º timer IRQ (garante PerCpu/GS válido).
pub fn note_core_progress() {
    if CORE_LAST_IRQ_US[0].load(Ordering::Relaxed) == 0 {
        return;
    }
    let idx = crate::smp::percpu::cpu_id() as usize;
    if idx < MAX_CORES {
        CORE_LAST_PROG_US[idx].store(crate::tsc::now_us(), Ordering::Relaxed);
    }
}

/// Chamar do handler da IRQ do timer (depois do fetch_add de TIMER_TICKS).
/// Faz o stamp do core do timer e executa a checagem do silêncio.
pub fn on_timer_irq() {
    CORE_LAST_IRQ_US[0].store(crate::tsc::now_us(), Ordering::Relaxed);
    check_and_dump();
}

/// Lógica de checagem — separada para teste (sem IRQ real).
pub fn check_and_dump() {
    if let Some(age) = decide(
        LAST_EMIT_US.load(Ordering::Relaxed),
        LAST_DUMP_US.load(Ordering::Relaxed),
        crate::tsc::now_us(),
        ticks_now(),
    ) {
        let now = crate::tsc::now_us();
        LAST_DUMP_US.store(now, Ordering::Relaxed);
        let n = DUMP_COUNT.fetch_add(1, Ordering::Relaxed) + 1;
        dump_line(now, age, n);
    }
}

/// Decisão PURA (testável com tempo injetado): `Some(age_us)` = dump agora;
/// `None` = não dumpar (desarmado / ainda fresco / rate-limit).
fn decide(last_emit_us: u64, last_dump_us: u64, now_us: u64, ticks: usize) -> Option<u64> {
    if ticks < ARM_TICKS {
        return None; // boot: silêncios legítimos (FAT walk, model load)
    }
    if last_emit_us == 0 {
        return None; // nunca logou — desarmado
    }
    let age = now_us.saturating_sub(last_emit_us);
    if age < SILENCE_THRESHOLD_US {
        return None;
    }
    if last_dump_us != 0 && now_us.saturating_sub(last_dump_us) < DUMP_PERIOD_US {
        return None; // rate-limit 1 dump / 10s (cadência OOM-HALT)
    }
    Some(age)
}

/// TIMER_TICKS (AtomicUsize em interrupts.rs) — leitura Relaxed barata no IRQ.
fn ticks_now() -> usize {
    crate::interrupts::TIMER_TICKS.load(Ordering::Relaxed)
}

/// Idade em segundos (inteiro) de um stamp; 0 = n/a honesto.
fn age_s(stamp_us: u64, now: u64) -> Option<u64> {
    if stamp_us == 0 {
        None
    } else {
        Some(now.saturating_sub(stamp_us) / 1_000_000)
    }
}

/// Escritor lock-free/alloc-free sobre buffer fixo (IRQ-safe).
struct StampSink<'a> {
    buf: &'a mut [u8],
    pos: usize,
}

impl<'a> StampSink<'a> {
    fn put(&mut self, s: &[u8]) {
        let room = self.buf.len().saturating_sub(self.pos);
        let n = s.len().min(room);
        self.buf[self.pos..self.pos + n].copy_from_slice(&s[..n]);
        self.pos += n;
    }
    fn put_num(&mut self, v: u64) {
        let mut digits = [0u8; 20];
        let mut d = 0usize;
        if v == 0 {
            digits[0] = b'0';
            d = 1;
        }
        let mut v = v;
        while v > 0 {
            digits[d] = b'0' + (v % 10) as u8;
            v /= 10;
            d += 1;
        }
        while d > 0 {
            d -= 1;
            self.put(&digits[d..d + 1]);
        }
    }
    /// Idade em s ou "–" (n/a).
    fn put_age(&mut self, stamp: u64, now: u64) {
        match age_s(stamp, now) {
            Some(s) => self.put_num(s),
            None => self.put(b"\xE2\x80\x93"), // – (en dash, honesto n/a)
        }
    }
}

fn dump_line(now: u64, age_us: u64, dump_n: u64) {
    let mut buf = [0u8; 640];
    let mut w = StampSink { buf: &mut buf, pos: 0 };

    w.put(b"[SILENCE] log parado ha ");
    w.put_num(age_us / 1_000_000);
    w.put(b"s tick=");
    w.put_num(ticks_now() as u64);
    w.put(b" dump#");
    w.put_num(dump_n);
    w.put(b" heap=");
    w.put_num(crate::allocator::CURRENT_HEAP_MB.load(Ordering::Relaxed) as u64);
    w.put(b"/");
    w.put_num(crate::allocator::HEAP_BUDGET_MB.load(Ordering::Relaxed) as u64);
    w.put(b"M aps=");
    w.put_num(crate::smp::percpu::AP_ONLINE.load(Ordering::Relaxed) as u64);
    w.put(b" pf=");
    w.put_num(crate::interrupts::PAGE_FAULT_COUNT.load(Ordering::Relaxed) as u64);
    let exc_kind = crate::interrupts::LAST_EXC_KIND.load(Ordering::Relaxed);
    if exc_kind != 0 {
        w.put(b" exc=");
        w.put_num(exc_kind as u64);
        w.put(b"@");
        w.put_num(crate::interrupts::LAST_EXC_IP.load(Ordering::Relaxed));
        w.put(b" ha ");
        w.put_num(
            now.saturating_sub(crate::interrupts::LAST_EXC_TSC.load(Ordering::Relaxed)) / 1_000_000,
        );
        w.put(b"s");
    }
    w.put(b" | irq:");
    for i in 0..MAX_CORES {
        if i >= 8 {
            break; // lab 8c — lista capada (stamps truncam honesto)
        }
        w.put(b" c");
        w.put_num(i as u64);
        w.put(b"=");
        w.put_age(CORE_LAST_IRQ_US[i].load(Ordering::Relaxed), now);
    }
    w.put(b" | prog:");
    for i in 0..MAX_CORES {
        if i >= 8 {
            break;
        }
        w.put(b" c");
        w.put_num(i as u64);
        w.put(b"=");
        w.put_age(CORE_LAST_PROG_US[i].load(Ordering::Relaxed), now);
    }
    w.put(b"\n");
    // puts = escrita serial lock-free dos handlers de IRQ — NUNCA SERIAL.lock()
    // (o spinner pode estar segurando o lock do serial: é UMA causa de silêncio).
    crate::interrupts::puts(&w.buf[..w.pos]);
}

/// Resets para testes (statics compartilhados — lição SESSION_346: serializar).
#[cfg(test)]
pub fn reset_for_tests() {
    LAST_EMIT_US.store(0, Ordering::Relaxed);
    LAST_DUMP_US.store(0, Ordering::Relaxed);
    DUMP_COUNT.store(0, Ordering::Relaxed);
    for c in CORE_LAST_IRQ_US.iter() {
        c.store(0, Ordering::Relaxed);
    }
    for c in CORE_LAST_PROG_US.iter() {
        c.store(0, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idade_nunca_negativa_e_na_quando_sem_amostra() {
        // n/a honesto (n/a ≠ 0, regra s411): stamp 0 → None.
        assert_eq!(age_s(0, 5_000_000), None);
        assert_eq!(age_s(5_000_000, 5_000_000), Some(0));
        assert_eq!(age_s(1_000_000, 5_000_000), Some(4));
        // wrap/saturação: stamp "no futuro" (TSC wrap) → 0, não panic/lixo.
        assert_eq!(age_s(9_000_000, 1_000_000), Some(0));
    }

    #[test]
    fn sink_capa_e_trunca_sem_panic() {
        let mut buf = [0u8; 8];
        let mut w = StampSink { buf: &mut buf, pos: 0 };
        w.put(b"0123456789");
        assert_eq!(w.pos, 8);
        assert_eq!(&buf, b"01234567");
    }

    #[test]
    fn sink_num_formata() {
        let mut buf = [0u8; 24];
        let pos = {
            let mut w = StampSink { buf: &mut buf, pos: 0 };
            w.put_num(0);
            w.put(b"|");
            w.put_num(12345);
            w.pos
        };
        assert_eq!(&buf[..pos], b"0|12345");
    }

    #[test]
    fn decide_pura_threshold_e_rate_limit() {
        const T0: u64 = 1_000_000_000; // TSC-us arbitrário (wrap-safe via saturating)
        // Boot (ticks baixos): nunca dumpa.
        assert_eq!(decide(T0, 0, T0 + 999_000_000, 10), None);
        // Desarmado (nunca logou): nunca dumpa mesmo com tick alto.
        assert_eq!(decide(0, 0, T0 + 999_000_000, ARM_TICKS), None);
        // Log fresco: sem dump.
        assert_eq!(decide(T0 + 999_000_000, 0, T0 + 999_000_000 + 1000, ARM_TICKS), None);
        // Silêncio de 61s: dump com idade ~61s.
        let age = decide(T0 + 999_000_000, 0, T0 + 999_000_000 + 61_000_000, ARM_TICKS).unwrap();
        assert_eq!(age / 1_000_000, 61);
        // Silêncio de 59s: ainda dentro do limiar.
        assert_eq!(decide(T0 + 999_000_000, 0, T0 + 999_000_000 + 59_000_000, ARM_TICKS), None);
        // Rate-limit: dump < 10s do último → None; ≥ 10s → Some.
        let last_dump = T0 + 999_000_000 + 61_000_000;
        assert_eq!(decide(T0 + 999_000_000, last_dump, last_dump + 9_999_999, ARM_TICKS), None);
        assert_eq!(
            decide(T0 + 999_000_000, last_dump, last_dump + 10_000_000, ARM_TICKS).is_some(),
            true
        );
    }

    #[test]
    fn desarmado_sem_log_nao_dumpan() {
        reset_for_tests();
        // No host TIMER_TICKS=0 → decide() corta no gate de ticks.
        check_and_dump();
        assert_eq!(DUMP_COUNT.load(Ordering::Relaxed), 0);
    }
}
