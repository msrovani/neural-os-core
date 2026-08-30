use alloc::vec::Vec;
use alloc::string::String;
use skill_registry::{Skill, McpManifest, OutputSchema};
use spin;
use crate::audio::vad::{VAD, VadTransition};
use crate::audio::piper::PiperEngine;

static TTS_ENGINE: spin::Mutex<Option<PiperEngine>> = spin::Mutex::new(None);

pub fn init_neural_tts() {
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
            k_nano::slog_bin!("Audio", "tts", "Piper: \"{}\" ({} samples)",
                text,
                audio.len());
            audio
        }
        _ => {
            let audio = crate::audio::tts::synthesize(text);
            k_nano::slog_bin!("Audio", "tts", "Formant: \"{}\" ({} samples)",
                text,
                audio.len());
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
/// Chamar após load() para detectar pesos corrompidos.
pub fn piper_smoke_test() -> bool {
    let guard = TTS_ENGINE.lock();
    if let Some(ref engine) = *guard {
        if !engine.is_loaded() { return false; }
        let pcm = engine.generate("hello");
        if pcm.is_empty() {
            k_nano::slog_jarbas!("Audio", "piper", "SMOKE FAIL: empty output");
            return false;
        }
        // Verificar amplitude máxima > 100 (não silêncio)
        let max_amp = pcm.iter().map(|s| s.unsigned_abs()).max().unwrap_or(0);
        if max_amp < 100 {
            k_nano::slog_jarbas!("Audio", "piper", "SMOKE FAIL: max amplitude={} (< 100, pesos corrompidos?)", max_amp);
            return false;
        }
        k_nano::slog_jarbas!("Audio", "piper", "SMOKE OK: {} samples, max_amp={}", pcm.len(), max_amp);
        true
    } else {
        false
    }
}

fn try_load_piper() -> Option<PiperEngine> {
    // 1) QEMU -device loader @0x124200000 (WHPX rápido; evita PIO ~61MB)
    if let Some(eng) = try_load_piper_from_loader() {
        return Some(eng);
    }
    // 2) FAT PIO — sempre carregar quando presente (motor TTS = Piper)
    let mut found_any = false;
    let mut load_failed = false;
    unsafe {
        for try_slave in &[false, true] {
            let mut tmp = k_nano::ATA_DRIVER.lock();
            if let Some(ref mut ata) = *tmp {
                let orig = ata.slave;
                ata.slave = *try_slave;
                let parts = k_nano::fat32::read_mbr(ata);
                for p in &parts {
                    if p.type_code != 0x1C && p.type_code != 0x0C && p.type_code != 0x0B {
                        continue;
                    }
                    if let Some(fs) = k_nano::fat32::Fat32Reader::new(ata, p) {
                        for name in &["PIPER.BIN", "PIPER_EN.BIN", "PIPER_PT_BR.BIN"] {
                            let Some(sz) = fs.lookup_file_size(name) else {
                                continue;
                            };
                            found_any = true;
                            k_nano::slog_jarbas!("Audio", "piper", "{} presente ({} KB) — carregando via PIO (read_file)…",
                                name,
                                sz / 1024);
                            // PACK_LLM: PIPER ~61MB ATA PIO congela apos K50 (greet).
                            // Formant ja falou; Piper fica para runtime/loader.
                            if sz > 8 * 1024 * 1024 {
                                k_nano::slog_jarbas!(
                                    "Audio",
                                    "piper",
                                    "skip ATA PIO {} {}KB (boot; formant TTS)",
                                    name,
                                    sz / 1024
                                );
                                continue;
                            }
                            let data = match fs.read_file(name) {
                                Some(d) => d,
                                None => {
                                    k_nano::slog_jarbas!("Audio", "piper", "{} I/O read FAILED",
                                        name);
                                    load_failed = true;
                                    continue;
                                }
                            };
                            if data.len() < sz.min(16) {
                                k_nano::slog_jarbas!("Audio", "piper", "{} I/O read FAILED ({} / {} KB)",
                                    name,
                                    data.len() / 1024,
                                    sz / 1024);
                                load_failed = true;
                                continue;
                            }
                            k_nano::slog_jarbas!("Audio", "piper", "{} lido {} KB — parse…",
                                name,
                                data.len() / 1024);
                            let mut eng = PiperEngine::new();
                            if eng.load(&data) {
                                k_nano::slog_jarbas!("Audio", "piper", "Piper TTS LOADED from FAT ({}): {} ({} KB)",
                                    if *try_slave { "slave" } else { "master" },
                                    name,
                                    data.len() / 1024);
                                ata.slave = orig;
                                return Some(eng);
                            }
                            k_nano::slog_jarbas!("Audio", "piper", "{} lido ({} KB) mas load() falhou (magic/tensors)",
                                name,
                                data.len() / 1024);
                            load_failed = true;
                        }
                    }
                }
                ata.slave = orig;
            }
        }
    }
    if load_failed || found_any {
        k_nano::load_status::set(
            k_nano::load_status::AssetKind::Piper,
            k_nano::load_status::LoadStatus::Failed,
        );
        k_nano::slog_jarbas!("Audio", "piper", "Piper TTS FAILED — arquivo presente mas nao carregavel; formant fallback");
    } else {
        k_nano::load_status::set(
            k_nano::load_status::AssetKind::Piper,
            k_nano::load_status::LoadStatus::Absent,
        );
        k_nano::slog_jarbas!("Audio", "piper", "Piper TTS ausente no FAT — formant synth ativo");
    }
    None
}

/// WHPX: `-device loader,file=PIPER_PT_BR_CADU_MEDIUM.bitnet,addr=0x124200000` + `-m 6G`.
/// Usa PHYS_MEM_OFFSET (igual BitNet @0x100000000) — VA = PA + offset.
fn try_load_piper_from_loader() -> Option<PiperEngine> {
    // QEMU launch: -device loader,PIPER_PT_BR_CADU_MEDIUM.bitnet,addr=0x124200000
    const LOAD_ADDR: u64 = 0x124200000;
    // Tamanho via FAT (mesmo nome no disco) — loader sozinho nao reporta len.
    let mut size_hint: Option<usize> = None;
    unsafe {
        let ata_guard = k_nano::ATA_DRIVER.lock();
        if let Some(ref ata) = *ata_guard {
            let parts = k_nano::fat32::read_mbr(ata);
            for p in &parts {
                if p.type_code != 0x1C && p.type_code != 0x0C && p.type_code != 0x0B {
                    continue;
                }
                if let Some(fs) = k_nano::fat32::Fat32Reader::new(ata, p) {
                    for name in &["PIPER.BIN", "PIPER_EN.BIN", "PIPER_PT_BR.BIN"] {
                        if let Some(sz) = fs.lookup_file_size(name) {
                            // Host PIPER regenerado ~60MB; FAT size hint pode estar stale.
                            size_hint = Some(sz.max(62 * 1024 * 1024));
                            break;
                        }
                    }
                }
                if size_hint.is_some() {
                    break;
                }
            }
        }
    }
    let Some(sz) = size_hint else {
        return None;
    };
    if sz < 16 {
        return None;
    }
    let pm = k_nano::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
    if !k_nano::memory::is_page_present(LOAD_ADDR + pm) {
        k_nano::slog_jarbas!("Audio", "piper", "QEMU-loader @0x124200000 page absent (skip)");
        return None;
    }
    let ptr = (LOAD_ADDR + pm) as *const u8;
    let magic = unsafe { core::ptr::read_volatile(ptr as *const u32) };
    if magic != 0xBE11BE11 {
        k_nano::slog_jarbas!("Audio", "piper", "QEMU-loader @0x124200000 magic=0x{:08X} (sem Piper; FAT PIO a seguir)", magic);
        return None;
    }
    k_nano::slog_jarbas!("Audio", "piper", "QEMU-loader @0x124200000 magic OK — parse {} KB…", sz / 1024);
    let data = unsafe { core::slice::from_raw_parts(ptr, sz) };
    let mut eng = PiperEngine::new();
    if eng.load(data) {
        k_nano::slog_jarbas!("Audio", "piper", "Piper TTS LOADED (QEMU-loader @0x124200000) size={} KB", sz / 1024);
        Some(eng)
    } else {
        k_nano::slog_jarbas!("Audio", "piper", "QEMU-loader parse FAILED — fallback FAT PIO");
        None
    }
}

pub struct TtsSkill;

impl Skill for TtsSkill {
    fn manifest(&self) -> McpManifest {
        McpManifest {
            name: String::from("tts_speak"),
            description: String::from("Sintetiza texto em audio PCM (neural GPU ou formant CPU)"),
            required_tokens: Vec::new(), preconditions: Vec::new(), context_links: Vec::new(),
            output_schema: OutputSchema::Any, idempotent: false, contracts: Vec::new(),
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

        // Telemetria VAD opcional
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
