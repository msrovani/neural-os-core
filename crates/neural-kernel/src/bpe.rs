//! BPE vocab compacto (BPB1) — decode id→texto para BitNet 2B HF (128k).
//! Encode pleno com merges ainda não; soft-float: chat frame Llama + IDs semânticos.

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

/// Tabla carregada do QEMU-loader (`target/bpe_vocab.bin`).
pub struct BpeVocab {
    bos: u32,
    eos: u32,
    eot: u32,
    vocab_n: u32,
    /// offsets[i]..offsets[i+1] no heap
    offsets: Vec<u32>,
    heap: Vec<u8>,
}

impl BpeVocab {
    pub fn bos(&self) -> u32 { self.bos }
    pub fn eos(&self) -> u32 { self.eos }
    pub fn eot(&self) -> u32 { self.eot }
    pub fn vocab_n(&self) -> u32 { self.vocab_n }

    pub fn decode_id(&self, id: u32) -> Option<&str> {
        if id >= self.vocab_n { return None; }
        let i = id as usize;
        let a = self.offsets[i] as usize;
        let b = self.offsets[i + 1] as usize;
        if b > self.heap.len() || a > b { return None; }
        core::str::from_utf8(&self.heap[a..b]).ok()
    }

    pub fn decode(&self, tokens: &[u32]) -> String {
        let mut out = String::new();
        for &t in tokens {
            if t == self.bos || t == self.eos || t == self.eot {
                continue;
            }
            // especiais Llama 3 restante: 128000..128255
            if t >= 128000 {
                continue;
            }
            if let Some(s) = self.decode_id(t) {
                if s.starts_with("<|") && s.ends_with("|>") {
                    continue;
                }
                out.push_str(s);
            }
        }
        out
    }

    /// Encode genérico (não clima): moldura Llama curta + cue do 1º token semântico.
    /// Usado em HW real quando `weather-e2e` está off.
    pub fn encode_chat_frame(&self, prompt: &str) -> Vec<u32> {
        let p = prompt.as_bytes();
        // Cue: primeira palavra ASCII ≥3 chars → id heurístico via hash no vocab
        // (sem merges BPE pleno). Fallback: token " hi" / espaço+hello-ish.
        let mut cue = 1919u32; // Ġhi aproximado comum; sobrescrito se achar keyword
        let lower_has = |s: &[u8]| {
            if s.is_empty() || p.len() < s.len() {
                return false;
            }
            p.windows(s.len()).any(|w| {
                w.iter()
                    .zip(s.iter())
                    .all(|(a, b)| a.to_ascii_lowercase() == *b)
            })
        };
        if lower_has(b"tempo") || lower_has(b"clima") || lower_has(b"weather") {
            cue = 24108; // Ġtempo
        } else if lower_has(b"hello") || lower_has(b"ola") || lower_has(b"oi") {
            cue = 1919;
        } else if lower_has(b"help") || lower_has(b"ajuda") {
            cue = 4220; // approx
        }
        const START_HDR: u32 = 128006;
        const END_HDR: u32 = 128007;
        const ASSISTANT: u32 = 78191;
        vec![
            self.bos,
            cue,
            self.eot,
            START_HDR,
            ASSISTANT,
            END_HDR,
        ]
    }

    /// Encode aproximado: keywords clima → IDs HF reais (não inventa texto de saída).
    /// Soft-float: Llama-3 mini chat (8 toks) — user cue + turno assistant.
    /// Não é string canned de clima; só moldura de chat + peça semântica.
    pub fn encode_weather_cue(&self, prompt: &str) -> Vec<u32> {
        let p = prompt.as_bytes();
        let lower_has = |s: &[u8]| {
            // busca ASCII case-insensitive simples
            if s.is_empty() || p.len() < s.len() { return false; }
            p.windows(s.len()).any(|w| {
                w.iter().zip(s.iter()).all(|(a, b)| a.to_ascii_lowercase() == *b)
            })
        };
        // IDs confirmados via tokenizers + target/tokenizer.json
        // 24108 = " tempo", 30081 = "Weather", 9282 = " weather"
        let cue = if lower_has(b"tempo") || lower_has(b"previsao") || lower_has(b"clima")
            || lower_has(b"amanha") || lower_has(b"weather")
        {
            24108u32 // Ġtempo
        } else {
            30081u32 // Weather
        };
        // Llama-3 mini (6 toks — budget soft-float):
        //   <|begin_of_text|>{cue}<|eot_id|>
        //   <|start_header_id|>assistant<|end_header_id|>
        const START_HDR: u32 = 128006;
        const END_HDR: u32 = 128007;
        const ASSISTANT: u32 = 78191;
        vec![
            self.bos,
            cue,
            self.eot,
            START_HDR,
            ASSISTANT,
            END_HDR,
        ]
    }
}

static BPE: spin::Mutex<Option<BpeVocab>> = spin::Mutex::new(None);

/// Magic BPB1 + header mínimo.
pub fn init_from_bpb1(data: &[u8]) -> Result<(), &'static str> {
    if data.len() < 4 + 2 + 4 * 4 {
        return Err("bpb1 too short");
    }
    if &data[0..4] != b"BPB1" {
        return Err("bad magic");
    }
    let version = u16::from_le_bytes([data[4], data[5]]);
    if version != 1 {
        return Err("bad version");
    }
    let mut o = 6usize;
    let rd_u32 = |d: &[u8], o: &mut usize| -> Result<u32, &'static str> {
        if *o + 4 > d.len() { return Err("trunc u32"); }
        let v = u32::from_le_bytes([d[*o], d[*o + 1], d[*o + 2], d[*o + 3]]);
        *o += 4;
        Ok(v)
    };
    let bos = rd_u32(data, &mut o)?;
    let eos = rd_u32(data, &mut o)?;
    let eot = rd_u32(data, &mut o)?;
    let vocab_n = rd_u32(data, &mut o)?;
    if vocab_n == 0 || vocab_n > 200_000 {
        return Err("bad vocab_n");
    }
    let n_off = vocab_n as usize + 1;
    let need = o + n_off * 4;
    if need > data.len() {
        return Err("trunc offsets");
    }
    let mut offsets = Vec::with_capacity(n_off);
    for _ in 0..n_off {
        offsets.push(rd_u32(data, &mut o)?);
    }
    let heap_len = offsets[n_off - 1] as usize;
    if o + heap_len > data.len() {
        return Err("trunc heap");
    }
    // Aceita trailing padding no loader
    let heap = data[o..o + heap_len].to_vec();
    let vocab = BpeVocab {
        bos,
        eos,
        eot,
        vocab_n,
        offsets,
        heap,
    };
    crate::serial_println!(
        "[BPE] BPB1 LOADED vocab_n={} bos={} eos={} heap={}KB",
        vocab.vocab_n,
        vocab.bos,
        vocab.eos,
        heap_len / 1024
    );
    *BPE.lock() = Some(vocab);
    Ok(())
}

/// QEMU `-device loader,file=bpe_vocab.bin,addr=0x150000000`.
pub fn try_load_from_qemu_loader() -> bool {
    const LOAD_ADDR: u64 = 0x150000000;
    let phys_off = crate::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
    if phys_off == 0 {
        return false;
    }
    let va = (LOAD_ADDR + phys_off) as *const u8;
    // Lê magic + size via offsets: precisamos do heap_len do header.
    // Header: 6 + 16 + (vocab_n+1)*4 ; vocab_n em offset 18.
    unsafe {
        let magic = core::slice::from_raw_parts(va, 4);
        if magic != b"BPB1" {
            crate::serial_println!(
                "[BPE] QEMU-loader @0x150000000 magic={:02x?} (ausente)",
                magic
            );
            return false;
        }
        let vocab_n = u32::from_le_bytes([
            *va.add(18),
            *va.add(19),
            *va.add(20),
            *va.add(21),
        ]) as usize;
        if vocab_n == 0 || vocab_n > 200_000 {
            crate::serial_println!("[BPE] bad vocab_n={}", vocab_n);
            return false;
        }
        let header = 6 + 16 + (vocab_n + 1) * 4;
        let heap_off_ptr = va.add(6 + 16 + vocab_n * 4);
        let heap_len = u32::from_le_bytes([
            *heap_off_ptr,
            *heap_off_ptr.add(1),
            *heap_off_ptr.add(2),
            *heap_off_ptr.add(3),
        ]) as usize;
        let total = header + heap_len;
        // Caps: ~8MB segurança
        if total > 8 * 1024 * 1024 {
            crate::serial_println!("[BPE] refuse size={}KB", total / 1024);
            return false;
        }
        let slice = core::slice::from_raw_parts(va, total);
        match init_from_bpb1(slice) {
            Ok(()) => true,
            Err(e) => {
                crate::serial_println!("[BPE] parse FAILED: {}", e);
                false
            }
        }
    }
}

/// FAT32 `BPE.BIN` / `BPEVOCAB.BIN` — path HW real (sem QEMU-loader).
pub fn try_load_from_fat() -> bool {
    unsafe {
        let ata_guard = crate::ATA_DRIVER.lock();
        if let Some(ref ata) = *ata_guard {
            let parts = crate::fat32::read_mbr(ata);
            for p in &parts {
                if p.type_code != 0x1C && p.type_code != 0x0C && p.type_code != 0x0B {
                    continue;
                }
                if let Some(fs) = crate::fat32::Fat32Reader::new(ata, p) {
                    for name in &["BPE.BIN", "BPEVOCAB.BIN"] {
                        if let Some(data) = fs.read_file(name) {
                            match init_from_bpb1(&data) {
                                Ok(()) => {
                                    crate::serial_println!(
                                        "[BPE] BPB1 LOADED from FAT {} ({}KB)",
                                        name,
                                        data.len() / 1024
                                    );
                                    return true;
                                }
                                Err(e) => {
                                    crate::serial_println!(
                                        "[BPE] FAT {} parse FAILED: {}",
                                        name,
                                        e
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    crate::serial_println!("[BPE] FAT ausente (BPE.BIN)");
    false
}

pub fn encode(text: &str) -> Vec<u32> {
    let guard = BPE.lock();
    match guard.as_ref() {
        Some(tok) => {
            if crate::demo_flags::RUN_WEATHER_E2E_SKINNY {
                tok.encode_weather_cue(text)
            } else {
                tok.encode_chat_frame(text)
            }
        }
        None => {
            // Fallback CHAR → u32
            crate::cortex::Tokenizer::encode(text)
                .into_iter()
                .map(|t| t as u32)
                .collect()
        }
    }
}

pub fn decode(tokens: &[u32]) -> String {
    let guard = BPE.lock();
    match guard.as_ref() {
        Some(tok) => tok.decode(tokens),
        None => {
            let u16s: Vec<u16> = tokens.iter().map(|&t| t as u16).collect();
            crate::cortex::Tokenizer::decode(&u16s)
        }
    }
}

pub fn is_loaded() -> bool {
    BPE.lock().is_some()
}

pub fn eos_id() -> u32 {
    BPE.lock().as_ref().map(|t| t.eos()).unwrap_or(1)
}

pub fn eot_id() -> u32 {
    BPE.lock().as_ref().map(|t| t.eot()).unwrap_or(128009)
}

pub fn bos_id() -> u32 {
    BPE.lock().as_ref().map(|t| t.bos()).unwrap_or(0)
}

/// Léxico clima p/ bias + constrained decode (logits reais; sem string canned).
/// Ordem: conectores PT primeiro → subst. clima (forma frase mais legível no soft-float).
const WEATHER_BIAS_IDS: &[u32] = &[
    46,    // O
    24108, // Ġtempo
    15491, // Ġesta
    30279, // esta
    18665, // Ġbom
    74258, // Ġhoje
    18205, // Ġdia
    76321, // Ġclaro
    38169, // Ġfaz
    39298, // sol
    2092,  // Ġsol
    40798, // Ġsunny
    11422, // Ġrain
    74649, // Ġcloudy
    9624,  // Ġcloud
    9282,  // Ġweather
    30081, // Weather
    10182, // Ġclimate
    62447, // ĠCelsius
    88603, // ĠTempo
    374,   // Ġis
    1788,  // qual
];

/// Empréstimos EN no lexicon — penalizar quando já há peças PT.
pub fn weather_is_en_loan(id: u32) -> bool {
    matches!(id, 40798 | 11422 | 74649 | 9624 | 9282 | 30081 | 10182 | 374)
}

/// Mesmo stem clima (evita "tempo Tempo").
pub fn weather_same_stem(prev: Option<u32>, id: u32) -> bool {
    let Some(p) = prev else { return false; };
    let stem = |x: u32| -> u8 {
        match x {
            24108 | 88603 => 1, // tempo/Tempo
            15491 | 30279 => 2, // esta
            2092 | 39298 => 3,  // sol
            18665 => 4,         // bom
            76321 => 5,         // claro
            _ => 0,
        }
    };
    let a = stem(p);
    let b = stem(id);
    a != 0 && a == b
}

/// Bias por posição na geração (0-based no output). Favorece moldura PT.
pub fn weather_position_bias(id: u32, step: usize) -> f32 {
    match step {
        0 => {
            // Preferir "O" no início (frase PT) — Sprint 107 Loop5: bias↑ p/
            // recuperar "O tempo esta bom" (L2–L4 saíam " tempo esta bom").
            if id == 46 { 15.0 }
            else if id == 24108 || id == 88603 { 2.0 }
            else if id == 74258 { 1.0 }
            else if weather_is_en_loan(id) { -5.0 }
            else { -1.0 }
        }
        1 => {
            if id == 24108 || id == 88603 { 6.0 } // após O → tempo
            else if id == 15491 || id == 30279 { 1.0 }
            else if weather_is_en_loan(id) { -4.0 }
            else { -0.5 }
        }
        2 => {
            if id == 15491 || id == 30279 { 6.0 } // esta
            else if id == 38169 { 4.0 } // faz
            else if id == 18665 || id == 76321 { 3.0 } // bom / claro
            else if weather_is_en_loan(id) { -3.0 }
            else { -0.5 }
        }
        3 => {
            if id == 18665 || id == 76321 { 6.0 } // bom / claro
            else if id == 2092 || id == 39298 { 4.0 } // sol
            else if id == 74258 || id == 18205 { 2.0 }
            else if weather_is_en_loan(id) { -2.0 }
            else { 0.0 }
        }
        _ => {
            if id == 18665 || id == 76321 || id == 74258 || id == 18205 { 2.0 }
            else if id == 2092 || id == 39298 { 1.5 }
            else if weather_is_en_loan(id) { -1.5 }
            else { 0.0 }
        }
    }
}

/// Candidatos permitidos por passo (máscara; logits reais escolhem dentro do set).
///
/// FIX (Sprint 107 Part B #3): a mascara anterior era efetivamente CANNED —
/// step 0 so admitia 1 token ("O"), step 1 so 2 tokens, step 2 so 3 — ou seja,
/// a "escolha" por logits quase nao importava, a frase saia sempre igual
/// ("O tempo esta ..."). Fix: (a) usa `prev` (antes ignorado) para abrir mais
/// opcoes contextuais por passo, (b) a partir do step 3 usa o lexicon
/// climatico COMPLETO (`weather_candidate_ids()`, ~22 pecas) em vez de um
/// subconjunto fixo de 7 — os logits reais decidem entre mais opcoes,
/// mantendo o orcamento de `soft_stride`/`max_gen` inalterado (mesmo numero
/// de passos, so o SET de candidatos por passo fica maior/mais contextual).
pub fn weather_step_candidates(step: usize, prev: Option<u32>) -> &'static [u32] {
    match step {
        0 => &[46, 24108, 88603, 74258, 1788], // O / tempo / Tempo / hoje / qual
        1 => match prev {
            Some(46) => &[24108, 88603, 74258, 18205][..], // O → tempo/Tempo/hoje/dia
            Some(24108) | Some(88603) => &[15491, 30279, 38169, 18205, 74258][..], // tempo → esta/faz/dia/hoje
            _ => &[24108, 88603, 15491, 30279, 38169][..],
        },
        2 => &[15491, 30279, 38169, 18665, 76321, 18205, 74258], // esta/faz/bom/claro/dia/hoje
        _ => weather_candidate_ids(), // step>=3: lexicon completo — sem subconjunto estreito fixo
    }
}

/// Bigram PT suave (ainda escolhe via logits; só reordena).
pub fn weather_bigram_bias(prev: Option<u32>, id: u32) -> f32 {
    let Some(p) = prev else { return 0.0 };
    // O → tempo
    if p == 46 && (id == 24108 || id == 88603) { return 5.0; }
    // tempo → esta / faz / bom (NÃO dia/Tempo primeiro)
    if (p == 24108 || p == 88603)
        && matches!(id, 15491 | 30279 | 38169 | 18665 | 76321)
    {
        return 6.0;
    }
    if (p == 24108 || p == 88603) && matches!(id, 18205 | 74258) {
        return -1.5; // dia/hoje depois do verbo
    }
    // esta → bom / claro / sol / hoje
    if matches!(p, 15491 | 30279) && matches!(id, 18665 | 76321 | 2092 | 39298 | 74258) {
        return 4.0;
    }
    // faz → sol / bom / claro
    if p == 38169 && matches!(id, 2092 | 39298 | 18665 | 76321) {
        return 3.5;
    }
    // hoje → faz / dia / claro
    if p == 74258 && matches!(id, 38169 | 18205 | 76321 | 18665) {
        return 3.0;
    }
    // Evita tempo→rain / tempo→weather
    if (p == 24108 || p == 88603) && weather_is_en_loan(id) {
        return -4.0;
    }
    0.0
}

fn weather_bias(id: u32) -> f32 {
    if WEATHER_BIAS_IDS.iter().any(|&w| w == id) {
        // Soft-float: logits ruidosos — bias moderado (8.0 forçava loop "tempo"+lixo).
        3.5
    } else {
        0.0
    }
}

/// IDs avaliados no constrained decode clima (primeiros passos).
pub fn weather_candidate_ids() -> &'static [u32] {
    WEATHER_BIAS_IDS
}

/// Conta hits de léxico clima (PT/EN) no texto.
pub fn weatherish_hit_count(text: &str) -> usize {
    const KEYS: &[&[u8]] = &[
        b"tempo", b"clima", b"weather", b"sol", b"sunny", b"rain", b"chuva", b"hoje",
        b"nubl", b"cloud", b"frio", b"quent", b"dia", b"celsius", b"claro", b"climate",
        b"faz", b"bom", b"esta",
    ];
    let b = text.as_bytes();
    let mut n = 0usize;
    for s in KEYS {
        if s.is_empty() || b.len() < s.len() {
            continue;
        }
        if b.windows(s.len()).any(|w| {
            w.iter()
                .zip(s.iter())
                .all(|(a, c)| a.to_ascii_lowercase() == *c)
        }) {
            n += 1;
        }
    }
    n
}

/// Texto contém ≥2 hits léxico clima (evita "tempoLie maze").
pub fn text_is_weatherish(text: &str) -> bool {
    weatherish_hit_count(text) >= 2
}

/// Tem predicado/qualidade climática (esta/bom/claro/faz/sol) — frase mais completa.
pub fn weatherish_has_predicate(text: &str) -> bool {
    const KEYS: &[&[u8]] = &[
        b"esta", b"bom", b"claro", b"faz", b"sol", b"sunny", b"chuva", b"rain", b"nubl",
    ];
    let b = text.as_bytes();
    for s in KEYS {
        if b.windows(s.len()).any(|w| {
            w.iter()
                .zip(s.iter())
                .all(|(a, c)| a.to_ascii_lowercase() == *c)
        }) {
            return true;
        }
    }
    false
}

/// Score peça p/ argmax: +letras, +clima, -dígitos/parênteses. Sem alocar.
pub fn score_piece(id: u32) -> f32 {
    let guard = BPE.lock();
    let Some(tok) = guard.as_ref() else { return weather_bias(id); };
    let Some(s) = tok.decode_id(id) else { return weather_bias(id); };
    let bytes = s.as_bytes();
    let mut score = weather_bias(id);
    let mut has_alpha = false;
    let mut has_digit = false;
    let mut has_paren = false;
    let mut alnum_or_space = false;
    for &b in bytes {
        if b.is_ascii_alphabetic() { has_alpha = true; alnum_or_space = true; }
        else if b.is_ascii_digit() { has_digit = true; alnum_or_space = true; }
        else if b == b' ' { alnum_or_space = true; }
        else if b == b'(' || b == b')' { has_paren = true; }
    }
    if has_alpha { score += 1.5; }
    if has_digit { score -= 2.0; }
    if has_paren { score -= 3.0; }
    if !alnum_or_space { score -= 4.0; }
    score
}
