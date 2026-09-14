//! Speech-to-Text — tiny CTC (MFCC → 2×LSTM → CTC decode).
//!
//! Carrega `.bin` com MAGIC 0xBE11BE11. Header: magic, feat_version, n_tensors, 0.
//!
//! ## Mudanças desta sessão (WS5 / SESSION_345)
//!
//! 1. **FFT radix-2** no lugar da DFT ingênua. A DFT fazia `N_BINS × FFTSIZE` =
//!    513 × 512 ≈ 263k MACs **por frame**; a FFT faz ~N/2·log2(N) ≈ 2.3k, ~100×
//!    menos. Com 48 kHz estéreo chegando (3× mais frames), era o custo dominante do
//!    pipeline e a causa do congelamento do orb durante a transcrição.
//! 2. **Job em slices**: `begin_job`/`step_job` processam um orçamento de frames por
//!    tick. Antes a transcrição inteira rodava dentro de um único `tick()`.
//! 3. **Sem texto fabricado**: o caminho "blank-suppress decode" re-decodificava com o
//!    blank suprimido e *inventava* comandos a partir de ruído. Foi removido; baixa
//!    confiança agora publica `STT_UNCERTAIN`, que é observável e honesto.
//! 4. **Vocabulário derivado do modelo** (`out.bias.len()`), não fixo em 28. Modelos
//!    antigos continuam carregando; modelos novos podem ter acentos/pontuação.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use libm::{cosf, expf, log10f, logf, sinf, sqrtf, tanhf};
use spin::Once;

/// Amostra de entrada do STT (o pipeline entrega 16 kHz mono).
pub const SAMPLE_RATE: u32 = 16000;
const FFTSIZE: usize = 512;
const N_BINS: usize = FFTSIZE / 2 + 1;
const N_MFCC: usize = 13;
const HIDDEN: usize = 64;

/// Vocabulário canônico — **contrato com `tools/train_stt.py`**. O índice do blank é
/// sempre o último (`vocab - 1`). O vocabulário efetivo vem do modelo carregado, então
/// truncar esta tabela não quebra modelos antigos.
pub const VOCAB_CHARS: &[char] = &[
    'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k', 'l', 'm', 'n', 'o', 'p', 'q', 'r', 's',
    't', 'u', 'v', 'w', 'x', 'y', 'z', ' ',
];

/// Versão da front-end exigida pelo kernel. O modelo declara a dele no header.
const FEAT_VERSION_LEGACY: u32 = 1;

static DFT_TABLE: Once<(Vec<f32>, Vec<f32>)> = Once::new();
/// Twiddles da FFT (radix-2, N=512): W_N^k = e^{-j2πk/N}.
static FFT_TWIDDLE: Once<(Vec<f32>, Vec<f32>)> = Once::new();

fn fft_twiddles() -> &'static (Vec<f32>, Vec<f32>) {
    FFT_TWIDDLE.call_once(|| {
        let mut cos_t = vec![0.0f32; FFTSIZE / 2];
        let mut sin_t = vec![0.0f32; FFTSIZE / 2];
        for k in 0..FFTSIZE / 2 {
            let ang = 2.0 * core::f32::consts::PI * k as f32 / FFTSIZE as f32;
            cos_t[k] = cosf(ang);
            sin_t[k] = sinf(ang);
        }
        (cos_t, sin_t)
    })
}

/// FFT radix-2 in-place (Cooley–Tukey, decimation-in-time, 512 pontos).
/// Substitui a DFT O(N²) que dominava o custo da transcrição.
fn fft512(re: &mut [f32; FFTSIZE], im: &mut [f32; FFTSIZE]) {
    let (cos_t, sin_t) = fft_twiddles();

    // Bit-reversal permutation.
    let mut j = 0usize;
    for i in 1..FFTSIZE {
        let mut bit = FFTSIZE >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }

    // Butterflies.
    let mut len = 2usize;
    while len <= FFTSIZE {
        let half = len / 2;
        let step = FFTSIZE / len;
        let mut start = 0usize;
        while start < FFTSIZE {
            let mut k = 0usize;
            let mut i = start;
            while i < start + half {
                let wr = cos_t[k];
                let wi = -sin_t[k];
                let xr = re[i + half];
                let xi = im[i + half];
                let tr = xr * wr - xi * wi;
                let ti = xr * wi + xi * wr;
                re[i + half] = re[i] - tr;
                im[i + half] = im[i] - ti;
                re[i] += tr;
                im[i] += ti;
                k += step;
                i += 1;
            }
            start += len;
        }
        len <<= 1;
    }
}

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

/// MFCC alinhado ao treino (janela Hamming → magnitude → mel triangles → log → CMVN).
///
/// A magnitude do espectro vem agora da FFT radix-2. O filterbank/log/CMVN ficam
/// IDÊNTICOS aos de `tools/train_stt.py` para não invalidar o `STT.BIN` existente.
pub fn mfcc(pcm: &[i16]) -> Vec<f32> {
    mfcc_from(pcm, 0)
}

/// `from_frame`: processa apenas os frames a partir do índice dado (job em slices).
pub fn mfcc_from(pcm: &[i16], from_frame: usize) -> Vec<f32> {
    let frame_shift = FFTSIZE / 2;
    let n_frames_total = pcm.len().saturating_sub(FFTSIZE) / frame_shift + 1;
    if n_frames_total == 0 || from_frame >= n_frames_total {
        return vec![];
    }
    let n_frames = n_frames_total - from_frame;
    let mut feats = vec![0.0f32; n_frames * N_MFCC];
    let mut re = [0.0f32; FFTSIZE];
    let mut im = [0.0f32; FFTSIZE];
    let mut spectrum = vec![0.0f32; N_BINS];

    for t in 0..n_frames {
        let off = (from_frame + t) * frame_shift;
        for i in 0..FFTSIZE {
            let idx = (off + i).min(pcm.len() - 1);
            let window =
                0.54 - 0.46 * cosf(2.0 * core::f32::consts::PI * i as f32 / (FFTSIZE as f32 - 1.0));
            re[i] = pcm[idx] as f32 * window;
            im[i] = 0.0;
        }
        fft512(&mut re, &mut im);
        for k in 0..N_BINS {
            spectrum[k] = sqrtf(re[k] * re[k] + im[k] * im[k]);
        }
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
    feats
}

/// CMVN por coeficiente sobre a matriz completa (aplicada quando o job termina a
/// fase de features, igual ao treino).
fn cmvn(feats: &mut [f32], n_frames: usize) {
    if n_frames <= 1 {
        return;
    }
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

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + expf(-x))
}

/// Resultado da transcrição, com confiança observável.
pub struct Transcript {
    pub text: String,
    /// Probabilidade média do caminho vencedor (softmax local, logits normalizados).
    pub confidence: f32,
    pub blank_ratio: f32,
}

pub struct SttEngine {
    w: Vec<(String, Vec<f32>)>,
    loaded: bool,
    /// Vocabulário efetivo do modelo (out.bias.len()); blank = vocab-1.
    vocab: usize,
    /// Versão da front-end declarada no header do `.bin`.
    feat_version: u32,
}

impl SttEngine {
    pub const fn new() -> Self {
        SttEngine {
            w: Vec::new(),
            loaded: false,
            vocab: 0,
            feat_version: 0,
        }
    }

    pub fn load(&mut self, data: &[u8]) -> bool {
        if data.len() < 16 {
            return false;
        }
        let r4 = |o: usize| u32::from_le_bytes(data[o..o + 4].try_into().unwrap_or([0; 4]));
        if r4(0) != 0xBE11BE11 {
            return false;
        }
        self.feat_version = r4(4);
        let n = r4(8) as usize;
        let f32s: &[f32] =
            unsafe { core::slice::from_raw_parts(data.as_ptr() as *const f32, data.len() / 4) };

        // Passagem 1: coleta (nome, raw_off, cnt) de todas as entradas do índice.
        let mut entries: Vec<(String, usize, usize)> = Vec::with_capacity(n);
        for i in 0..n {
            let b = 16 + i * 40;
            if b + 40 > data.len() {
                break;
            }
            let nb = &data[b..b + 32];
            let nm =
                String::from_utf8_lossy(&nb[..nb.iter().position(|&x| x == 0).unwrap_or(32)])
                    .into_owned();
            let raw_off = r4(b + 32) as usize;
            let cnt = r4(b + 36) as usize;
            if cnt == 0 {
                continue;
            }
            entries.push((nm, raw_off, cnt));
        }

        // FIX (Sprint 107 Part B #2): decide bytes-vs-f32-index UMA VEZ, global, pelo
        // delta entre offsets consecutivos — a heurística por tensor corrompia
        // silenciosamente os dois primeiros (maiores) tensores da LSTM.
        let mut off_is_bytes = true;
        for w in 1..entries.len() {
            let (_, off_prev, cnt_prev) = &entries[w - 1];
            let (_, off_cur, _) = &entries[w];
            if *off_cur <= *off_prev {
                continue;
            }
            let delta = off_cur - off_prev;
            if delta == *cnt_prev {
                off_is_bytes = false;
                break;
            } else if delta == cnt_prev * 4 {
                off_is_bytes = true;
                break;
            }
        }

        for (nm, raw_off, cnt) in entries {
            let off = if off_is_bytes {
                if raw_off % 4 != 0 {
                    continue;
                }
                raw_off / 4
            } else {
                raw_off
            };
            if off + cnt > f32s.len() {
                continue;
            }
            let mut d = vec![0.0f32; cnt];
            d.copy_from_slice(&f32s[off..off + cnt]);
            self.w.push((nm, d));
        }

        // Vocabulário DERIVADO do modelo (não hardcodado em 28).
        let out_bias = self.w("out.bias").len();
        self.vocab = out_bias.max(2);
        let declared = self.feat_version;
        if declared != 0 && declared != FEAT_VERSION_LEGACY {
            k_nano::slog_bin!(
                "Audio",
                "warn",
                "STT.BIN feat_version={} != kernel={} — regenere com tools/train_stt.py",
                declared,
                FEAT_VERSION_LEGACY
            );
        }

        self.loaded = !self.w.is_empty();
        if self.loaded {
            let p: usize = self.w.iter().map(|(_, d)| d.len()).sum();
            k_nano::slog_bin!(
                "Audio",
                "stt",
                "{} tensors, {}K params, vocab={} (blank={})",
                self.w.len(),
                p / 1000,
                self.vocab,
                self.vocab - 1
            );
        }
        self.loaded
    }

    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    pub fn vocab(&self) -> usize {
        self.vocab
    }

    fn w(&self, name: &str) -> &[f32] {
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
        if !self.w.is_empty() {
            &self.w[0].1
        } else {
            &[]
        }
    }

    // LSTM cell: w_ih: [4*dim, in_features] row-major; stride = inp.len().
    fn lstm_cell(
        &self,
        x: &[f32],
        h: &mut [f32],
        c: &mut [f32],
        dim: usize,
        w_ih: &[f32],
        w_hh: &[f32],
        b_ih: &[f32],
        b_hh: &[f32],
    ) {
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
        let bget = |b: &[f32], i: usize| -> f32 {
            if i < b.len() {
                b[i]
            } else {
                0.0
            }
        };
        for i in 0..dim {
            let f = sigmoid(
                gates(i, x, w_ih) + gates(i, h, w_hh) + bget(b_ih, i) + bget(b_hh, i),
            );
            let in_g = sigmoid(
                gates(i + dim, x, w_ih)
                    + gates(i + dim, h, w_hh)
                    + bget(b_ih, i + dim)
                    + bget(b_hh, i + dim),
            );
            let g = tanhf(
                gates(i + dim * 2, x, w_ih)
                    + gates(i + dim * 2, h, w_hh)
                    + bget(b_ih, i + dim * 2)
                    + bget(b_hh, i + dim * 2),
            );
            let o = sigmoid(
                gates(i + dim * 3, x, w_ih)
                    + gates(i + dim * 3, h, w_hh)
                    + bget(b_ih, i + dim * 3)
                    + bget(b_hh, i + dim * 3),
            );
            c[i] = f * c[i] + in_g * g;
            h[i] = o * tanhf(c[i]);
        }
    }

    /// Normaliza um frame de logits em probabilidades (softmax local).
    fn softmax_frame(&self, logits: &[f32]) -> f32 {
        let vocab = self.vocab.min(logits.len());
        if vocab == 0 {
            return 0.0;
        }
        let mut max = f32::NEG_INFINITY;
        for v in &logits[..vocab] {
            if *v > max {
                max = *v;
            }
        }
        let mut sum = 0.0f32;
        let mut best = 0.0f32;
        for v in &logits[..vocab] {
            let p = expf(v - max);
            sum += p;
            if p > best {
                best = p;
            }
        }
        if sum > 0.0 {
            best / sum
        } else {
            0.0
        }
    }

    /// CTC greedy decode. **Sem caminho que invente texto**: se o caminho é todo
    /// blank, o resultado é vazio e a confiança baixa — o chamador decide (e o
    /// operador vê `STT_UNCERTAIN`).
    fn decode(&self, logits: &[f32], n_frames: usize) -> Transcript {
        let vocab = self.vocab;
        let blank_id = vocab - 1;
        let mut prev = blank_id;
        let mut out: Vec<char> = Vec::new();
        let mut conf_sum = 0.0f32;
        let mut blanks = 0usize;

        for t in 0..n_frames {
            let base = t * vocab;
            if base + vocab > logits.len() {
                break;
            }
            let frame = &logits[base..base + vocab];
            let mut best = 0usize;
            let mut best_v = f32::NEG_INFINITY;
            for (c, v) in frame.iter().enumerate() {
                if *v > best_v {
                    best_v = *v;
                    best = c;
                }
            }
            conf_sum += self.softmax_frame(frame);
            if best == blank_id {
                blanks += 1;
            } else if best != prev {
                // Fora do vocabulário: ignora (nunca sintetiza caractere).
                if let Some(ch) = VOCAB_CHARS.get(best) {
                    out.push(*ch);
                }
            }
            prev = best;
        }

        let text: String = out.into_iter().collect();
        Transcript {
            text,
            confidence: if n_frames > 0 {
                conf_sum / n_frames as f32
            } else {
                0.0
            },
            blank_ratio: if n_frames > 0 {
                blanks as f32 / n_frames as f32
            } else {
                1.0
            },
        }
    }

    /// Transcreve PCM completo (uma passada). Usado por skills e pelo job.
    pub fn transcribe(&self, pcm: &[i16]) -> String {
        self.transcribe_rich(pcm).text
    }

    pub fn transcribe_rich(&self, pcm: &[i16]) -> Transcript {
        let empty = Transcript {
            text: String::new(),
            confidence: 0.0,
            blank_ratio: 1.0,
        };
        if !self.loaded || pcm.len() < FFTSIZE {
            k_nano::slog_bin!(
                "Audio",
                "stt",
                "transcribe skip: loaded={} pcm_len={} (min={})",
                self.loaded,
                pcm.len(),
                FFTSIZE
            );
            return empty;
        }
        let mut feats = mfcc(pcm);
        if feats.is_empty() {
            return empty;
        }
        let n_frames = feats.len() / N_MFCC;
        cmvn(&mut feats, n_frames);
        apply_lstm(self, &feats, n_frames)
    }
}

/// Roda a pilha LSTM + CTC sobre features já normalizadas.
fn apply_lstm(eng: &SttEngine, feats: &[f32], n_frames: usize) -> Transcript {
    let vocab = eng.vocab;
    let empty = Transcript {
        text: String::new(),
        confidence: 0.0,
        blank_ratio: 1.0,
    };
    let w_ih0 = eng.w("lstm0.w_ih");
    let w_hh0 = eng.w("lstm0.w_hh");
    let b_ih0 = eng.w("lstm0.b_ih");
    let b_hh0 = eng.w("lstm0.b_hh");
    let w_ih1 = eng.w("lstm1.w_ih");
    let w_hh1 = eng.w("lstm1.w_hh");
    let b_ih1 = eng.w("lstm1.b_ih");
    let b_hh1 = eng.w("lstm1.b_hh");
    let w_out = eng.w("out.weight");
    let b_out = eng.w("out.bias");
    let need_ih0 = 4 * HIDDEN * N_MFCC;
    let need_hh = 4 * HIDDEN * HIDDEN;
    if w_ih0.len() < need_ih0
        || w_hh0.len() < need_hh
        || w_ih1.len() < need_hh
        || w_hh1.len() < need_hh
        || w_out.len() < vocab * HIDDEN
        || b_out.len() < vocab
    {
        k_nano::slog_bin!(
            "Audio",
            "stt",
            "weights incomplete ih0={} hh0={} vocab={} — skip",
            w_ih0.len(),
            w_hh0.len(),
            vocab
        );
        return empty;
    }

    let mut h0 = vec![0.0f32; HIDDEN];
    let mut c0 = vec![0.0f32; HIDDEN];
    let mut h1 = vec![0.0f32; HIDDEN];
    let mut c1 = vec![0.0f32; HIDDEN];
    let mut logits = vec![0.0f32; n_frames * vocab];

    for t in 0..n_frames {
        let x = &feats[t * N_MFCC..(t + 1) * N_MFCC];
        eng.lstm_cell(x, &mut h0, &mut c0, HIDDEN, w_ih0, w_hh0, b_ih0, b_hh0);
        eng.lstm_cell(&h0, &mut h1, &mut c1, HIDDEN, w_ih1, w_hh1, b_ih1, b_hh1);
        for c in 0..vocab {
            let mut s = b_out[c];
            for j in 0..HIDDEN {
                s += w_out[c * HIDDEN + j] * h1[j];
            }
            logits[t * vocab + c] = s;
        }
    }

    eng.decode(&logits, n_frames)
}

// ============================================================================
// Job em slices — mantém o orb/mouse vivos durante a transcrição
// ============================================================================

enum JobPhase {
    Features,
    Lstm,
}

/// Job de transcrição fatiado. `begin_job` captura o PCM, `step_job(budget)` avança
/// no máximo `budget` frames por chamada e devolve `Some(texto)` quando termina.
pub struct SttJob {
    pcm: Vec<i16>,
    feats: Vec<f32>,
    n_frames_total: usize,
    feat_cursor: usize,
    h0: Vec<f32>,
    c0: Vec<f32>,
    h1: Vec<f32>,
    c1: Vec<f32>,
    logits: Vec<f32>,
    frame: usize,
    phase: JobPhase,
    vocab: usize,
}

static STT_JOB: spin::Mutex<Option<SttJob>> = spin::Mutex::new(None);

static STT_ENGINE: spin::Mutex<Option<SttEngine>> = spin::Mutex::new(None);

/// Inicia um job. `false` se o STT não está carregado ou o áudio é curto demais.
pub fn begin_job(pcm: &[i16]) -> bool {
    // Escopo CURTO: só ler `loaded`/`vocab`. O guard cai antes do slice abaixo
    // (spin::Mutex não é reentrante — manter aqui seria deadlock no `advance`).
    let (n_frames_total, vocab) = {
        let eng_guard = STT_ENGINE.lock();
        let Some(eng) = eng_guard.as_ref() else {
            return false;
        };
        if !eng.loaded || pcm.len() < FFTSIZE {
            return false;
        }
        (
            pcm.len().saturating_sub(FFTSIZE) / (FFTSIZE / 2) + 1,
            eng.vocab,
        )
    };

    let mut job = SttJob {
        pcm: pcm.to_vec(),
        feats: vec![0.0f32; n_frames_total * N_MFCC],
        n_frames_total,
        feat_cursor: 0,
        h0: vec![0.0f32; HIDDEN],
        c0: vec![0.0f32; HIDDEN],
        h1: vec![0.0f32; HIDDEN],
        c1: vec![0.0f32; HIDDEN],
        logits: vec![0.0f32; n_frames_total * vocab],
        frame: 0,
        phase: JobPhase::Features,
        vocab,
    };
    // Primeiro slice já no `begin` (16 frames), para não atrasar o turno.
    // O engine é passado por referência: NUNCA copiar os pesos por tick (eram
    // ~216 KB de `to_vec()` a cada slice) e nunca re-lockar o engine aqui dentro
    // (spin::Mutex não é reentrante → deadlock).
    {
        let eng_guard = STT_ENGINE.lock();
        let Some(eng) = eng_guard.as_ref() else {
            return false;
        };
        let _ = job.advance(eng, 16);
    }
    *STT_JOB.lock() = Some(job);
    true
}

pub fn job_active() -> bool {
    STT_JOB.lock().is_some()
}

/// Avança o job em no máximo `budget` frames. Retorna `Some(texto)` ao terminar.
///
/// Ordem de lock fixa: **STT_ENGINE → STT_JOB** (nunca o inverso).
pub fn step_job(budget: usize) -> Option<String> {
    let eng_guard = STT_ENGINE.lock();
    let Some(eng) = eng_guard.as_ref() else {
        return None;
    };
    let mut guard = STT_JOB.lock();
    let Some(job) = guard.as_mut() else {
        return None;
    };
    if let Some(text) = job.advance(eng, budget) {
        *guard = None;
        return Some(text);
    }
    None
}

impl SttJob {
    /// Avança o job; devolve o texto quando a decodificação termina.
    /// `eng` vem por referência — zero cópia de pesos por slice.
    fn advance(&mut self, eng: &SttEngine, budget: usize) -> Option<String> {
        match self.phase {
            JobPhase::Features => {
                // Processa `budget` frames de MFCC (via FFT radix-2).
                let start = self.feat_cursor;
                let end = (start + budget).min(self.n_frames_total);
                if end > start {
                    let slice_pcm_start = start * (FFTSIZE / 2);
                    let partial = mfcc_from(&self.pcm[slice_pcm_start..], 0);
                    let produced = (end - start).min(partial.len() / N_MFCC);
                    for t in 0..produced {
                        let dst = (start + t) * N_MFCC;
                        let src = t * N_MFCC;
                        self.feats[dst..dst + N_MFCC]
                            .copy_from_slice(&partial[src..src + N_MFCC]);
                    }
                    self.feat_cursor = end;
                }
                if self.feat_cursor >= self.n_frames_total {
                    cmvn(&mut self.feats, self.n_frames_total);
                    self.phase = JobPhase::Lstm;
                } else {
                    return None;
                }
            }
            JobPhase::Lstm => {}
        }

        // Fase LSTM/CTC. Pesos por REFERÊNCIA (sem `to_vec()` por tick) e um
        // scratch de stack para alimentar a 2ª camada sem `.clone()` por frame
        // (eram 64 alocações por tick no caminho de áudio).
        let w_ih0 = eng.w("lstm0.w_ih");
        let w_hh0 = eng.w("lstm0.w_hh");
        let b_ih0 = eng.w("lstm0.b_ih");
        let b_hh0 = eng.w("lstm0.b_hh");
        let w_ih1 = eng.w("lstm1.w_ih");
        let w_hh1 = eng.w("lstm1.w_hh");
        let b_ih1 = eng.w("lstm1.b_ih");
        let b_hh1 = eng.w("lstm1.b_hh");
        let w_out = eng.w("out.weight");
        let b_out = eng.w("out.bias");

        let mut h0_scratch = [0.0f32; HIDDEN];
        let end = (self.frame + budget).min(self.n_frames_total);
        for t in self.frame..end {
            let x = &self.feats[t * N_MFCC..(t + 1) * N_MFCC];
            tmp_lstm(x, &mut self.h0, &mut self.c0, w_ih0, w_hh0, b_ih0, b_hh0);
            h0_scratch.copy_from_slice(&self.h0);
            tmp_lstm(&h0_scratch, &mut self.h1, &mut self.c1, w_ih1, w_hh1, b_ih1, b_hh1);
            for c in 0..self.vocab {
                let mut s = *b_out.get(c).unwrap_or(&0.0);
                let base = c * HIDDEN;
                for j in 0..HIDDEN {
                    if base + j < w_out.len() {
                        s += w_out[base + j] * self.h1[j];
                    }
                }
                self.logits[t * self.vocab + c] = s;
            }
        }
        self.frame = end;
        if self.frame < self.n_frames_total {
            return None;
        }

        // Fim: decodifica e reporta honestamente a confiança.
        let text = decode_logits(
            &self.logits,
            self.n_frames_total,
            self.vocab,
        );
        if text.confidence < 0.55 || text.blank_ratio > 0.92 {
            let _ = k_nano::EVENT_BUS.publish(event_bus::Event {
                id: 0,
                topic: alloc::string::String::from("STT_UNCERTAIN"),
                payload: alloc::format!(
                    "conf={:.2} blank={:.2} len={}",
                    text.confidence,
                    text.blank_ratio,
                    text.text.len()
                )
                .into_bytes(),
                token: event_bus::CapabilityToken::Legacy(1),
            });
            k_nano::slog_bin!(
                "Audio",
                "warn",
                "STT incerto: conf={:.2} blank={:.2} text_len={}",
                text.confidence,
                text.blank_ratio,
                text.text.len()
            );
        }
        Some(text.text)
    }
}

fn decode_logits(logits: &[f32], n_frames: usize, vocab: usize) -> Transcript {
    let blank_id = vocab - 1;
    let mut prev = blank_id;
    let mut out: Vec<char> = Vec::new();
    let mut conf_sum = 0.0f32;
    let mut blanks = 0usize;
    for t in 0..n_frames {
        let base = t * vocab;
        if base + vocab > logits.len() {
            break;
        }
        let frame = &logits[base..base + vocab];
        let mut best = 0usize;
        let mut best_v = f32::NEG_INFINITY;
        let mut max = f32::NEG_INFINITY;
        for v in frame {
            if *v > max {
                max = *v;
            }
        }
        let mut sum = 0.0f32;
        let mut best_p = 0.0f32;
        for (c, v) in frame.iter().enumerate() {
            let p = expf(v - max);
            sum += p;
            if *v > best_v {
                best_v = *v;
                best = c;
            }
            if c == best {
                best_p = p;
            }
        }
        if sum > 0.0 {
            let _ = best_p;
            conf_sum += best_p / sum;
        }
        if best == blank_id {
            blanks += 1;
        } else if best != prev {
            if let Some(ch) = VOCAB_CHARS.get(best) {
                out.push(*ch);
            }
        }
        prev = best;
    }
    Transcript {
        text: out.into_iter().collect(),
        confidence: if n_frames > 0 {
            conf_sum / n_frames as f32
        } else {
            0.0
        },
        blank_ratio: if n_frames > 0 {
            blanks as f32 / n_frames as f32
        } else {
            1.0
        },
    }
}

/// LSTM cell livre (sem `&self`) para uso no job fatiado.
fn tmp_lstm(
    x: &[f32],
    h: &mut [f32],
    c: &mut [f32],
    w_ih: &[f32],
    w_hh: &[f32],
    b_ih: &[f32],
    b_hh: &[f32],
) {
    let dim = h.len();
    let gate = |gi: usize, inp: &[f32], w: &[f32]| -> f32 {
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
    let bget = |b: &[f32], i: usize| if i < b.len() { b[i] } else { 0.0 };
    for i in 0..dim {
        let f = sigmoid(gate(i, x, w_ih) + gate(i, h, w_hh) + bget(b_ih, i) + bget(b_hh, i));
        let ig = sigmoid(
            gate(i + dim, x, w_ih)
                + gate(i + dim, h, w_hh)
                + bget(b_ih, i + dim)
                + bget(b_hh, i + dim),
        );
        let g = tanhf(
            gate(i + dim * 2, x, w_ih)
                + gate(i + dim * 2, h, w_hh)
                + bget(b_ih, i + dim * 2)
                + bget(b_hh, i + dim * 2),
        );
        let o = sigmoid(
            gate(i + dim * 3, x, w_ih)
                + gate(i + dim * 3, h, w_hh)
                + bget(b_ih, i + dim * 3)
                + bget(b_hh, i + dim * 3),
        );
        c[i] = f * c[i] + ig * g;
        h[i] = o * tanhf(c[i]);
    }
}

// ============================================================================
// Carregamento (QEMU loader + FAT)
// ============================================================================

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
                        size_hint = sz.max(256 * 1024).min(1024 * 1024);
                        break;
                    }
                }
            }
        }
    }
    if !k_nano::memory::is_page_present(LOAD_ADDR + phys_off) {
        k_nano::slog_bin!("Audio", "stt", "QEMU-loader @0x163000000 page absent (skip)");
        return false;
    }
    let va = (LOAD_ADDR + phys_off) as *const u8;
    let magic = unsafe { core::ptr::read_volatile(va as *const u32) };
    if magic != 0xBE11BE11 {
        k_nano::slog_bin!(
            "Audio",
            "stt",
            "QEMU-loader @0x163000000 magic=0x{:08X} (ausente)",
            magic
        );
        return false;
    }
    let data = unsafe { core::slice::from_raw_parts(va, size_hint) };
    let mut eng = SttEngine::new();
    if eng.load(data) {
        k_nano::slog_bin!(
            "Audio",
            "stt",
            "CTC LOADED (QEMU-loader @0x163000000) size={}KB",
            size_hint / 1024
        );
        *STT_ENGINE.lock() = Some(eng);
        true
    } else {
        k_nano::slog_bin!("Audio", "stt", "QEMU-loader parse FAILED");
        false
    }
}

/// FAT32 `STT.BIN` — path HW real (sem QEMU-loader).
pub fn try_load_from_fat() -> bool {
    if k_nano::platform_probe::hypervisor().is_sandbox() {
        k_nano::slog_bin!("Audio", "stt", "skip FAT STT.BIN PIO no hypervisor");
        return false;
    }
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
                        if sz > 2 * 1024 * 1024 {
                            k_nano::slog_bin!(
                                "Audio",
                                "stt",
                                "skip STT.BIN {}KB (cap 2MB)",
                                sz / 1024
                            );
                            return false;
                        }
                    }
                    if let Some(data) = fs.read_file("STT.BIN") {
                        let mut eng = SttEngine::new();
                        if eng.load(&data) {
                            k_nano::slog_bin!(
                                "Audio",
                                "stt",
                                "CTC LOADED from FAT STT.BIN ({}KB)",
                                data.len() / 1024
                            );
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
    STT_ENGINE
        .lock()
        .as_ref()
        .map(|e| e.is_loaded())
        .unwrap_or(false)
}

pub fn vocab() -> usize {
    STT_ENGINE
        .lock()
        .as_ref()
        .map(|e| e.vocab())
        .unwrap_or(0)
}

/// Transcreve PCM via engine global (vazio se STT não carregado).
pub fn transcribe_global(pcm: &[i16]) -> String {
    let guard = STT_ENGINE.lock();
    match guard.as_ref() {
        Some(eng) if eng.is_loaded() => eng.transcribe(pcm),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A FFT deve bater com a DFT direta (mesma magnitude) — é o que garante que a
    /// troca do front-end não mudou a feature.
    #[test]
    fn fft_matches_naive_dft() {
        let n = 64usize; // sub-conjunto para manter o teste host rápido
        let mut signal = [0.0f32; FFTSIZE];
        for i in 0..FFTSIZE {
            signal[i] = ((i * 7 % 31) as f32 - 15.0) * 0.1;
        }
        // FFT completa
        let mut re = signal;
        let mut im = [0.0f32; FFTSIZE];
        fft512(&mut re, &mut im);
        // DFT direta em alguns bins
        let (cos_t, sin_t) = fft_twiddles();
        let _ = n;
        for k in [0usize, 1, 5, 17, 100, 255] {
            let mut dr = 0.0f32;
            let mut di = 0.0f32;
            for t in 0..FFTSIZE {
                // W_N^{k*t} = twiddle[(k*t) % 512]
                let idx = (k * t) % (FFTSIZE / 2);
                let sign = if ((k * t) / (FFTSIZE / 2)) % 2 == 0 { 1.0 } else { -1.0 };
                dr += signal[t] * cos_t[idx] * sign;
                di -= signal[t] * sin_t[idx] * sign;
            }
            let mag_fft = sqrtf(re[k] * re[k] + im[k] * im[k]);
            let mag_dft = sqrtf(dr * dr + di * di);
            assert!(
                (mag_fft - mag_dft).abs() < 0.5,
                "bin {k}: fft={mag_fft} dft={mag_dft}"
            );
        }
    }

    /// Nenhum caminho pode produzir texto a partir de logits degenerados — o decoder
    /// honesto devolve vazio em vez de inventar palavras.
    #[test]
    fn blank_only_path_yields_no_text() {
        let vocab = 28usize;
        let n_frames = 40usize;
        let mut logits = vec![-10.0f32; n_frames * vocab];
        for t in 0..n_frames {
            logits[t * vocab + (vocab - 1)] = 10.0; // blank domina
        }
        let out = decode_logits(&logits, n_frames, vocab);
        assert!(out.text.is_empty(), "não deve fabricar texto: {:?}", out.text);
        assert!(out.blank_ratio > 0.99);
    }

    /// Vocabulário é derivado do modelo, não fixo em 28.
    #[test]
    fn vocab_table_is_canonical_prefix() {
        assert_eq!(VOCAB_CHARS.len(), 27);
        assert_eq!(VOCAB_CHARS[0], 'a');
        assert_eq!(VOCAB_CHARS[25], 'z');
        assert_eq!(VOCAB_CHARS[26], ' ');
    }
}
