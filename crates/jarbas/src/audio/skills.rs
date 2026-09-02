use alloc::string::String;
use alloc::vec::Vec;
use core::sync::atomic::Ordering;
use skill_registry::{McpManifest, OutputSchema, Skill};
use spin;

use crate::audio::piper::{piper_blob_size, PiperEngine, is_piper_header};
use crate::audio::vad::{VadTransition, VAD};

static TTS_ENGINE: spin::Mutex<Option<PiperEngine>> = spin::Mutex::new(None);

const PIPER_FAT_NAMES: &[&str] = &["PIPER.BIN", "PIPER_EN.BIN", "PIPER_PT_BR.BIN"];

pub fn init_neural_tts() {
    if piper_is_loaded() {
        return;
    }
    let engine = try_load_piper();
    if engine.is_some() {
        k_nano::load_status::set(
            k_nano::load_status::AssetKind::Piper,
            k_nano::load_status::LoadStatus::Loaded,
        );
    }
    // Absent/Failed já definidos em try_load_piper (não sobrescrever Failed com Absent)
    *TTS_ENGINE.lock() = engine;
}

/// Sintetiza texto: Piper se carregado, senão formant. Logs `[TTS] Piper` / `[TTS] Formant`.
pub fn synthesize_tts(text: &str) -> Vec<i16> {
    let guard = TTS_ENGINE.lock();
    match guard.as_ref() {
        Some(engine) if engine.is_loaded() => {
            let audio = engine.generate(text);
            k_nano::slog_jarbas!(
                "Jarbas",
                "ok",
                "TTS Piper: \"{}\" ({} samples)",
                text.chars().take(48).collect::<String>(),
                audio.len()
            );
            audio
        }
        _ => {
            let audio = crate::audio::tts::synthesize(text);
            k_nano::slog_jarbas!(
                "Jarbas",
                "ok",
                "TTS Formant: \"{}\" ({} samples)",
                text.chars().take(48).collect::<String>(),
                audio.len()
            );
            audio
        }
    }
}

/// True se Piper neural está carregado e utilizável.
pub fn piper_is_loaded() -> bool {
    TTS_ENGINE
        .lock()
        .as_ref()
        .map(|e| e.is_loaded())
        .unwrap_or(false)
}

/// Smoke test: valida que Piper gera audio com amplitude > 0.
pub fn piper_smoke_test() -> bool {
    let guard = TTS_ENGINE.lock();
    if let Some(ref engine) = *guard {
        if !engine.is_loaded() {
            return false;
        }
        let pcm = engine.generate("hello");
        if pcm.is_empty() {
            k_nano::slog_jarbas!("Audio", "piper", "SMOKE FAIL: empty output");
            return false;
        }
        let max_amp = pcm.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0);
        if max_amp < 100 {
            k_nano::slog_jarbas!(
                "Audio",
                "piper",
                "SMOKE FAIL: max amplitude={} (< 100, pesos corrompidos?)",
                max_amp
            );
            return false;
        }
        k_nano::slog_jarbas!(
            "Audio",
            "piper",
            "SMOKE OK: {} samples, max_amp={}",
            pcm.len(),
            max_amp
        );
        true
    } else {
        false
    }
}

fn try_load_piper() -> Option<PiperEngine> {
    // 1) QEMU -device loader scan [0x100000000..0x180000000) — magic Piper v3
    if let Some(eng) = try_load_piper_from_loader_scan() {
        return Some(eng);
    }
    // 2) virtio-blk data disk (disk_qemu.raw) — rápido no TCG 4c
    if let Some(eng) = try_load_piper_from_virtio_fat() {
        return Some(eng);
    }
    // 3) ATA PIO — só arquivos pequenos ou HW sem virtio (PACK_LLM ~61MB congela TCG)
    try_load_piper_from_ata_fat()
}

fn try_load_piper_from_loader_scan() -> Option<PiperEngine> {
    let pm = k_nano::memory::PHYS_MEM_OFFSET.load(Ordering::Relaxed);
    let mut addr = 0x1000_0000_0u64;
    while addr < 0x1800_0000_0 {
        let va = addr.saturating_add(pm);
        if !k_nano::memory::is_page_present(va) {
            addr = addr.saturating_add(0x100_000);
            continue;
        }
        let ptr = va as *const u8;
        let hdr = unsafe { core::slice::from_raw_parts(ptr, 512) };
        if !is_piper_header(hdr) {
            addr = addr.saturating_add(0x100_000);
            continue;
        }
        let n = u32::from_le_bytes(hdr[8..12].try_into().ok()?) as usize;
        let idx_bytes = 16usize.saturating_add(n.saturating_mul(40)).min(256 * 1024);
        if mapped_len(va, idx_bytes) < idx_bytes {
            addr = addr.saturating_add(0x100_000);
            continue;
        }
        let idx = unsafe { core::slice::from_raw_parts(ptr, idx_bytes) };
        let Some(mut sz) = piper_blob_size(idx) else {
            addr = addr.saturating_add(0x100_000);
            continue;
        };
        let mapped = mapped_len(va, sz);
        if mapped < 4096 {
            addr = addr.saturating_add(0x100_000);
            continue;
        }
        sz = mapped.min(sz);
        k_nano::slog_jarbas!(
            "Audio",
            "piper",
            "QEMU-loader @{:#x} Piper v3 — parse {} KB…",
            addr,
            sz / 1024
        );
        let data = unsafe { core::slice::from_raw_parts(ptr, sz) };
        let mut eng = PiperEngine::new();
        if eng.load(data) {
            k_nano::slog_jarbas!(
                "Audio",
                "ok",
                "Piper TTS LOADED (QEMU-loader @{:#x}) size={} KB",
                addr,
                sz / 1024
            );
            return Some(eng);
        }
        k_nano::slog_jarbas!(
            "Audio",
            "piper",
            "QEMU-loader @{:#x} parse FAILED — fallback FAT",
            addr
        );
        addr = addr.saturating_add(0x100_000);
    }
    None
}

fn mapped_len(base_va: u64, want: usize) -> usize {
    let page = 4096usize;
    let mut ok = 0usize;
    while ok < want {
        if !k_nano::memory::is_page_present(base_va.saturating_add(ok as u64)) {
            break;
        }
        ok = ok.saturating_add(page).min(want);
    }
    ok
}

unsafe fn try_load_piper_from_fat_dev(
    dev: &mut dyn k_nano::block_dev::BlockDevice,
    backend: &str,
    max_bytes: usize,
) -> Option<PiperEngine> {
    let parts = k_nano::fat32::read_mbr_dev(dev);
    for p in &parts {
        if !matches!(p.type_code, 0x0B | 0x0C | 0x1C | 0xEF) {
            continue;
        }
        for name in PIPER_FAT_NAMES {
            let Some(sz) = k_nano::fat32::lookup_root_file_size_dev(dev, p, name) else {
                continue;
            };
            if sz > max_bytes {
                k_nano::slog_jarbas!(
                    "Audio",
                    "piper",
                    "skip {} {}KB via {} (>{}MB cap)",
                    name,
                    sz / 1024,
                    backend,
                    max_bytes / (1024 * 1024)
                );
                continue;
            }
            k_nano::slog_jarbas!(
                "Audio",
                "piper",
                "{} presente ({} KB) via {} — read…",
                name,
                sz / 1024,
                backend
            );
            let Some(data) = k_nano::fat32::read_root_file_dev(dev, p, name) else {
                k_nano::slog_jarbas!("Audio", "piper", "{} I/O read FAILED via {}", name, backend);
                continue;
            };
            if data.len() < sz.min(16) {
                k_nano::slog_jarbas!(
                    "Audio",
                    "piper",
                    "{} I/O short read ({} / {} KB) via {}",
                    name,
                    data.len() / 1024,
                    sz / 1024,
                    backend
                );
                continue;
            }
            let mut eng = PiperEngine::new();
            if eng.load(&data) {
                k_nano::slog_jarbas!(
                    "Audio",
                    "ok",
                    "Piper TTS LOADED from {} ({}): {} KB",
                    backend,
                    name,
                    data.len() / 1024
                );
                return Some(eng);
            }
            k_nano::slog_jarbas!(
                "Audio",
                "piper",
                "{} parse FAILED via {} ({} KB)",
                name,
                backend,
                data.len() / 1024
            );
        }
    }
    None
}

fn try_load_piper_from_virtio_fat() -> Option<PiperEngine> {
    let mut g = k_nano::virtio_blk::VIRTIO_BLK_DEV.lock();
    let vb = g.as_mut()?;
    // virtio-blk: até 128MB inline (read_root_file_dev cap 256MB)
    unsafe { try_load_piper_from_fat_dev(vb, "virtio-blk", 128 * 1024 * 1024) }
}

fn try_load_piper_from_ata_fat() -> Option<PiperEngine> {
    let slow_ata = k_nano::storage_bw::skip_measure()
        || k_nano::virtio_blk::VIRTIO_BLK_DEV.lock().is_some();
    let max_bytes = if slow_ata {
        8 * 1024 * 1024
    } else {
        128 * 1024 * 1024
    };
    let mut found_any = false;
    let mut load_failed = false;
    unsafe {
        for try_slave in &[false, true] {
            let mut tmp = k_nano::ATA_DRIVER.lock();
            let Some(ref mut ata) = *tmp else {
                continue;
            };
            let orig = ata.slave;
            ata.slave = *try_slave;
            let parts = k_nano::fat32::read_mbr(ata);
            for p in &parts {
                if !matches!(p.type_code, 0x0B | 0x0C | 0x1C) {
                    continue;
                }
                if let Some(fs) = k_nano::fat32::Fat32Reader::new(ata, p) {
                    for name in PIPER_FAT_NAMES {
                        let Some(sz) = fs.lookup_file_size(name) else {
                            continue;
                        };
                        found_any = true;
                        if sz > max_bytes {
                            k_nano::slog_jarbas!(
                                "Audio",
                                "piper",
                                "skip ATA PIO {} {}KB (boot; formant TTS)",
                                name,
                                sz / 1024
                            );
                            continue;
                        }
                        let Some(data) = fs.read_file(name) else {
                            load_failed = true;
                            continue;
                        };
                        let mut eng = PiperEngine::new();
                        if eng.load(&data) {
                            k_nano::slog_jarbas!(
                                "Audio",
                                "piper",
                                "Piper TTS LOADED from FAT ({}): {} ({} KB)",
                                if *try_slave { "slave" } else { "master" },
                                name,
                                data.len() / 1024
                            );
                            ata.slave = orig;
                            return Some(eng);
                        }
                        load_failed = true;
                    }
                }
            }
            ata.slave = orig;
        }
    }
    if load_failed || found_any {
        k_nano::load_status::set(
            k_nano::load_status::AssetKind::Piper,
            k_nano::load_status::LoadStatus::Failed,
        );
        k_nano::slog_jarbas!(
            "Audio",
            "piper",
            "Piper TTS FAILED — arquivo presente mas nao carregavel; formant fallback"
        );
    } else {
        k_nano::load_status::set(
            k_nano::load_status::AssetKind::Piper,
            k_nano::load_status::LoadStatus::Absent,
        );
        k_nano::slog_jarbas!("Audio", "piper", "Piper TTS ausente no FAT — formant synth ativo");
    }
    None
}

pub struct TtsSkill;

impl Skill for TtsSkill {
    fn manifest(&self) -> McpManifest {
        McpManifest {
            name: String::from("tts_speak"),
            description: String::from("Sintetiza texto em audio PCM (neural GPU ou formant CPU)"),
            required_tokens: Vec::new(),
            preconditions: Vec::new(),
            context_links: Vec::new(),
            output_schema: OutputSchema::Any,
            idempotent: false,
            contracts: Vec::new(),
        }
    }

    fn execute(&self, input: &[u8]) -> Result<Vec<u8>, &'static str> {
        let text = core::str::from_utf8(input).map_err(|_| "UTF-8 invalido")?;
        let pcm = synthesize_tts(text);
        Ok(pcm.iter().flat_map(|s| s.to_le_bytes()).collect())
    }
}

pub struct SttSkill;

impl Skill for SttSkill {
    fn manifest(&self) -> McpManifest {
        McpManifest {
            name: String::from("stt_transcribe"),
            description: String::from("Transcreve PCM i16 LE via CTC global (mesmo path do runtime)"),
            required_tokens: Vec::new(),
            preconditions: Vec::new(),
            context_links: Vec::new(),
            output_schema: OutputSchema::String,
            idempotent: false,
            contracts: Vec::new(),
        }
    }

    fn execute(&self, input: &[u8]) -> Result<Vec<u8>, &'static str> {
        let pcm: &[i16] = unsafe {
            core::slice::from_raw_parts(input.as_ptr() as *const i16, input.len() / 2)
        };
        if pcm.is_empty() {
            return Ok(alloc::vec![b'?']);
        }

        let mut vad = VAD::new(crate::audio::settings::vad_threshold(), 16000);
        let mut speech_segments = 0u32;
        for chunk in pcm.chunks(320) {
            if chunk.len() < 320 {
                continue;
            }
            let (_e, _z, _a, transition) = vad.process_frame(chunk);
            if transition == VadTransition::SpeechStart {
                speech_segments += 1;
            }
        }

        let text = crate::audio::stt::transcribe_global(pcm);
        let result = if text.is_empty() {
            alloc::format!("[STT] empty vad_segs={} samples={}", speech_segments, pcm.len())
        } else {
            text
        };
        k_nano::slog_bin!("Audio", "stt", "skill: {}", result);
        Ok(result.into_bytes())
    }
}
