//! Skill Lab (F1.5 — OPCODE-0052/0058): hook de lab determinístico via
//! QEMU-loader. Magic `LSK1` @ phys 0x0211_0000 → gera (`G`) ou reusa (`E`)
//! uma skill WASM e a executa, carimbando ACT/ESCALATE no boot_report.
//! One-shot (zera o magic). Espelha `lab_inject.rs` (LINJ @0x0210_0000).
//!
//! Blob: `LSK1`(4) + op(1: b'G'|b'E') + name\0 + text\0 (vazio p/ E) + a\0 + b\0.

use alloc::string::String;
use core::sync::atomic::{AtomicBool, Ordering};

/// Phys do loader (vizinho do LINJ) — cabe em guest >=32MB.
pub const LAB_SKILL_PHYS: u64 = 0x0211_0000;
const MAGIC: &[u8; 4] = b"LSK1";
const BLOB_LEN: usize = 512;

static CONSUMED: AtomicBool = AtomicBool::new(false);

/// FNV-1a 64 (offset 0xcbf2_9ce4_8422_2325, prime 0x1_0000_01b3).
fn fnv1a64(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// Lê uma string NUL-terminada de `buf` a partir de `i`; devolve (String, próximo i).
fn cstr_from(buf: &[u8], i: usize) -> Option<(String, usize)> {
    let start = i;
    let mut j = i;
    while j < buf.len() && buf[j] != 0 {
        j += 1;
    }
    if j >= buf.len() {
        return None; // sem NUL
    }
    let s = core::str::from_utf8(&buf[start..j]).ok()?;
    Some((String::from(s), j + 1))
}

fn run_and_log(name: &str, a: i32, b: i32, bytes: &[u8]) {
    match crate::wasmi_rt::run_i32_2(bytes, "run", a, b, 0) {
        Ok(v) => k_nano::slog_hermes!(
            "SKILL_LAB",
            "ok",
            "run name={} a={} b={} result={}",
            name,
            a,
            b,
            v
        ),
        Err(e) => k_nano::slog_hermes!("SKILL_LAB", "warn", "run name={} err={}", name, e),
    }
}

/// op G: gera (promote model-text → WASM) e roda.
fn op_generate(name: &str, text: &str, a: i32, b: i32) {
    match crate::evolve::promote_model_text_to_wasm(name, "lab", text) {
        Ok(()) => {
            let key = alloc::format!("skill/wasm/{}", name);
            match k_nano::storage::get_blob(&key) {
                Ok(bytes) => {
                    k_nano::slog_hermes!(
                        "SKILL_LAB",
                        "ok",
                        "act=gen name={} prov=model-born bytes={} hash={:#x}",
                        name,
                        bytes.len(),
                        fnv1a64(&bytes)
                    );
                    k_nano::boot_report::inc_act(1);
                    run_and_log(name, a, b, &bytes);
                }
                Err(e) => {
                    k_nano::slog_hermes!(
                        "SKILL_LAB",
                        "warn",
                        "escalate=gen name={} err=get_blob {}",
                        name,
                        e
                    );
                    k_nano::boot_report::inc_escalate(1);
                }
            }
        }
        Err(e) => {
            k_nano::slog_hermes!("SKILL_LAB", "warn", "escalate=gen name={} err={}", name, e);
            k_nano::boot_report::inc_escalate(1);
        }
    }
}

/// op E: reusa (blob já persistido no Tickv) e roda.
fn op_reuse(name: &str, a: i32, b: i32) {
    let has = crate::globals::SKILL_REGISTRY.lock().has_skill(name);
    let key = alloc::format!("skill/wasm/{}", name);
    match k_nano::storage::get_blob(&key) {
        Ok(bytes) => {
            k_nano::slog_hermes!(
                "SKILL_LAB",
                "ok",
                "reuse name={} has_skill={} hash={:#x}",
                name,
                has as u8,
                fnv1a64(&bytes)
            );
            match crate::wasmi_rt::run_i32_2(&bytes, "run", a, b, 0) {
                Ok(v) => {
                    k_nano::slog_hermes!(
                        "SKILL_LAB",
                        "ok",
                        "act=reuse name={} a={} b={} result={}",
                        name,
                        a,
                        b,
                        v
                    );
                    k_nano::boot_report::inc_act(1);
                }
                Err(e) => {
                    k_nano::slog_hermes!("SKILL_LAB", "warn", "act=reuse name={} err={}", name, e);
                    k_nano::boot_report::inc_escalate(1);
                }
            }
        }
        Err(_) => {
            k_nano::slog_hermes!(
                "SKILL_LAB",
                "warn",
                "escalate=reuse name={} reason=not_found",
                name
            );
            k_nano::boot_report::inc_escalate(1);
        }
    }
}

/// Lê o blob LSK1 do loader, despacha G/E uma vez e devolve Some(()) se agiu.
pub fn poll_and_run() -> Option<()> {
    if CONSUMED.load(Ordering::Relaxed) {
        return None;
    }
    let pmoff = k_nano::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    if pmoff == 0 {
        return None;
    }
    let mut raw = [0u8; BLOB_LEN];
    unsafe {
        k_nano::apic::map_page_uc(LAB_SKILL_PHYS, pmoff);
        let p = (LAB_SKILL_PHYS + pmoff) as *const u8;
        for (i, b) in raw.iter_mut().enumerate() {
            *b = core::ptr::read_volatile(p.add(i));
        }
    }
    if &raw[0..4] != MAGIC {
        return None;
    }
    // consumir: zera o magic (one-shot; evita re-run)
    unsafe {
        let p = (LAB_SKILL_PHYS + pmoff) as *mut u8;
        for i in 0..4 {
            core::ptr::write_volatile(p.add(i), 0);
        }
    }
    CONSUMED.store(true, Ordering::Release);

    let op = raw[4];
    let Some((name, i)) = cstr_from(&raw, 5) else {
        k_nano::slog_hermes!("SKILL_LAB", "warn", "LSK1 name sem NUL — skip");
        return None;
    };
    let Some((text, i)) = cstr_from(&raw, i) else {
        k_nano::slog_hermes!("SKILL_LAB", "warn", "LSK1 text sem NUL — skip");
        return None;
    };
    let Some((a_s, i)) = cstr_from(&raw, i) else {
        k_nano::slog_hermes!("SKILL_LAB", "warn", "LSK1 a sem NUL — skip");
        return None;
    };
    let Some((b_s, _)) = cstr_from(&raw, i) else {
        k_nano::slog_hermes!("SKILL_LAB", "warn", "LSK1 b sem NUL — skip");
        return None;
    };
    if name.is_empty() {
        k_nano::slog_hermes!("SKILL_LAB", "warn", "LSK1 nome vazio — skip");
        return None;
    }
    let a: i32 = a_s.trim().parse().unwrap_or(0);
    let b: i32 = b_s.trim().parse().unwrap_or(0);
    match op {
        b'G' => op_generate(&name, &text, a, b),
        b'E' => op_reuse(&name, a, b),
        other => {
            k_nano::slog_hermes!(
                "SKILL_LAB",
                "warn",
                "LSK1 op desconhecido {:#x} — skip",
                other
            );
            return None;
        }
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn magic_and_phys_contract() {
        assert_eq!(MAGIC, b"LSK1");
        assert_eq!(LAB_SKILL_PHYS, 0x0211_0000);
    }

    #[test]
    fn fnv1a64_known_vectors() {
        assert_eq!(fnv1a64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a64(b"a"), 0xaf63_dc4c_8601_ec8c);
    }

    #[test]
    fn cstr_from_parses_nul_terminated() {
        let buf = b"abc\0def\0";
        let (s, i) = cstr_from(buf, 0).unwrap();
        assert_eq!(s, "abc");
        let (s2, _) = cstr_from(buf, i).unwrap();
        assert_eq!(s2, "def");
        assert!(cstr_from(b"no-nul", 0).is_none());
    }

    /// Expressão real via op-IR: `(a*a + 3*b) - 5`.
    #[test]
    fn run_expression_aa_plus_3b_minus_5() {
        let (n_params, ops) =
            crate::wasm_build::model_text_to_ops("(a*a + 3*b) - 5").expect("op-IR parse");
        let wasm = crate::wasm_build::build_run_module(n_params, &ops).expect("build_run_module");
        assert_eq!(crate::wasmi_rt::run_i32_2(&wasm, "run", 6, 7, 0).unwrap(), 52);
        assert_eq!(crate::wasmi_rt::run_i32_2(&wasm, "run", 9, 2, 0).unwrap(), 82);
    }
}
