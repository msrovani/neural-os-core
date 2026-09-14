# SESSION_346 — Voice Pipeline 2.0: dono único do mic, formato HDA, STT fatiado e honesto

**Data:** 2026-09-14
**Sprint:** s346 (marco único "reescrita da topologia de agentes de áudio")
**Escopo:** plano `Plano de Ataque — Pipeline de Voz 2.0` (WS1–WS6), aprovado pelo maintainer.
**Numeração:** SESSION_345 é de outra frente (FAT pins / persist defer) — este registro
usa **346** para não colidir (regra da lição SESSION_264/277).

---

## Por que: os defeitos medidos no código real

A pergunta era "por que o STT/emoção não funcionam e por que a voz trava o orb?".
A resposta não estava no modelo — estava na **captura**.

| # | Defeito | Evidência (antes) | Impacto |
|---|---------|-------------------|---------|
| D1 | BDL do SD0 lido **16× por interrupção** | `hda.rs`: `let rpi = load()` fora do `for _ in 0..16`, logo `entry_idx`/`next_rpi` constantes | 32.768 amostras duplicadas por BCIS; RPI avançava 1 enquanto o HW passava N entradas |
| D2a | Formato do ADC errado | `FMT_16BIT_48KHZ_STEREO = 0x21` → no layout Intel (§3.7.1, bits 14:12 canais / 10:8 bits / 6:4 base rate) `0x21` = **8-bit mono 32 kHz não-PCM** | Todo o áudio (RX e TX) com taxa/canais/profundidade trocados |
| D2b | Playback mono 16 kHz dentro de stream 48 kHz estéreo | `write_hda_playback` escrevia i16 mono direto no buffer SD1 | TTS ~6× acelerado, L/R alternados |
| D3 | 37,5% do áudio descartado | `voice.rs`/`wakeword.rs`: `chunks(320)` + `if len < 320 { continue }` sobre eventos de 512 | 192 de cada 512 amostras jogadas fora, sempre nas mesmas fronteiras |
| D4 | Dois VADs independentes | VAD próprio em `wakeword.rs` **e** em `voice.rs` | 2× custo, estados que podiam discordar |
| D5 | Treino com **bipes**, não fala | `train_stt.py::synthesize_pcm` = 3 senoides com `f0/f1/f2` derivados de `ord(ch)-ord('a')`; validação em `CORPUS[:8]` (vazamento) | O modelo não pode transcrever fala real — por construção |
| D6 | Decoder **fabricava** texto | `stt.rs` "blank-suppress decode": re-decodificava com blank suprimido | Inventava comandos a partir de ruído (viola a premissa de honestidade do AIOS) |
| D7 | STT inteiro dentro do tick | `mfcc` com DFT O(513×512) ≈ 263k MACs/frame | Segundos de congelamento do orb/mouse por transcrição |
| D8 | EWMA sobre **índice do enum** | `LAST_VOICE_EMOTION = prev*0.7 + idx*0.3` | joy(0) e sarcasm(7) médiavam para fear(3) |
| D9 | Barge-in não interrompia | `pipeline.rs` limpava o ring, mas `JarbasAgent::stream_tts` redrenava no tick seguinte | A fala voltava |
| D10 | `SKIP_NEXT_FULL_TTS` bool único | Pular o `HERMES_RESPONSE` inteiro | A frase final (nunca fechada durante o generate) **nunca era falada** |
| D13 | `MIC_CAPTURE_RING` sem consumidor real | push em `voice.rs`, pop só no detector de barge-in | Ring morto |

---

## O que foi feito

### WS1 — `crates/k_nano/src/audio/hda.rs`
- `FMT_16BIT_48KHZ_STEREO = 0x0000_1100` (chan(2)=0x1000 | 16-bit=0x100 | base 48k=0) e
  `FMT_16BIT_16KHZ_MONO = 0x0150` (usada na observação de capacidade, path 16 k gated).
- Constantes `CAPTURE_RATE_HZ=48000`, `CAPTURE_CHANNELS=2`, `VOICE_RATE_HZ=16000`,
  `VOICE_DECIM=3` — **fonte única**; a FE não hardcoda mais 16 kHz.
- **IRQ não faz mais trabalho pesado**: só seta `HDA_SD0_PENDING` e dá ack (sem alloc em
  contexto de interrupção, sem walk duplicado).
- `drain_sd0_completed()` — único walk do BDL, guiado pelo **LPIB** (posição do HW no
  buffer cíclico): drena exatamente as entradas completas, cada uma **uma vez**.
  LPIB implausível → fallback honesto de 1 entrada/chamada + contador `CAP_LPIB_STALE`.
- `publish_sd0_entry` — 1 publish por entrada (2048 amostras) em vez de 4 chunks de 512
  (4× menos alocações no hot path).
- Contadores auditáveis: `CAP_ENTRIES_DRAINED`, `CAP_SAMPLES_PUBLISHED`, `CAP_LPIB_STALE`,
  `PLAY_SAMPLES_DROPPED`, `CAP_ADC_SUPPORTED_FMT`.
- **Observação de capacidade do ADC** (Get Parameter 0x0A) registrada no boot — o switch
  para 16 kHz mono fica para HW real, com o dado em mãos (Observe→Plan do ADR-0042).
- `write_hda_playback` — expande mono→estéreo (L=R) e interpola ×`VOICE_DECIM`
  (inverso exato da decimação), com **cursor de escrita** e back-pressure honesto
  (descarta o excedente em vez de rasgar o anel): `free_frames` derivado do LPIB do SD1.

### WS2 — `crates/jarbas/src/audio/capture.rs` (novo)
`AudioInputAgent` é o **dono único do microfone**: único caller de `poll_hda_audio`/
`poll_uac_audio`; de-interleave estéreo→mono (com fase persistente entre eventos
ímpares); decimação 48k→16k com filtro box; **carry-over** entre eventos; publica
`AUDIO_FRAME` de **320 amostras exatas** (fim do D3: por construção
`OUT_SAMPLES * VOICE_DECIM == IN_SAMPLES`). Contadores: `IN_SAMPLES`, `OUT_SAMPLES`,
`FRAMES_PUBLISHED`, `CARRY_OVERFLOW` (todos auditáveis).

### WS3 — `voice.rs` + `wakeword.rs`
- VAD **único**, agora no dono do mic; transições publicadas em `VAD_TRANSITION`.
- `WakeWordAgent` perdeu VAD/energy-chunking: consome frames fixos e roda só o
  classificador (MLP + padrão de picos) em janelas de 64 frames (~1,28 s).
- `JarbasVoiceAgent` → sessão com `VoiceState { Sleeping, Listening, Thinking, Speaking,
  BargeIn, Error }`, **derivado** a cada tick e publicado em `VOICE_STATE` só quando muda
  (orb/HUD desenham snapshot em vez de decidir).
- `AudioPipelineAgent` (e `pipeline.rs`) **removidos** — seu único papel era o barge-in,
  que passou para a sessão.

### WS4 — barge-in real
- `TTS_GENERATION: AtomicU64`. `request_interrupt()` limpa o ring, **incrementa a
  geração**, cancela a inferência e volta a escutar. O `stream_tts` do `JarbasAgent`
  carrega a geração e é **abandonado** se ela mudou (antes era redrenado).
- `SPOKEN_PARTIAL` (ledger) substitui `SKIP_NEXT_FULL_TTS`: o `HERMES_RESPONSE` fala
  **só o que falta** (`unsaid_remainder`), sem repetir nem perder a frase final.
  Se o prefixo não casar, fala tudo (repetir < perder).

### WS5 — STT
- **FFT radix-2** (Cooley–Tukey, 512 pontos) no lugar da DFT ingênua: ~100× menos MACs
  por frame. **Teste de paridade FFT vs DFT direta** incluído (`fft_matches_naive_dft`).
  O filterbank/log/CMVN foram mantidos **idênticos** de propósito, para não invalidar o
  `STT.BIN` existente num mesmo turno em que não há como retreinar.
- **Job em slices** (`begin_job`/`step_job`, 64 frames/tick) — a transcrição sai do tick.
- **Removido o caminho que inventava texto**; baixa confiança publica `STT_UNCERTAIN`
  com `conf`/`blank_ratio`. Teste `blank_only_path_yields_no_text` garante que um caminho
  todo-blank devolve vazio.
- **Vocabulário derivado do modelo** (`out.bias.len()`), não fixo em 28; a ordem canônica
  vive em `stt.rs::VOCAB_CHARS` e é verificada contra o trainer por
  `tools/stt_vocab_check.py` (passou: 27 chars, vocab=28, blank=27).
- Header do `.bin`: offset 4 passa a ser `feat_version`; o kernel avisa se divergir.

### WS5 (tooling)
- `tools/gen_stt_corpus.py` (novo): corpus PT-BR do domínio + variações (**840 frases**,
  era 20 fixas) e síntese **real** via espeak-ng multi-voz/velocidade → `data/stt_wavs/`.
- `tools/train_stt.py`: `synthesize_pcm` agora **prefere fala real** e só cai no formant
  com **aviso grave** (o fallback não produz modelo utilizável); `TRAIN_CORPUS` carrega
  `data/stt_corpus.txt`; holdout passa a ser o **fim** do corpus (era `CORPUS[:8]`, que
  também era treino); `VOCAB` derivado de `VOCAB_CHARS`; `FEAT_VERSION` gravado.

### WS6 — emoção
- `EMOTION_DIST: [AtomicU32; 8]` (Q8, decaimento α=0.7) + dominante por **argmax**;
  `LAST_VOICE_EMOTION` deixa de ser média de índices.
- `context.rs` injeta a **segunda opinião** (`Alt: <emoção> (<peso>)`) no prompt quando a
  distribuição é ambígua — antes só o vencedor aparecia e a LLM perdia a ambiguidade.

---

## Verificação

| Checagem | Resultado |
|----------|-----------|
| `cargo check -p k-nano` | 0 erros |
| `cargo check -p jarbas` | 0 erros (2 warnings pré-existentes em `jarvis.rs`) |
| `cargo check --release -p neural-kernel` (rebuild real, 17,7 s) | **0 erros**, 36 warnings (baseline) |
| `cargo test -p jarbas --lib` | **92 passed / 0 failed** (era 89; +3 novos) |
| `cargo test --workspace --exclude neural-kernel --exclude boot --no-fail-fast` | 1 falha em `k_ai::boot_observe::ingest_bootlog_cross_boot_order_and_cap` → **flake** (passa isolado e em `-p k_ai --lib`: 45/45) |
| `python tools/stt_vocab_check.py` | ✅ vocabulário kernel↔trainer idêntico |
| `python tools/gen_stt_corpus.py --corpus-only` | ✅ 840 frases em `data/stt_corpus.txt` |

**Não validado (honesto):** nenhum boot QEMU/real foi executado nesta sessão. Os números
de captura (≈47 entradas/s, `CARRY_OVERFLOW == 0`, `PLAY_SAMPLES_DROPPED`) são o critério
de aceite a medir no próximo boot. O `STT.BIN` atual (front-end legado, dataset de bipes)
continua impreciso: precisa de `gen_stt_corpus.py` + `train_stt.py` com espeak-ng.

## Residuais
1. **Aceite QEMU/HW** dos contadores de captura e do `VOICE_STATE` no HUD.
2. **Retreinar o STT** com fala real (espeak-ng/Piper) — 840 frases já disponíveis.
3. **Filterbank Mel triangular + DCT-II** no lugar do filterbank atual (centers em mel,
   sem DCT): exige treino e kernel mudando juntos → `FEAT_VERSION=2`.
4. **AEC (NLMS)** com `PLAYBACK_RING` como referência — não implementado nesta passada
   (WS4 ficou no barge-in; full-duplex exige medir ERLE no metal).
5. Switch gated para ADC 16 kHz mono usando `CAP_ADC_SUPPORTED_FMT` observado no boot.
6. Cadência do mixer (1024 amostras/tick) vs relógio do codec — hoje sobra ~15% e o
   excedente é descartado (contabilizado, sem tear).
