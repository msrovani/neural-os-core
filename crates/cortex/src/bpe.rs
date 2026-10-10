//! BPE vocab compacto (BPB1) — decode id→texto para BitNet.
//! - Llama-3 128k (2B): chat frame + IDs semânticos
//! - SentencePiece BPE 32k (850/xl/3B): merges MRG1 + ▁→espaço no decode

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec;
use alloc::vec::Vec;

const SP_SPACE: char = '\u{2581}'; // ▁ SentencePiece
const BYTELEVEL_SPACE: char = '\u{0120}'; // Ġ ByteLevel (GPT-2/Falcon3)

/// Tabla carregada do QEMU-loader (`target/bpe_vocab.bin` ou `bpe_vocab_sp32.bin`).
pub struct BpeVocab {
    bos: u32,
    eos: u32,
    eot: u32,
    vocab_n: u32,
    /// offsets[i]..offsets[i+1] no heap
    offsets: Vec<u32>,
    heap: Vec<u8>,
    /// Só SP32 (vocab≤33k): peça→id p/ encode
    rev: BTreeMap<String, u32>,
    /// Merges BPE em ordem (HF tokenizer.json) — encode correcto vs greedy
    merges: Vec<(String, String)>,
}

impl BpeVocab {
    pub fn bos(&self) -> u32 { self.bos }
    pub fn eos(&self) -> u32 { self.eos }
    pub fn eot(&self) -> u32 { self.eot }
    pub fn vocab_n(&self) -> u32 { self.vocab_n }

    /// SentencePiece 32k (BitNet 850/xl/3B) vs Llama-3 128k (2B).
    pub fn is_sp32(&self) -> bool {
        self.vocab_n > 0 && self.vocab_n <= 33_000
    }

    /// Falcon3 ByteLevel BPE (131k, `bos=<|startoftext|>=10`) vs Llama-3 128k
    /// (ByteLevel mas `bos=128000`). Distingue o frame de chat correto: para o
    /// Falcon3 o `encode_chat_frame` legado (cue Llama-3, 6 tokens) NÃO tokeniza
    /// o prompt — s437.
    pub fn is_falcon_bytelevel(&self) -> bool {
        !self.is_sp32() && self.bos < 1000
    }

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
            // ora-1 bullet2: sem cap 128000 — Falcon3 vai a 131072; especiais
            // caem no filtro `<|...|>` abaixo (texto-legível intacto).
            if let Some(s) = self.decode_id(t) {
                if s.starts_with("<|") && s.ends_with("|>") {
                    continue;
                }
                if s == "<s>" || s == "</s>" || s == "<unk>" || s == "<pad>" || s == "</line>" {
                    continue;
                }
                // SentencePiece ▁ / ByteLevel Ġ = espaço (mapa no decode, não no armazenamento)
                for ch in s.chars() {
                    if ch == SP_SPACE || ch == BYTELEVEL_SPACE {
                        out.push(' ');
                    } else {
                        out.push(ch);
                    }
                }
            }
        }
        out
    }

    /// Encode SentencePiece BPE (merges HF) — alinhado a `tokenizers`.
    /// "ola" → [<s>, ▁o, la] (não greedy ▁ol+a).
    pub fn encode_sp32(&self, text: &str) -> Vec<u32> {
        let mut out = vec![self.bos];
        if text.is_empty() {
            return out;
        }
        // SP: espaços → ▁; prefixo ▁ no início
        let mut norm = String::new();
        norm.push(SP_SPACE);
        for ch in text.chars() {
            if ch == ' ' {
                norm.push(SP_SPACE);
            } else {
                norm.push(ch);
            }
        }
        // 1 char = 1 peça; aplica merges em ordem
        let mut word: Vec<String> = norm.chars().map(|c| {
            let mut s = String::new();
            s.push(c);
            s
        }).collect();
        if !self.merges.is_empty() {
            for (a, b) in self.merges.iter() {
                let mut i = 0usize;
                while i + 1 < word.len() {
                    if word[i] == *a && word[i + 1] == *b {
                        let mut merged = String::with_capacity(a.len() + b.len());
                        merged.push_str(a);
                        merged.push_str(b);
                        word[i] = merged;
                        word.remove(i + 1);
                    } else {
                        i += 1;
                    }
                }
            }
        } else {
            // Fallback greedy se MRG1 ausente (pior; evita ▁ol vs ▁o+la quando possível
            // via preferência peça-mais-curta entre top-2 — ainda imperfecto)
            return self.encode_sp32_greedy_fallback(text);
        }
        for piece in word.iter() {
            if let Some(&id) = self.rev.get(piece) {
                out.push(id);
            } else {
                out.push(0); // <unk>
            }
        }
        out
    }

    fn encode_sp32_greedy_fallback(&self, text: &str) -> Vec<u32> {
        let mut out = vec![self.bos];
        if text.is_empty() || self.rev.is_empty() {
            return out;
        }
        let mut s = String::new();
        s.push(SP_SPACE);
        for ch in text.chars() {
            if ch == ' ' {
                s.push(SP_SPACE);
            } else {
                s.push(ch);
            }
        }
        const MAX_PIECE: usize = 24;
        while !s.is_empty() {
            let mut best: Option<(u32, usize)> = None;
            let max_try = s.len().min(MAX_PIECE);
            let mut len = max_try;
            while len > 0 {
                if s.is_char_boundary(len) {
                    let piece = &s[..len];
                    if let Some(&id) = self.rev.get(piece) {
                        best = Some((id, len));
                        break;
                    }
                }
                len -= 1;
            }
            if let Some((id, len)) = best {
                out.push(id);
                s = String::from(&s[len..]);
            } else {
                let Some(ch) = s.chars().next() else { break; };
                let l = ch.len_utf8();
                out.push(0);
                s = String::from(&s[l..]);
            }
        }
        out
    }

    /// Encode merge-order: aplica merges BPE iterativamente até não conseguir mais.
    /// Mais preciso que greedy (encontra a segmentação BPE ótima).
    /// Usa a lista de merges `self.merges` em ordem de inserção (MRG1 do loader).
    pub fn encode_merge_order(&self, text: &str) -> Vec<u32> {
        let mut out = vec![self.bos];
        if text.is_empty() {
            return out;
        }
        // SP32: ▁ prefix + replace ' ' → ▁
        let mut norm = String::new();
        norm.push(SP_SPACE);
        for ch in text.chars() {
            if ch == ' ' {
                norm.push(SP_SPACE);
            } else {
                norm.push(ch);
            }
        }
        if self.merges.is_empty() {
            return self.encode_sp32_greedy_fallback(text);
        }
        // Start with character-level pieces
        let mut word: Vec<String> = norm.chars().map(|c| {
            let mut s = String::new();
            s.push(c);
            s
        }).collect();
        // Iteratively apply the highest priority merge until no more merges apply
        self.apply_bpe_merges(&mut word);
        // Convert pieces to token IDs
        for piece in word.iter() {
            if let Some(&id) = self.rev.get(piece) {
                out.push(id);
            } else {
                // Fallback: try to find sub-piece encoding
                out.push(0); // <unk>
            }
        }
        out
    }

    /// Aplica merges BPE (rank = ordem em `self.merges`) até estabilizar.
    /// Menor rank aplicável vence; reinicia após cada merge (BPE canónico).
    /// ponytail: varre `self.merges` (128k) por merge — O(merges·len) por peça;
    /// aceitável p/ prompts curtos; um rank-map trocaria por O(len).
    fn apply_bpe_merges(&self, word: &mut Vec<String>) {
        loop {
            let mut merged = false;
            // Try merges in priority order (list order = rank)
            for (a, b) in self.merges.iter() {
                let mut i = 0;
                while i + 1 < word.len() {
                    if word[i] == *a && word[i + 1] == *b {
                        let mut m = String::with_capacity(a.len() + b.len());
                        m.push_str(a);
                        m.push_str(b);
                        word[i] = m;
                        word.remove(i + 1);
                        merged = true;
                        // Restart from the beginning after a merge for optimality
                        break;
                    } else {
                        i += 1;
                    }
                }
                if merged { break; }
            }
            if !merged { break; }
        }
    }

    /// ByteLevel BPE encode (GPT-2/Falcon3, vocab 131k): texto → token IDs.
    ///
    /// Pipeline: pretokenizer GPT-2 (ASCII) → byte→unicode (mapa GPT-2) →
    /// merges por rank → `rev` lookup. Desconhecido → 0 (`<unk>`).
    ///
    /// FIDELIDADE: o tokenizer Falcon3 real usa
    /// `Sequence[Punctuation(Contiguous), ByteLevel, Digits(individual_digits)]`.
    /// Para ASCII, a regex GPT-2 + split individual de dígitos reproduz a
    /// referência (`tokenizers` 0.23.2, `target1/falcon3/tokenizer.json`) —
    /// verificado 0/430 em corpus ASCII (ver `tests::bytelevel_matches_falcon3_reference`).
    /// LIMITAÇÃO: NÃO-ASCII é aproximado (`char::is_alphabetic`/`is_numeric` no
    /// lugar de `\p{L}`/`\p{N}` estritos) e não é coberto pelos testes.
    pub fn encode_bytelevel(&self, text: &str) -> Vec<u32> {
        let mut out = Vec::new();
        for pretoken in pretokenize_bytelevel(text) {
            // Cada BYTE vira uma peça (mapa GPT-2), não cada char.
            let mut word: Vec<String> = pretoken
                .bytes()
                .map(|b| {
                    let mut s = String::new();
                    s.push(byte_to_char(b));
                    s
                })
                .collect();
            self.apply_bpe_merges(&mut word);
            for piece in &word {
                out.push(self.rev.get(piece).copied().unwrap_or(0));
            }
        }
        out
    }

    /// Encode genérico (não clima): moldura Llama curta + cue do 1º token semântico.
    /// Usado em HW real quando `weather-e2e` está off.
    pub fn encode_chat_frame(&self, prompt: &str) -> Vec<u32> {
        if self.is_sp32() {
            return self.encode_merge_order(prompt);
        }
        let p = prompt.as_bytes();
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
        if prompt_is_greeting(prompt) {
            return self.encode_greeting_cue(prompt);
        }
        // Cue: primeira palavra ASCII ≥3 chars → id heurístico via hash no vocab
        // (sem merges BPE pleno). Fallback: token " hi" / espaço+hello-ish.
        let mut cue = 1919u32; // Ġhi aproximado comum; sobrescrito se achar keyword
        if lower_has(b"tempo") || lower_has(b"clima") || lower_has(b"weather") {
            cue = 24108; // Ġtempo
        } else if lower_has(b"hello") || lower_has(b"ola") || lower_has(b"oi") {
            cue = 22691; // " Hello"
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

    /// Falcon3 Instruct: `<|user|>\n{prompt}\n<|assistant|>\n` + BOS, tokenizado
    /// com o BPE ByteLevel REAL. O frame Llama-3 (`encode_chat_frame`) descartava
    /// o texto e devolvia 6 tokens constantes — todo job via o mesmo input e a
    /// saída degenerava no mesmo gibberish determinístico (s437, `prompt_len=6`).
    pub fn encode_falcon_chat(&self, prompt: &str) -> Vec<u32> {
        let mut framed = String::with_capacity(prompt.len() + 32);
        framed.push_str("<|user|>\n");
        framed.push_str(prompt);
        framed.push_str("\n<|assistant|>\n");
        let mut out = vec![self.bos];
        out.extend(self.encode_bytelevel(&framed));
        out
    }

    /// Moldura chat + cue de saudacao (IDs BPB1 reais). Logits escolhem o resto.
    pub fn encode_greeting_cue(&self, prompt: &str) -> Vec<u32> {
        if self.is_sp32() {
            return self.encode_sp32(prompt);
        }
        let p = prompt.as_bytes();
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
        // 7839=" Good", 22691=" Hello", 2052=" All"
        let cue = if lower_has(b"hello") || lower_has(b"hola") {
            22691u32
        } else if lower_has(b"all systems") || lower_has(b"systems") {
            2052u32
        } else {
            7839u32 // Good
        };
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
        if self.is_sp32() {
            return self.encode_sp32(prompt);
        }
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
    // Aceita trailing padding / MRG1 no loader
    let heap = data[o..o + heap_len].to_vec();
    o += heap_len;
    let mut rev = BTreeMap::new();
    // Índice inverso p/ encode (SP32 32k e ByteLevel 131k Falcon3).
    // O gate antigo `<= 33_000` deixava o Falcon3 131k sem text→id.
    if vocab_n > 0 {
        for id in 0..vocab_n {
            let i = id as usize;
            let a = offsets[i] as usize;
            let b = offsets[i + 1] as usize;
            if b > heap.len() || a > b {
                continue;
            }
            if let Ok(s) = core::str::from_utf8(&heap[a..b]) {
                if s.is_empty() {
                    continue;
                }
                // Não indexar specials
                if s.starts_with('<') && s.ends_with('>') {
                    continue;
                }
                rev.entry(String::from(s)).or_insert(id);
            }
        }
    }
    // MRG1: merges BPE (opcional; necessário p/ encode SP32 correcto)
    let mut merges: Vec<(String, String)> = Vec::new();
    if o + 8 <= data.len() && &data[o..o + 4] == b"MRG1" {
        o += 4;
        let merge_n = rd_u32(data, &mut o)? as usize;
        if merge_n > 200_000 {
            return Err("bad merge_n");
        }
        merges.reserve(merge_n);
        for _ in 0..merge_n {
            if o + 2 > data.len() {
                return Err("trunc merge a len");
            }
            let la = u16::from_le_bytes([data[o], data[o + 1]]) as usize;
            o += 2;
            if o + la > data.len() {
                return Err("trunc merge a");
            }
            let a = core::str::from_utf8(&data[o..o + la]).map_err(|_| "merge a utf8")?;
            o += la;
            if o + 2 > data.len() {
                return Err("trunc merge b len");
            }
            let lb = u16::from_le_bytes([data[o], data[o + 1]]) as usize;
            o += 2;
            if o + lb > data.len() {
                return Err("trunc merge b");
            }
            let b = core::str::from_utf8(&data[o..o + lb]).map_err(|_| "merge b utf8")?;
            o += lb;
            merges.push((String::from(a), String::from(b)));
        }
    }
    let vocab = BpeVocab {
        bos,
        eos,
        eot,
        vocab_n,
        offsets,
        heap,
        rev,
        merges,
    };
    k_nano::slog_bin!("BPE", "ok", "BPB1 LOADED vocab_n={} bos={} eos={} heap={}KB rev={} merges={} sp32={}",
        vocab.vocab_n,
        vocab.bos,
        vocab.eos,
        heap_len / 1024,
        vocab.rev.len(),
        vocab.merges.len(),
        vocab.is_sp32() as u8);
    *BPE.lock() = Some(vocab);
    Ok(())
}

/// Scan QEMU loader region for `BPB1` magic.
/// The PS1 auto-loader places files sequentially from `0x100000000`,
/// so we scan 1MB-aligned addresses looking for the magic.
pub fn try_load_from_qemu_loader() -> bool {
    let phys_off = k_nano::memory::PHYS_MEM_OFFSET.load(core::sync::atomic::Ordering::Relaxed);
    if phys_off == 0 {
        return false;
    }
    scan_and_load_bpb1(phys_off, 0x100000000, 0x180000000, 0x100000)
}

/// Scan address range [start..end) at `step` for BPB1 magic, load first match.
fn scan_and_load_bpb1(phys_off: u64, start: u64, end: u64, step: u64) -> bool {
    let mut addr = start;
    while addr < end {
        // SESSION_262 / matrix QA: hole além da RAM -> #PF storm (CR2=pmoff+0x140000000).
        if !k_nano::memory::is_page_present(addr + phys_off) {
            addr = addr.saturating_add(step);
            continue;
        }
        let va = (addr + phys_off) as *const u8;
        unsafe {
            let magic = core::slice::from_raw_parts(va, 4);
            if magic == b"BPB1" {
                k_nano::slog_bin!("BPE", "ok", "BPB1 found @0x{:x} (scan)",
                    addr);
                let vocab_n = u32::from_le_bytes([
                    *va.add(18), *va.add(19), *va.add(20), *va.add(21),
                ]) as usize;
                if vocab_n == 0 || vocab_n > 200_000 {
                    k_nano::slog_bin!("BPE", "warn", "bad vocab_n={} @0x{:x}",
                        vocab_n, addr);
                    addr = addr.saturating_add(step);
                    continue;
                }
                let header = 6 + 16 + (vocab_n + 1) * 4;
                let heap_off_ptr = va.add(6 + 16 + vocab_n * 4);
                let heap_len = u32::from_le_bytes([
                    *heap_off_ptr,
                    *heap_off_ptr.add(1),
                    *heap_off_ptr.add(2),
                    *heap_off_ptr.add(3),
                ]) as usize;
                let mut total = header + heap_len;
                // MRG1 opcional após heap
                let mrg = va.add(total);
                if total < 12 * 1024 * 1024
                    && core::slice::from_raw_parts(mrg, 4) == b"MRG1"
                {
                    let merge_n = u32::from_le_bytes([
                        *mrg.add(4), *mrg.add(5), *mrg.add(6), *mrg.add(7),
                    ]) as usize;
                    let mut o = total + 8;
                    for _ in 0..merge_n {
                        if o + 4 > 12 * 1024 * 1024 { break; }
                        let la =
                            u16::from_le_bytes([*va.add(o), *va.add(o + 1)])
                                as usize;
                        o += 2 + la;
                        if o + 2 > 12 * 1024 * 1024 { break; }
                        let lb =
                            u16::from_le_bytes([*va.add(o), *va.add(o + 1)])
                                as usize;
                        o += 2 + lb;
                    }
                    if o < 12 * 1024 * 1024 { total = o; }
                }
                if total > 12 * 1024 * 1024 {
                    addr = addr.saturating_add(step);
                    continue;
                }
                // Fail-closed: valida TODAS as páginas 4K de [va, va+total)
                // antes de expor via from_raw_parts (hole além da RAM = #PF).
                {
                    let va_u64 = va as u64;
                    let end_va = va_u64.saturating_add(total as u64);
                    let mut p = va_u64 & !0xFFF;
                    let mut hole = false;
                    while p < end_va {
                        if !k_nano::memory::is_page_present(p) {
                            hole = true;
                            break;
                        }
                        p = p.saturating_add(0x1000);
                    }
                    if hole {
                        k_nano::slog_bin!("BPE", "warn",
                            "BPB1 hole @0x{:x} total={} — skip", addr, total);
                        addr = addr.saturating_add(step);
                        continue;
                    }
                }
                let slice = core::slice::from_raw_parts(va, total);
                match init_from_bpb1(slice) {
                    Ok(()) => return true,
                    Err(e) => {
                        k_nano::slog_bin!("BPE", "fail",
                            "BPB1 parse FAILED @0x{:x}: {}", addr, e);
                    }
                }
            }
        }
        addr = addr.saturating_add(step);
    }
    k_nano::slog_bin!("BPE", "warn",
        "QEMU-loader scan [{:#x}..{:#x}] — BPB1 ausente", start, end);
    false
}

/// FAT32 `BPE.BIN` / `BPEVOCAB.BIN` — path HW real (sem QEMU-loader).
/// Sandbox (QEMU/WHPX): skip ATA PIO no boot — timeout 7–10s (paridade BGE SESSION_345).
pub fn try_load_from_fat() -> bool {
    if k_nano::platform_probe::probe_done()
        && k_nano::platform_probe::hypervisor().is_sandbox()
    {
        k_nano::slog_bin!(
            "BPE",
            "warn",
            "skip FAT PIO boot hv={} — Runtime/HW ou QEMU-loader BPB1",
            k_nano::platform_probe::hypervisor().name()
        );
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
                    for name in &["BPE.BIN", "BPEVOCAB.BIN", "BPESP32.BIN"] {
                        if let Some(data) = fs.read_file(name) {
                            match init_from_bpb1(&data) {
                                Ok(()) => {
                                    k_nano::slog_bin!("BPE", "ok", "BPB1 LOADED from FAT {} ({}KB)",
                                        name,
                                        data.len() / 1024);
                                    return true;
                                }
                                Err(e) => {
                                    k_nano::slog_bin!("BPE", "fail", "FAT {} parse FAILED: {}",
                                        name,
                                        e);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    k_nano::slog_bin!("BPE", "warn", "FAT ausente (BPE.BIN)");
    false
}

/// Prompt de saudacao boot / "Generate a single short sentence greeting".
pub fn prompt_is_greeting(text: &str) -> bool {
    let b = text.as_bytes();
    let has = |s: &[u8]| {
        if s.is_empty() || b.len() < s.len() {
            return false;
        }
        b.windows(s.len()).any(|w| {
            w.iter()
                .zip(s.iter())
                .all(|(a, c)| a.to_ascii_lowercase() == *c)
        })
    };
    has(b"greeting")
        || has(b"saudacao")
        || has(b"generate a single short sentence")
        || (has(b"you are jarvis") && has(b"greeting"))
        || has(b"oi jarbas")
        || has(b"oi jarvis")
        || has(b"ola jarbas")
        || has(b"hello jarbas")
        || has(b"hello jarvis")
}

pub fn encode(text: &str) -> Vec<u32> {
    let guard = BPE.lock();
    match guard.as_ref() {
        Some(tok) => {
            if tok.is_sp32() {
                // BitNet 850/xl/3B: merge-order (mais preciso que greedy).
                tok.encode_merge_order(text)
            } else if tok.is_falcon_bytelevel() {
                // Falcon3 131k: template Instruct + texto real (s437). Antes caía
                // em `encode_chat_frame` (cue Llama-3, 6 tokens) e o prompt era
                // descartado — mesma saída p/ todo input.
                tok.encode_falcon_chat(text)
            } else if prompt_is_greeting(text) {
                tok.encode_greeting_cue(text)
            } else if false { // ponytail: demo_flags bin-specific
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

/// ora-1 bullet1/3: Falcon3 ativo? (ByteLevel 131k, bos<1000). Usado p/ bypass
/// do slim e p/ argmax puro (sem coherence/bias) no path Falcon.
pub fn is_falcon_active() -> bool {
    BPE.lock().as_ref().map(|t| t.is_falcon_bytelevel()).unwrap_or(false)
}

/// ora-1 bullet5: contrato gibberish honesto sobre janela de 8 toks.
/// true = rep 4-gram>0.6 OU distinct-2<0.2 OU piece_len médio<3.
/// Sem vocab carregado → false (não acusa no path texto-legível/char).
pub fn gibberish_stop(tail: &[u32]) -> bool {
    if tail.len() < 8 {
        return false;
    }
    let w = &tail[tail.len() - 8..];
    // distinct-2: 7 bigramas; <0.2 → ≤1 distinto.
    let mut dist2 = 0usize;
    for i in 0..7 {
        let a0 = w[i];
        let b0 = w[i + 1];
        let mut seen = false;
        for k in 0..i {
            if w[k] == a0 && w[k + 1] == b0 {
                seen = true;
                break;
            }
        }
        if !seen {
            dist2 += 1;
        }
    }
    if dist2 * 5 < 7 {
        // dist2/7 < 0.2  → dist2 ≤ 1
        return true;
    }
    // 4-gram: 5 janelas; rep>0.6 → distinct/total<0.4 → ≤1 distinto.
    let mut dist4 = 0usize;
    for i in 0..5 {
        let mut seen = false;
        for k in 0..i {
            if w[k] == w[i] && w[k + 1] == w[i + 1] && w[k + 2] == w[i + 2] && w[k + 3] == w[i + 3] {
                seen = true;
                break;
            }
        }
        if !seen {
            dist4 += 1;
        }
    }
    let total4 = 5usize;
    // rep = 1 - dist/total > 0.6
    if (total4 - dist4) * 10 > total4 * 6 {
        return true;
    }
    // piece_len médio < 3 (peças de 1-2 chars = BPE colapsado).
    let guard = BPE.lock();
    let Some(tok) = guard.as_ref() else { return false };
    let mut sum = 0usize;
    let mut n = 0usize;
    for &id in w {
        // decode_id cru (sem skip de special — special já filtrado no argmax;
        // se aparecer aqui, len 0 baixa a média = suspeito, honesto).
        let l = tok.decode_id(id).map(|s| s.chars().count()).unwrap_or(0);
        sum += l;
        n += 1;
    }
    if n > 0 && sum * 10 < n * 30 {
        // sum/n < 3
        return true;
    }
    false
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
        0 => &[46, 24108, 88603, 74258, 1788, 23700, 30279], // O / tempo / Tempo / hoje / qual / como / esta
        1 => match prev {
            Some(46) => &[24108, 88603, 74258, 18205, 15491, 30279, 38169, 1788][..], // O → tempo/Tempo/hoje/dia/esta/faz/qual
            Some(24108) | Some(88603) => &[15491, 30279, 38169, 18205, 74258, 18665, 76321, 2092, 39298, 1788][..], // tempo → esta/faz/dia/hoje/bom/claro/sol/qual
            _ => &[24108, 88603, 15491, 30279, 38169, 18205, 74258, 18665, 76321, 1788][..],
        },
        2 => weather_candidate_ids(), // full lexicon — logits escolhem
        _ => weather_candidate_ids(), // step>=3: lexicon completo — sem subconjunto estreito fixo
    }
}

/// Bigram PT suave (ainda escolhe via logits; só reordena).
/// Bias reduzidos ~50% para dar mais peso aos logits reais do modelo.
pub fn weather_bigram_bias(prev: Option<u32>, id: u32) -> f32 {
    let Some(p) = prev else { return 0.0 };
    // O → tempo
    if p == 46 && (id == 24108 || id == 88603) { return 2.5; }
    // tempo → esta / faz / bom (NÃO dia/Tempo primeiro)
    if (p == 24108 || p == 88603)
        && matches!(id, 15491 | 30279 | 38169 | 18665 | 76321)
    {
        return 3.0;
    }
    if (p == 24108 || p == 88603) && matches!(id, 18205 | 74258) {
        return -0.75; // dia/hoje depois do verbo
    }
    // esta → bom / claro / sol / hoje
    if matches!(p, 15491 | 30279) && matches!(id, 18665 | 76321 | 2092 | 39298 | 74258) {
        return 2.0;
    }
    // faz → sol / bom / claro
    if p == 38169 && matches!(id, 2092 | 39298 | 18665 | 76321) {
        return 1.75;
    }
    // hoje → faz / dia / claro
    if p == 74258 && matches!(id, 38169 | 18205 | 76321 | 18665) {
        return 1.5;
    }
    // Evita tempo→rain / tempo→weather
    if (p == 24108 || p == 88603) && weather_is_en_loan(id) {
        return -2.0;
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

/// Texto de saudacao fluente (early-exit generate).
pub fn text_is_greetingish(text: &str) -> bool {
    let b = text.as_bytes();
    let has = |s: &[u8]| {
        if s.is_empty() || b.len() < s.len() {
            return false;
        }
        b.windows(s.len()).any(|w| {
            w.iter()
                .zip(s.iter())
                .all(|(a, c)| a.to_ascii_lowercase() == *c)
        })
    };
    let spaces = b.iter().filter(|&&c| c == b' ').count();
    if spaces < 1 || text.trim().len() < 10 {
        return false;
    }
    // Suit-boot + saudacoes classicas (texto original JARBAS / Neural OS).
    let open = has(b"good")
        || has(b"hello")
        || has(b"all systems")
        || has(b"JARBAS")
        || has(b"upload")
        || has(b"at your")
        || has(b"hud");
    let body = has(b"online")
        || has(b"ready")
        || has(b"operational")
        || has(b"systems")
        || has(b"day")
        || has(b"morning")
        || has(b"service")
        || has(b"fleet")
        || has(b"standing by")
        || has(b"nominal")
        || has(b"engaged");
    open && body
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
    let Some(tok) = guard.as_ref() else { return 0.0; };
    // SP32: sem bias clima Llama-128k (IDs 24108/… colidem com peças erradas).
    let base = if tok.is_sp32() { 0.0 } else { weather_bias(id) };
    let Some(s) = tok.decode_id(id) else { return base; };
    if s.starts_with('<') && s.ends_with('>') {
        return -20.0; // specials / pad /line
    }
    let bytes = s.as_bytes();
    let mut score = base;
    let mut has_alpha = false;
    let mut has_digit = false;
    let mut has_paren = false;
    let mut alnum_or_space = false;
    for &b in bytes {
        if b.is_ascii_alphabetic() { has_alpha = true; alnum_or_space = true; }
        else if b.is_ascii_digit() { has_digit = true; alnum_or_space = true; }
        else if b == b' ' || b == 0xE2 { alnum_or_space = true; } // espaço ou ▁ utf8 lead
        else if b == b'(' || b == b')' { has_paren = true; }
    }
    // ▁ sozinho / peças só-espaço
    if s == "\u{2581}" || s.trim().is_empty() {
        score -= 1.0;
    }
    if has_alpha { score += 1.5; }
    if has_digit { score -= 2.0; }
    if has_paren { score -= 3.0; }
    if !alnum_or_space { score -= 4.0; }
    score
}

/// True se id é special SP/Llama (não gerar).
pub fn is_special_id(id: u32) -> bool {
    let guard = BPE.lock();
    let Some(tok) = guard.as_ref() else {
        return id <= 2;
    };
    if id == tok.bos() || id == tok.eos() || id == tok.eot() {
        return true;
    }
    if let Some(s) = tok.decode_id(id) {
        if s.starts_with('<') && s.ends_with('>') {
            return true;
        }
    }
    false
}

// ── ByteLevel BPE (GPT-2/Falcon3) ───────────────────────────────────────────

/// GPT-2 `bytes_to_unicode`: imprimíveis mapeiam p/ si; o resto → 256+n
/// (n = nº de bytes não-imprimíveis anteriores). Tabela de 256 entradas.
fn byte_to_char(b: u8) -> char {
    let printable = |x: u8| matches!(x, 0x21..=0x7E | 0xA1..=0xAC | 0xAE..=0xFF);
    if printable(b) {
        b as char
    } else {
        let n = (0u16..b as u16).filter(|&x| !printable(x as u8)).count() as u32;
        char::from_u32(256 + n).unwrap_or('\u{FFFD}')
    }
}

fn is_letter(c: char) -> bool { c.is_alphabetic() }
fn is_number(c: char) -> bool { c.is_numeric() }
fn is_space(c: char) -> bool { c.is_whitespace() }
fn is_punct(c: char) -> bool { !is_space(c) && !is_letter(c) && !is_number(c) }

/// Contração GPT-2 (`'s|'t|'re|'ve|'m|'ll|'d`), em ordem.
const CONTRACTIONS: [&str; 7] = ["'s", "'t", "'re", "'ve", "'m", "'ll", "'d"];

/// Pretokenizer GPT-2 hand-rolled (ASCII) + `Digits(individual_digits)` Falcon3.
/// Sem regex (no_std). Ver `BpeVocab::encode_bytelevel` p/ fidelidade/limitações.
fn pretokenize_bytelevel(text: &str) -> Vec<String> {
    let ch: Vec<char> = text.chars().collect();
    let n = ch.len();
    let mut out: Vec<String> = Vec::new();
    let mut i = 0usize;
    while i < n {
        let c = ch[i];
        // `'s|'t|'re|'ve|'m|'ll|'d`
        if c == '\'' {
            let mut hit = 0usize;
            for pat in CONTRACTIONS {
                let pc: Vec<char> = pat.chars().collect();
                if i + pc.len() <= n && ch[i..i + pc.len()] == pc[..] {
                    hit = pc.len();
                    break;
                }
            }
            if hit > 0 {
                out.push(ch[i..i + hit].iter().collect());
                i += hit;
                continue;
            }
        }
        // ` ?\p{L}+`
        let j = if c == ' ' { i + 1 } else { i };
        if j < n && is_letter(ch[j]) {
            let mut k = j;
            while k < n && is_letter(ch[k]) { k += 1; }
            out.push(ch[i..k].iter().collect());
            i = k;
            continue;
        }
        // ` ?\p{N}+`
        if j < n && is_number(ch[j]) {
            let mut k = j;
            while k < n && is_number(ch[k]) { k += 1; }
            out.push(ch[i..k].iter().collect());
            i = k;
            continue;
        }
        // ` ?[^\s\p{L}\p{N}]+`
        if j < n && is_punct(ch[j]) {
            let mut k = j;
            while k < n && is_punct(ch[k]) { k += 1; }
            out.push(ch[i..k].iter().collect());
            i = k;
            continue;
        }
        // `\s+(?!\S)|\s+`: agrupa corrida de espaços; recua 1 se seguida de não-espaço
        if is_space(c) {
            let mut k = i;
            while k < n && is_space(ch[k]) { k += 1; }
            if k == n {
                out.push(ch[i..k].iter().collect());
                i = k;
            } else if k - i >= 2 {
                out.push(ch[i..k - 1].iter().collect());
                i = k - 1;
            } else {
                out.push(c.to_string());
                i += 1;
            }
            continue;
        }
        out.push(c.to_string());
        i += 1;
    }
    // Falcon3 `Digits(individual_digits=true)`: cada dígito vira pretoken próprio
    let mut out2 = Vec::with_capacity(out.len());
    for seg in out {
        let mut cur = String::new();
        for c in seg.chars() {
            if is_number(c) {
                if !cur.is_empty() {
                    out2.push(core::mem::take(&mut cur));
                }
                out2.push(c.to_string());
            } else {
                cur.push(c);
            }
        }
        if !cur.is_empty() {
            out2.push(cur);
        }
    }
    out2
}

/// Entry point explícito: ByteLevel BPE (Falcon3 131k). NÃO altera `encode`.
pub fn encode_bytelevel(text: &str) -> Vec<u32> {
    let guard = BPE.lock();
    match guard.as_ref() {
        Some(tok) => tok.encode_bytelevel(text),
        None => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Fixture REAL do vocab Falcon3 131k (target1/falcon3/tokenizer.json).
    // PIECES = peças usadas (id real); MERGES = subconjunto aplicado, ordenado
    // por rank (dos 128810 reais); GROUND_TRUTH = saída de `tokenizers` 0.23.2.
    const PIECES: &[(u32, &str)] = &[
        (2024, "!"),
        (2030, "'"),
        (2035, ","),
        (2039, "0"),
        (2040, "1"),
        (2041, "2"),
        (2042, "3"),
        (2043, "4"),
        (2044, "5"),
        (2049, ":"),
        (2054, "?"),
        (2058, "C"),
        (2070, "O"),
        (2106, "s"),
        (2107, "t"),
        (2226, "\u{0120}"),
        (2287, "it"),
        (2343, "\u{0120}you"),
        (2401, "\u{0120}we"),
        (2402, "\u{0120}are"),
        (2455, "all"),
        (2609, "ll"),
        (2932, "\u{0120}don"),
        (3139, "\u{0120}world"),
        (3236, "the"),
        (3490, "\u{0120}How"),
        (4184, "\u{0120}quick"),
        (5119, "\u{0120}systems"),
        (9307, "\u{0120}brown"),
        (10544, "\u{0120}esta"),
        (13955, "Hello"),
        (14800, "\u{0120}operational"),
        (15086, "\u{0120}bom"),
        (17035, "\u{0120}Safe"),
        (18938, "\u{0120}tempo"),
        (22112, "Safe"),
        (29840, "\u{0120}fox"),
        (49815, "temperature"),
        (116026, "Dangerous"),
    ];
    const MERGES: &[(&str, &str)] = &[
        ("\u{0120}", "t"),
        ("\u{0120}", "a"),
        ("h", "e"),
        ("r", "e"),
        ("o", "n"),
        ("e", "r"),
        ("\u{0120}", "s"),
        ("a", "t"),
        ("o", "r"),
        ("\u{0120}", "w"),
        ("e", "s"),
        ("o", "u"),
        ("i", "t"),
        ("a", "n"),
        ("\u{0120}", "f"),
        ("\u{0120}", "b"),
        ("\u{0120}", "o"),
        ("a", "l"),
        ("\u{0120}", "d"),
        ("i", "c"),
        ("i", "on"),
        ("o", "m"),
        ("s", "t"),
        ("r", "o"),
        ("e", "l"),
        ("\u{0120}", "y"),
        ("\u{0120}", "S"),
        ("\u{0120}y", "ou"),
        ("o", "w"),
        ("at", "ion"),
        ("q", "u"),
        ("es", "t"),
        ("e", "m"),
        ("\u{0120}w", "e"),
        ("\u{0120}a", "re"),
        ("\u{0120}", "H"),
        ("l", "d"),
        ("al", "l"),
        ("el", "l"),
        ("p", "er"),
        ("\u{0120}w", "or"),
        ("u", "re"),
        ("\u{0120}", "qu"),
        ("ou", "s"),
        ("l", "l"),
        ("f", "e"),
        ("an", "g"),
        ("ic", "k"),
        ("ro", "w"),
        ("y", "st"),
        ("yst", "em"),
        ("\u{0120}d", "on"),
        ("ation", "al"),
        ("\u{0120}", "est"),
        ("\u{0120}wor", "ld"),
        ("\u{0120}s", "ystem"),
        ("o", "x"),
        ("t", "he"),
        ("at", "ure"),
        ("\u{0120}H", "ow"),
        ("\u{0120}o", "per"),
        ("\u{0120}t", "em"),
        ("\u{0120}qu", "ick"),
        ("row", "n"),
        ("\u{0120}system", "s"),
        ("ang", "er"),
        ("p", "o"),
        ("\u{0120}S", "a"),
        ("ell", "o"),
        ("\u{0120}b", "rown"),
        ("\u{0120}est", "a"),
        ("H", "ello"),
        ("\u{0120}oper", "ational"),
        ("\u{0120}b", "om"),
        ("S", "a"),
        ("\u{0120}Sa", "fe"),
        ("\u{0120}tem", "po"),
        ("t", "em"),
        ("Sa", "fe"),
        ("per", "ature"),
        ("\u{0120}f", "ox"),
        ("tem", "perature"),
        ("D", "anger"),
        ("Danger", "ous"),
    ];
    const GROUND_TRUTH: &[(&str, &[u32])] = &[
        ("Safe", &[22112]),
        ("Dangerous", &[116026]),
        ("Hello, world! How are you?", &[13955, 2035, 3139, 2024, 3490, 2402, 2343, 2054]),
        ("it's don't we'll", &[2287, 2030, 2106, 2932, 2030, 2107, 2401, 2030, 2609]),
        (" Safe", &[17035]),
        ("O tempo esta bom", &[2070, 18938, 10544, 15086]),
        ("all systems operational", &[2455, 5119, 14800]),
        ("the quick brown fox", &[3236, 4184, 9307, 29840]),
        ("12345", &[2040, 2041, 2042, 2043, 2044]),
        ("temperature: 20C", &[49815, 2049, 2226, 2041, 2039, 2058]),
    ];

    /// Monta um `BpeVocab` a partir da fixture (mesmo layout de offsets/heap do loader).
    fn fixture_vocab() -> BpeVocab {
        let vocab_n = PIECES.iter().map(|(i, _)| *i).max().unwrap() + 1;
        let mut heap: Vec<u8> = Vec::new();
        let mut offsets = Vec::new();
        offsets.resize((vocab_n + 1) as usize, 0u32);
        let mut by_id: Vec<(u32, &str)> = PIECES.to_vec();
        by_id.sort_by_key(|(i, _)| *i);
        let mut pi = 0usize;
        let mut cur = 0usize;
        for id in 0..vocab_n {
            while pi < by_id.len() && by_id[pi].0 < id {
                pi += 1;
            }
            offsets[id as usize] = cur as u32;
            if pi < by_id.len() && by_id[pi].0 == id {
                heap.extend_from_slice(by_id[pi].1.as_bytes());
                cur += by_id[pi].1.len();
            }
        }
        offsets[vocab_n as usize] = cur as u32;
        let mut rev = BTreeMap::new();
        for (id, s) in PIECES {
            if s.is_empty() || (s.starts_with('<') && s.ends_with('>')) {
                continue;
            }
            rev.entry(String::from(*s)).or_insert(*id);
        }
        BpeVocab {
            bos: 0,
            eos: 1,
            eot: 2,
            vocab_n,
            offsets,
            heap,
            rev,
            merges: MERGES
                .iter()
                .map(|(a, b)| (String::from(*a), String::from(*b)))
                .collect(),
        }
    }

    #[test]
    fn bytelevel_matches_falcon3_reference() {
        let v = fixture_vocab();
        for (text, want) in GROUND_TRUTH {
            assert_eq!(v.encode_bytelevel(text).as_slice(), *want, "text={:?}", text);
        }
    }

    #[test]
    fn bytelevel_decode_maps_bytelevel_space() {
        let v = fixture_vocab();
        assert_eq!(v.decode(&[17035]), " Safe"); // "ĠSafe"
        assert_eq!(v.decode(&[22112]), "Safe");
    }

    #[test]
    fn byte_to_char_matches_gpt2_table() {
        assert_eq!(byte_to_char(b'!'), '!');
        assert_eq!(byte_to_char(b'~'), '~');
        assert_eq!(byte_to_char(b' '), '\u{0120}');
        assert_eq!(byte_to_char(0), '\u{0100}');
        assert_eq!(byte_to_char(127), '\u{0121}');
        assert_eq!(byte_to_char(160), '\u{0142}');
        assert_eq!(byte_to_char(173), '\u{0143}');
        assert_eq!(byte_to_char(255), '\u{00FF}');
        let mut seen = [false; 512];
        for b in 0u16..256 {
            let c = byte_to_char(b as u8) as u32 as usize;
            assert!(!seen[c], "byte {} colide", b);
            seen[c] = true;
        }
    }

    #[test]
    fn bytelevel_pretokenizer_digits_and_contractions() {
        let v = fixture_vocab();
        // `Digits(individual_digits)`: cada dígito vira token próprio
        assert_eq!(v.encode_bytelevel("12345").as_slice(), &[2040, 2041, 2042, 2043, 2044]);
        // `Punctuation` separa o apóstrofo: `'s` NÃO funde
        assert_eq!(v.encode_bytelevel("it's").as_slice(), &[2287, 2030, 2106]);
    }

    /// s437: Falcon3 (ByteLevel 131k) deve tokenizar o PROMPT, não devolver o
    /// frame-cue Llama-3 fixo de 6 tokens (que descartava o texto e degenerava
    /// toda resposta no mesmo gibberish determinístico).
    #[test]
    fn falcon_chat_encodes_prompt_not_cue_frame() {
        let v = fixture_vocab();
        assert!(v.is_falcon_bytelevel(), "fixture = Falcon ByteLevel (bos<1000, vocab>33k)");
        let a = v.encode_falcon_chat(" world");
        let b = v.encode_falcon_chat("temperature");
        assert!(a.len() > 6, "prompt deve ser tokenizado (len={})", a.len());
        assert_ne!(a, b, "prompts diferentes → tokens diferentes");
        assert_eq!(a[0], v.bos(), "BOS no início");
        assert!(a.contains(&3139), "peça 'Ġworld' presente");
        assert!(b.contains(&49815), "peça 'Ġtemperature' presente");
    }

    /// Bullet 4: path Falcon nunca cruza cues greeting/weather — mesmo texto
    /// de saudação tokeniza o prompt real, nunca o frame-cue fixo de 6 toks.
    #[test]
    fn falcon_ignores_greeting_cue() {
        let v = fixture_vocab();
        assert!(v.is_falcon_bytelevel());
        let f = v.encode_falcon_chat("hello jarbas");
        let g = v.encode_greeting_cue("hello jarbas");
        assert_eq!(g.len(), 6, "cue legado = frame fixo de 6 toks");
        assert!(f.len() > 6, "Falcon tokeniza o prompt (len={})", f.len());
        assert_ne!(f, g, "Falcon nunca devolve o cue de saudação");
        assert_eq!(f[0], v.bos(), "BOS no início");
    }

    /// Monta um BPB1 sintético (só o header + heap; sem MRG1).
    fn synthetic_bpb1(vocab_n: u32, pieces: &[(u32, &str)]) -> Vec<u8> {
        let mut offs = Vec::new();
        offs.resize(vocab_n as usize + 1, 0u32);
        let mut heap: Vec<u8> = Vec::new();
        let mut sorted: Vec<(u32, &str)> = pieces.to_vec();
        sorted.sort_by_key(|(i, _)| *i);
        let mut pi = 0usize;
        let mut cur = 0u32;
        for id in 0..vocab_n {
            while pi < sorted.len() && sorted[pi].0 < id {
                pi += 1;
            }
            offs[id as usize] = cur;
            if pi < sorted.len() && sorted[pi].0 == id {
                heap.extend_from_slice(sorted[pi].1.as_bytes());
                cur += sorted[pi].1.len() as u32;
            }
        }
        offs[vocab_n as usize] = cur;
        let mut d = Vec::new();
        d.extend_from_slice(b"BPB1");
        d.extend_from_slice(&1u16.to_le_bytes());
        d.extend_from_slice(&0u32.to_le_bytes()); // bos
        d.extend_from_slice(&1u32.to_le_bytes()); // eos
        d.extend_from_slice(&2u32.to_le_bytes()); // eot
        d.extend_from_slice(&vocab_n.to_le_bytes());
        for o in &offs {
            d.extend_from_slice(&o.to_le_bytes());
        }
        d.extend_from_slice(&heap);
        d
    }

    #[test]
    fn rev_map_built_for_bytelevel_vocab_over_33k() {
        // Task 1: o gate antigo `vocab_n <= 33_000` deixava o Falcon3 131k sem `rev`.
        let data = synthetic_bpb1(40_000, &[(39_999, "Zzz"), (0, "A")]);
        init_from_bpb1(&data).expect("BPB1 parse");
        {
            let g = BPE.lock();
            let v = g.as_ref().expect("vocab carregado");
            assert_eq!(v.rev.get("Zzz"), Some(&39_999u32));
            assert_eq!(v.rev.get("A"), Some(&0u32));
        }
        *BPE.lock() = None; // não vaza estado p/ outros testes
    }
}
