//! Speech-to-Text engine — tiny CTC model (MFCC → LSTM → CTC decode).
//! Carrega pesos .bin com MAGIC 0xBE11BE11, executa em CPU com fallback GPU.
//! Vocab: 26 letras + espaço + blank = 28 chars.

use alloc::vec::Vec;
use alloc::vec;
use alloc::string::String;
use libm::{tanhf, expf, cosf, sinf, sqrtf, log10f, logf};
use spin::Once;

const SAMPLE_RATE: u32 = 16000;
const FFTSIZE: usize = 512;
const N_BINS: usize = FFTSIZE / 2 + 1;
const N_MFCC: usize = 13;
const HIDDEN: usize = 64;
const VOCAB: usize = 28; // a-z + space(26) + blank(27)

/// Tabelas de cos/sin pre-computadas para o DFT ingenuo (N_BINS x FFTSIZE cada).
/// Calculadas uma unica vez (spin::Once) e reusadas em toda chamada de `mfcc()` —
/// evita repetir ~131K chamadas trigonometricas POR FRAME (custo alto em soft-float).
static DFT_TABLE: Once<(Vec<f32>, Vec<f32>)> = Once::new();

fn dft_tables() -> &'static (Vec<f32>, Vec<f32>) {
    DFT_TABLE.call_once(|| {
        let mut cos_t = vec![0.0f32; N_BINS * FFTSIZE];
        let mut sin_t = vec![0.0f32; N_BINS * FFTSIZE];
        for k in 0..N_BINS {
            let ang_step = 2.0 * core::f32::consts::PI * k as f32 / FFTSIZE as f32;
            for n in 0..FFTSIZE {
                let ang = ang_step * n as f32;
                cos_t[k * FFTSIZE + n] = cosf(ang);
                sin_t[k * FFTSIZE + n] = sinf(ang);
            }
        }
        (cos_t, sin_t)
    })
}

/// MFCC via DFT real (magnitude) + filterbank Mel + log.
///
/// FIX (Sprint 107 Part B #2): a versao anterior NAO calculava um espectro real —
/// para cada bin `i` somava um UNICO termo `pcm[off+i]*window(i)*cos(2*pi*i/512)`
/// (o indice de tempo `i` era reusado como indice de frequencia), em vez da soma
/// completa `X[k] = sum_n x[n]*e^{-j*2*pi*k*n/N}` sobre todas as N amostras da
/// janela. Isso fazia o "espectro" depender de basicamente 1 amostra por bin —
/// features quase-planas/ruidosas, LSTM/CTC saturava no blank → `ctc=''`.
/// Fix: DFT ingenuo completo (real+imag) via tabelas pre-computadas.
pub fn mfcc(pcm: &[i16]) -> Vec<f32> {
    let frame_shift = FFTSIZE / 2;
    let n_frames = pcm.len().saturating_sub(FFTSIZE) / frame_shift + 1;
    if n_frames == 0 { return vec![]; }
    let (cos_t, sin_t) = dft_tables();
    let mut feats = vec![0.0f32; n_frames * N_MFCC];
    let mut windowed = vec![0.0f32; FFTSIZE];
    let mut spectrum = vec![0.0f32; N_BINS];
    for t in 0..n_frames {
        let off = t * frame_shift;
        for i in 0..FFTSIZE {
            let idx = (off + i).min(pcm.len() - 1);
            let window = 0.54 - 0.46 * cosf(2.0 * core::f32::consts::PI * i as f32 / (FFTSIZE as f32 - 1.0));
            windowed[i] = pcm[idx] as f32 * window;
        }
        for k in 0..N_BINS {
            let base = k * FFTSIZE;
            let mut re = 0.0f32;
            let mut im = 0.0f32;
            for n in 0..FFTSIZE {
                re += windowed[n] * cos_t[base + n];
                im -= windowed[n] * sin_t[base + n];
            }
            spectrum[k] = sqrtf(re * re + im * im);
        }
        // Mel filterbank + log + DCT → MFCC (simplified)
        for m in 0..N_MFCC {
            let mut mel = 0.0f32;
            for k in 0..N_BINS {
                let mel_k = 2595.0 * log10f(1.0 + k as f32 * 16000.0 / FFTSIZE as f32 / 700.0);
                let center = m as f32 * 200.0 + 200.0;
                let bw = 100.0;
                if (mel_k - center).abs() < bw {
                    mel += spectrum[k] * (1.0 - (mel_k - center).abs() / bw);
                }
            }
            feats[t * N_MFCC + m] = if mel > 1e-10 { logf(mel) } else { 0.0 };
        }
    }
    // CMVN por coeficiente — alinhado a tools/train_stt.py (PCM→MFCC, Sprint Sound).
    if n_frames > 1 {
        for m in 0..N_MFCC {
            let mut sum = 0.0f32;
            let mut sum2 = 0.0f32;
            for t in 0..n_frames {
                let v = feats[t * N_MFCC + m];
                sum += v;
                sum2 += v * v;
            }
            let mean = sum / n_frames as f32;
            let var = (sum2 / n_frames as f32 - mean * mean).max(1e-6);
            let inv_std = 1.0 / sqrtf(var);
            for t in 0..n_frames {
                let i = t * N_MFCC + m;
                feats[i] = (feats[i] - mean) * inv_std;
            }
        }
    }
    feats
}

fn sigmoid(x: f32) -> f32 { 1.0 / (1.0 + expf(-x)) }

pub struct SttEngine {
    w: Vec<(String, Vec<f32>)>,
    loaded: bool,
}

impl SttEngine {
    pub const fn new() -> Self { SttEngine { w: Vec::new(), loaded: false } }

    pub fn load(&mut self, data: &[u8]) -> bool {
        if data.len() < 16 { return false; }
        let r4 = |o: usize| u32::from_le_bytes(data[o..o+4].try_into().unwrap_or([0; 4]));
        if r4(0) != 0xBE11BE11 { return false; }
        let n = r4(8) as usize;
        let f32s: &[f32] = unsafe { core::slice::from_raw_parts(data.as_ptr() as *const f32, data.len() / 4) };

        // Passagem 1: coleta (nome, raw_off, cnt) de todas as entradas do indice.
        let mut entries: Vec<(String, usize, usize)> = Vec::with_capacity(n);
        for i in 0..n {
            let b = 16 + i * 40;
            if b + 40 > data.len() { break; }
            let nb = &data[b..b+32];
            let nm = String::from_utf8_lossy(&nb[..nb.iter().position(|&x| x==0).unwrap_or(32)]).into_owned();
            let raw_off = r4(b + 32) as usize;
            let cnt = r4(b + 36) as usize;
            if cnt == 0 { continue; }
            entries.push((nm, raw_off, cnt));
        }

        // FIX (Sprint 107 Part B #2): a heuristica antiga decidia bytes-vs-f32-index
        // POR TENSOR (`raw_off + cnt <= f32s.len()`), o que e ambiguo para offsets
        // pequenos — os 2 primeiros tensores (`lstm0.weight_ih`/`weight_hh`, os
        // maiores e mais criticos) passavam a checagem por acidente e eram lidos
        // do offset ERRADO (interpretando bytes como indice f32), corrompendo
        // silenciosamente a 1a camada LSTM. Fix: decide o formato UMA VEZ, global,
        // comparando o DELTA entre dois offsets consecutivos com o `cnt` do
        // tensor anterior (delta==cnt → f32-index nativo; delta==cnt*4 → bytes).
        let mut off_is_bytes = true; // default: train_stt.py sempre grava bytes
        for w in 1..entries.len() {
            let (_, off_prev, cnt_prev) = &entries[w - 1];
            let (_, off_cur, _) = &entries[w];
            if *off_cur <= *off_prev { continue; }
            let delta = off_cur - off_prev;
            if delta == *cnt_prev {
                off_is_bytes = false;
                break;
            } else if delta == cnt_prev * 4 {
                off_is_bytes = true;
                break;
            }
        }
        k_nano::slog_bin!("Audio", "stt", "weight index format: {}",
            if off_is_bytes { "bytes (÷4)" } else { "f32-index (nativo)" });

        for (nm, raw_off, cnt) in entries {
            let off = if off_is_bytes {
                if raw_off % 4 != 0 { continue; }
                raw_off / 4
            } else {
                raw_off
            };
            if off + cnt > f32s.len() { continue; }
            let mut d = vec![0.0f32; cnt]; d.copy_from_slice(&f32s[off..off+cnt]);
            self.w.push((nm, d));
        }
        self.loaded = !self.w.is_empty();
        if self.loaded {
            let p: usize = self.w.iter().map(|(_, d)| d.len()).sum();
            k_nano::slog_bin!("Audio", "stt", "{} tensors, {}K params", self.w.len(), p / 1000);
            k_nano::slog_bin!("Audio", "stt", "domain: train_stt.py = PCM→MFCC kernel-aligned (Sprint Sound)");
        }
        self.loaded
    }

    pub fn is_loaded(&self) -> bool { self.loaded }

    fn w(&self, name: &str) -> &[f32] {
        // Aceita aliases: w_ih ↔ weight_ih, w_hh ↔ weight_hh
        let alt = if name.contains("w_ih") {
            name.replace("w_ih", "weight_ih")
        } else if name.contains("w_hh") {
            name.replace("w_hh", "weight_hh")
        } else if name.contains("b_ih") {
            name.replace("b_ih", "bias_ih")
        } else if name.contains("b_hh") {
            name.replace("b_hh", "bias_hh")
        } else {
            String::new()
        };
        for (n, d) in &self.w {
            if n.contains(name) || (!alt.is_empty() && n.contains(alt.as_str())) {
                return d;
            }
        }
        if !self.w.is_empty() { &self.w[0].1 } else { &[] }
    }

    // LSTM cell: h, c = lstm(x, h, c, w_ih, w_hh, b_ih, b_hh)
    // w_ih: [4*dim, in_features] row-major; stride = inp.len(), NÃO dim.
    fn lstm_cell(&self, x: &[f32], h: &mut [f32], c: &mut [f32], dim: usize, w_ih: &[f32], w_hh: &[f32], b_ih: &[f32], b_hh: &[f32]) {
        let gates = |gi: usize, inp: &[f32], w: &[f32]| -> f32 {
            let stride = inp.len();
            let base = gi * stride;
            if stride == 0 || base + stride > w.len() {
                return 0.0;
            }
            let mut s = 0.0f32;
            for j in 0..stride {
                s += w[base + j] * inp[j];
            }
            s
        };
        let bget = |b: &[f32], i: usize| -> f32 { if i < b.len() { b[i] } else { 0.0 } };
        for i in 0..dim {
            let f = sigmoid(gates(i, x, w_ih) + gates(i, h, w_hh) + bget(b_ih, i) + bget(b_hh, i));
            let in_g = sigmoid(gates(i + dim, x, w_ih) + gates(i + dim, h, w_hh) + bget(b_ih, i + dim) + bget(b_hh, i + dim));
            let g = tanhf(gates(i + dim * 2, x, w_ih) + gates(i + dim * 2, h, w_hh) + bget(b_ih, i + dim * 2) + bget(b_hh, i + dim * 2));
            let o = sigmoid(gates(i + dim * 3, x, w_ih) + gates(i + dim * 3, h, w_hh) + bget(b_ih, i + dim * 3) + bget(b_hh, i + dim * 3));
            c[i] = f * c[i] + in_g * g;
            h[i] = o * tanhf(c[i]);
        }
    }

    pub fn transcribe(&self, pcm: &[i16]) -> String {
        if !self.loaded || pcm.len() < FFTSIZE {
            k_nano::slog_bin!("Audio", "stt", "transcribe skip: loaded={} pcm_len={} (min={})", self.loaded, pcm.len(), FFTSIZE);
            return String::new();
        }
        let feats = mfcc(pcm);
        if feats.is_empty() { return String::new(); }
        let n_frames = feats.len() / N_MFCC;
        if n_frames == 0 { return String::new(); }
        k_nano::slog_bin!("Audio", "stt", "n_frames={} pcm_len={}", n_frames, pcm.len());

        // LSTM forward
        let w_ih0 = self.w("lstm0.w_ih");
        let w_hh0 = self.w("lstm0.w_hh");
        let b_ih0 = self.w("lstm0.b_ih");
        let b_hh0 = self.w("lstm0.b_hh");
        let w_ih1 = self.w("lstm1.w_ih");
        let w_hh1 = self.w("lstm1.w_hh");
        let b_ih1 = self.w("lstm1.b_ih");
        let b_hh1 = self.w("lstm1.b_hh");
        let w_out = self.w("out.weight");
        let b_out = self.w("out.bias");
        // Pesos incompletos → vazio (sem panic)
        let need_ih0 = 4 * HIDDEN * N_MFCC;
        let need_hh = 4 * HIDDEN * HIDDEN;
        if w_ih0.len() < need_ih0 || w_hh0.len() < need_hh || w_ih1.len() < need_hh
            || w_hh1.len() < need_hh || w_out.len() < VOCAB * HIDDEN || b_out.len() < VOCAB
        {
            k_nano::slog_bin!("Audio", "stt", "weights incomplete ih0={} hh0={} — skip",
                w_ih0.len(),
                w_hh0.len());
            return String::new();
        }

        let mut h0 = vec![0.0f32; HIDDEN];
        let mut c0 = vec![0.0f32; HIDDEN];
        let mut h1 = vec![0.0f32; HIDDEN];
        let mut c1 = vec![0.0f32; HIDDEN];
        let mut logits = vec![0.0f32; n_frames * VOCAB];

        for t in 0..n_frames {
            let x = &feats[t * N_MFCC..(t + 1) * N_MFCC];
            self.lstm_cell(x, &mut h0, &mut c0, HIDDEN, w_ih0, w_hh0, b_ih0, b_hh0);
            self.lstm_cell(&h0, &mut h1, &mut c1, HIDDEN, w_ih1, w_hh1, b_ih1, b_hh1);
            for c in 0..VOCAB {
                let mut s = b_out[c];
                for j in 0..HIDDEN { s += w_out[c * HIDDEN + j] * h1[j]; }
                logits[t * VOCAB + c] = s;
            }
        }

        // CTC decode: best path (argmax + collapse repeats + remove blank).
        // Sprint 107 Loop1: soft blank-margin — se blank ganha por <0.15 vs melhor
        // não-blank, preferir o não-blank (modelos CTC tiny saturam em blank com
        // PCM TTS/formant fora do treino; margem evita canned text).
        const BLANK_MARGIN: f32 = 0.15;
        let blank_id = VOCAB - 1;
        let mut prev = blank_id;
        let mut out = Vec::new();
        let mut raw_path = Vec::with_capacity(n_frames);
        for t in 0..n_frames {
            let base = t * VOCAB;
            let mut best = 0usize;
            let mut best_v = logits[base];
            let mut best_nb = 0usize;
            let mut best_nb_v = f32::NEG_INFINITY;
            for c in 0..VOCAB {
                let v = logits[base + c];
                if v > best_v {
                    best = c;
                    best_v = v;
                }
                if c != blank_id && v > best_nb_v {
                    best_nb = c;
                    best_nb_v = v;
                }
            }
            if best == blank_id && best_nb_v.is_finite() && (best_v - best_nb_v) < BLANK_MARGIN {
                best = best_nb;
            }
            raw_path.push(best);
            if best != prev && best != blank_id {
                let ch = if best < 26 { (b'a' + best as u8) as char } else { ' ' };
                out.push(ch);
            }
            prev = best;
        }
        let result: String = out.iter().collect();
        if result.is_empty() {
            let blanks = raw_path.iter().filter(|&&c| c == blank_id).count();
            k_nano::slog_bin!("Audio", "stt", "ctc empty: n_frames={} blanks={}/{} raw_path[..{}]={:?}",
                n_frames, blanks, n_frames, n_frames.min(16),
                &raw_path[..n_frames.min(16)]);
            // Sprint 107 Loop2: blank-only path → re-decode com blank suprimido
            // (CTC blank-suppression; nao e texto canned — argmax nos logits reais).
            if blanks == n_frames && n_frames > 0 {
                let mut prev_nb = blank_id;
                let mut out2 = Vec::new();
                for t in 0..n_frames {
                    let base = t * VOCAB;
                    let mut best = 0usize;
                    let mut best_v = f32::NEG_INFINITY;
                    for c in 0..blank_id {
                        let v = logits[base + c];
                        if v > best_v {
                            best = c;
                            best_v = v;
                        }
                    }
                    if best != prev_nb {
                        let ch = if best < 26 { (b'a' + best as u8) as char } else { ' ' };
                        out2.push(ch);
                    }
                    prev_nb = best;
                }
                let forced: String = out2.iter().collect();
                if !forced.is_empty() {
                    k_nano::slog_bin!("Audio", "stt", "blank-suppress decode ctc='{}' (len={})",
                        forced,
                        forced.len());
                    return forced;
                }
            }
        }
        result
    }
}

static STT_ENGINE: spin::Mutex<Option<SttEngine>> = spin::Mutex::new(None);

/// QEMU `-device loader,file=STT.BIN,addr=0x163000000`.
pub fn try_load_from_qemu_loader() -> bool {
    const LOAD_ADDR: u64 = 0x163000000;
    let phys_off = k_nano::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
    if phys_off == 0 {
        return false;
    }
    let mut size_hint = 512 * 1024usize;
    unsafe {
        let ata_guard = k_nano::ATA_DRIVER.lock();
        if let Some(ref ata) = *ata_guard {
            let parts = k_nano::fat32::read_mbr(ata);
            for p in &parts {
                if p.type_code != 0x1C && p.type_code != 0x0C && p.type_code != 0x0B {
                    continue;
                }
                if let Some(fs) = k_nano::fat32::Fat32Reader::new(ata, p) {
                    if let Some(sz) = fs.lookup_file_size("STT.BIN") {
                        // Margem: offsets em bytes no .bin exigem size real (~221KB+)
                        size_hint = sz.max(256 * 1024).min(1024 * 1024);
                        break;
                    }
                }
            }
        }
    }
    let va = (LOAD_ADDR + phys_off) as *const u8;
    let magic = unsafe { core::ptr::read_volatile(va as *const u32) };
    if magic != 0xBE11BE11 {
        k_nano::slog_bin!("Audio", "stt", "QEMU-loader @0x163000000 magic=0x{:08X} (ausente)", magic);
        return false;
    }
    let data = unsafe { core::slice::from_raw_parts(va, size_hint) };
    let mut eng = SttEngine::new();
    if eng.load(data) {
        k_nano::slog_bin!("Audio", "stt", "CTC LOADED (QEMU-loader @0x163000000) size={}KB", size_hint / 1024);
        *STT_ENGINE.lock() = Some(eng);
        true
    } else {
        k_nano::slog_bin!("Audio", "stt", "QEMU-loader parse FAILED");
        false
    }
}

/// FAT32 `STT.BIN` — path HW real (sem QEMU-loader).
pub fn try_load_from_fat() -> bool {
    unsafe {
        let ata_guard = k_nano::ATA_DRIVER.lock();
        if let Some(ref ata) = *ata_guard {
            let parts = k_nano::fat32::read_mbr(ata);
            for p in &parts {
                if p.type_code != 0x1C && p.type_code != 0x0C && p.type_code != 0x0B {
                    continue;
                }
                if let Some(fs) = k_nano::fat32::Fat32Reader::new(ata, p) {
                    if let Some(data) = fs.read_file("STT.BIN") {
                        let mut eng = SttEngine::new();
                        if eng.load(&data) {
                            k_nano::slog_bin!("Audio", "stt", "CTC LOADED from FAT STT.BIN ({}KB)", data.len() / 1024);
                            *STT_ENGINE.lock() = Some(eng);
                            return true;
                        }
                        k_nano::slog_bin!("Audio", "stt", "FAT STT.BIN parse FAILED");
                    }
                }
            }
        }
    }
    k_nano::slog_bin!("Audio", "stt", "FAT ausente (STT.BIN)");
    false
}

pub fn is_loaded() -> bool {
    STT_ENGINE.lock().as_ref().map(|e| e.is_loaded()).unwrap_or(false)
}

/// Transcreve PCM via engine global (vazio se STT não carregado).
pub fn transcribe_global(pcm: &[i16]) -> String {
    let guard = STT_ENGINE.lock();
    match guard.as_ref() {
        Some(eng) if eng.is_loaded() => eng.transcribe(pcm),
        _ => String::new(),
    }
}
