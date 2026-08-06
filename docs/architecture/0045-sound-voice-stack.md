# ADR-0045: Sound — Stack de Voz Nativo (HDA + Piper + STT CTC)

**Data:** 2026-07-16  
**Status:** Accepted  
**Sprint:** Sound base ✅ + 107 Voice ✅ + **Sprint Sound ✅** (2026-07-16; soft-float/VITS aberto; cutover jarbas ✅ e51a48b)  
**Depende de:** ADR-0036 (JARVIS Interaction), ADR-0042 (K³CHJ N5 voz como expressão)  
**Supersede / invalida como caminho primário:** sherpa-onnx, Pocket TTS, Kokoro-82M como TTS padrão, Vosk, Wyoming protocol, Rustpotter crate  
**Release:** documentado em CHANGELOG `[1.7.1]` (docs); clima e2e `[1.7.2]`; handoff Sound `[1.7.3]` docs

---

## 1. Contexto

O projeto acumulou ideias de voz via ecossistema host (sherpa-onnx, Vosk, Wyoming, Kokoro ONNX, Rustpotter). Em bare-metal `no_std`, essas rotas **não são o stack bootável**. O que realmente roda no binário é o pipeline nativo sob `jarbas/src/audio/*` (truth, desde o cutover e51a48b).

Documentação e IDEA_BANK ainda apontavam engines externos como 🟡/🔴 “pós B-01”, embora **B-01 (DHCP/RX NIC) esteja morto** (#415 — SLIP serial tunnel) e a voz **não dependa de rede** para TTS/STT local.

### Fonte da verdade (pós cutover e51a48b)

| Caminho | Papel |
|---------|--------|
| `crates/jarbas/src/audio/*` | **Truth** — 20 arquivos reais (mod, frame, ringbuf, vad, tts, ser, context, neural, piper, voice, skills, settings, mixer, jarvis, wakeword, usb, pipeline, token, codebook, stt) |
| `crates/neural-kernel/src/audio/mod.rs` | **Facade** — `pub use jarbas_crate::audio::*;` (4 linhas); bin re-exporta jarbas como truth de áudio |

O cutover de áudio foi concluído em **e51a48b**: o binário re-exporta `jarbas_crate::audio`, os antigos truth de `neural-kernel/src/audio/*` foram deletados e o bridge log reporta `audio_truth=jarbas-crate cutover=done(e51a48b)`. HDA permanece em R0 (`k_nano/src/audio/hda.rs`) com BE R1 (`k_hal/src/audio/hda.rs`) — ver pendência de duplicação na §7. Mudanças de comportamento de áudio devem ir em `jarbas/src/audio/*` (camada `audio/` genérica) ou `k_nano`/`k_hal` (HDA).

---

## 2. Decisão

1. **Stack canônico de voz (primário):** HDA + Piper VITS (+ fallback formant) + STT CTC + VAD + mixer + pintura FB do texto TTS.  
2. **Não adotar** sherpa-onnx / Pocket TTS / Kokoro / Vosk / Wyoming / Rustpotter como dependência de boot ou skill primária.  
3. **WakeWord:** código existe (`wakeword.rs`); **registrado** no AgentFleet (Loop 5 / Sprint 107). Path Mic→WAKEWORD e2e → Sprint Sound.  
4. **UAC (USB Audio Class):** stub + PCI USB probe (`usb.rs`) — válido como futuro (#84 → Sprint Sound); HDA é o caminho atual.  
5. IDEA_BANK / ROADMAP / TECNOLOGIAS devem marcar itens obsoletos como ❌ supersedido (histórico preservado) e apontar para esta ADR.

### Non-goals

- Declarar pipeline TTS→STT→LLM→TTS “completo” (leftovers em Sprint Sound).  
- Afirmar Piper VITS/HiFi-GAN pleno quando o boot usa neural-lite / formant.  
- Bloquear ADR-0042 N2→N5 com gaps de voz.

---

## 3. Stack atual (honesto)

```text
Mic (HDA SD0) → ringbuf / VAD → [WakeWord*] → STT CTC → Hermes/Cortex
                                                              ↓
Speaker (HDA SD1) / FB paint ← mixer ← Piper VITS | formant ← TTS cmd
```

\* WakeWord: módulo presente; agente **registrado** no boot (Loop 5). Path Mic→WAKE e2e → Sprint Sound.

| Componente | Arquivo (truth) | Estado |
|------------|-----------------|--------|
| Intel HDA capture+playback | `k_nano/audio/hda.rs` (R0) + `k_hal/audio/hda.rs` (R1 BE) | ✅ driver + agente registrado |
| Piper TTS VITS | `audio/piper.rs` | ▶️ neural-lite no e2e 107; VITS pleno → Sound |
| Formant TTS fallback | `audio/tts.rs` / voice path | ✅ produz `pcm_samples>0` |
| STT CTC (MFCC→LSTM→CTC) | `audio/stt.rs` | ▶️ CTC LOADED; retrain PCM → Sound |
| VAD | `audio/vad.rs` | ✅ base; refinements → Sound |
| Mixer / settings | `audio/mixer.rs`, `settings.rs` | ✅ |
| FB TTS paint | jarbas/display path + `[JARBAS-TTS-FB]` | ✅ painted em e2e clima |
| WakeWord “Jarvis” | `audio/wakeword.rs` | ✅ registrado; Mic→WAKE e2e → Sound |
| UAC stub | `audio/usb.rs` | ⏳ PCI probe; enum real → Sound (#84) |
| SER | `audio/ser.rs` | ▶️ heurístico; polish → Sound |
| Pipeline / frames | `pipeline.rs`, `frame.rs`, `ringbuf.rs` | ▶️ skinny 107 ✅; runtime pleno → Sound |
| jarbas/audio cutover | `jarbas/src/audio/*` | ✅ **e51a48b** — bin re-exporta `jarbas_crate::audio`; antigos truth de `neural-kernel/src/audio/*` deletados |

**Agentes registrados (boot):** `HdaAudioAgent`, `UsbAudioAgent` (stub), `JarvisVoiceAgent`, `WakeWordAgent`.

---

## 4. O que esta ADR supersede

| Tecnologia / IDEA | Motivo |
|-------------------|--------|
| sherpa-onnx (+ Pocket TTS / Whisper bindings) | Requer `std`/host; não cabe no boot `no_std` |
| Kokoro-82M como TTS padrão (#360) | Substituído por Piper VITS nativo + formant |
| Vosk STT | Substituído por STT CTC nativo |
| Wyoming protocol | Stack OVOS/host; não é IPC bare-metal |
| Rustpotter crate | Substituído por MLP wakeword nativo (quando registrar) |
| Bloqueio “voz pós B-01” (#315.21–25, #315.N+1) | Voz local ≠ NIC; B-01 resolvido via SLIP (#415) |

Histórico em CHANGELOG antigo (ex. VoiceService “Piper + Vosk”) permanece como registro; **não** é plano ativo.

---

## 5. Sprint Sound — status (2026-07-16)

| Item | Status |
|------|--------|
| STT PCM→MFCC (`train_stt.py`) | ✅ path + `STT.BIN`; WER CTC tiny ainda fraco |
| Mic→Wake→STT→TTS gate | ✅ Wake Continuous; janela pós-WAKE; bypass `weather-e2e` |
| Piper neural-lite polish | ✅; VITS/HiFi-GAN **não** implementado (soft-float blocker) |
| Soft-float latency | ⏳ defer honesto |
| UAC parse (#84) | ✅ descriptors; isócrono DMA → HW |
| VAD / SER / Wake polish | ✅ |
| jarbas cutover | ✅ **e51a48b** — truth = `jarbas/src/audio`; bin `pub use` + deletados espelhos |

Soft-float qualidade PT do 2B também toca **N3** (ADR-0042) — não só voz.

---

## 6. IDEA_BANK — mapa de status (pós esta ADR)

| ID | Status pós-ADR | Nota |
|----|----------------|------|
| #75 Intel HDA | ✅ feito | Absorvido por #83 / HDA real |
| #83 HDA driver | ✅ feito | Sprint Sound / 101 |
| #84 UAC | 🟡 futuro | → Sprint Sound (reaberta) |
| #315.21 Pocket/sherpa TTS | ❌ supersedido | → Piper + formant |
| #315.22 sherpa Whisper / Vosk | ❌ supersedido | → STT CTC |
| #315.23 Rustpotter | ❌ supersedido | → wakeword nativo (registrado) |
| #315.24 Audio ring buffer | ✅ feito / parcial | `ringbuf.rs` no truth |
| #315.25 Voice pipeline sherpa | ❌ supersedido como spec | Pipeline nativo = Sprint Sound |
| #315.N+1 Piper+Vosk+Wyoming | ❌ supersedido | Piper+CTC nativo; leftovers → Sound |
| #360 Kokoro-82M | ❌ supersedido | Piper é o TTS primário |
| #438 N5 voz | ✅ funcional / qualidade pendente | UI/persona e expressão = ADR-42; voz production-grade = Sound |

---

## 7. Consequências

- Docs (`IDEA_BANK`, `TECNOLOGIAS`, `TODO`, `STATE`, `AGENTS`) alinhados a esta verdade.  
- Novas PRs de voz devem citar ADR-0045 e tocar `jarbas/src/audio/*` (truth) — HDA em `k_nano`/`k_hal`.  
- Não reabrir “integrar sherpa/Vosk/Kokoro no kernel” sem ADR nova que justifique exceção (ex. tool host-only fora do boot).

### Pendências honestas (pós cutover)

- **Soft-float / Piper VITS:** VITS/HiFi-GAN **não** implementado — boot usa neural-lite/formant (blocker soft-float).  
- **UAC isócrono:** `#84` ▶️ **AWAITING_HW** — parse+probe+USB-TRUST ✅; DMA isócrono exige HW real.  
- **Dedup HDA `k_nano` ↔ `k_hal`:** duplicação real (bin inicializa via `k_nano::audio::hda`, agente/poll via `k_hal::audio::hda` com cópia própria de `init_hda`/`poll_hda_audio`) — flagrada pelo guarda `tools/check_duplication.py`; item separado, NÃO parte deste cutover.

---

## 8. Referências

- `crates/jarbas/src/audio/` (truth pós cutover e51a48b) · `crates/k_nano/src/audio/` + `crates/k_hal/src/audio/` (HDA R0/R1)
- ADR-0036, ADR-0042 (N5)  
- `docs/memory/STATE.md` (evidência TTS/FB Sprint 107; handoff Sound)  
- `docs/memory/SESSION_109.md` (sync docs desta ADR)  

---

## 9. Planos Cursor implementados

### `Sprint Sound completa` (`sprint_sound_completa`)

| Entrega do plano | Status | Nota |
|------------------|--------|------|
| Mic→Wake→STT→TTS gate | ✅ | SESSION_122 |
| STT PCM path + `STT.BIN` | ✅ | WER CTC tiny fraco |
| UAC parse descriptors | ✅ | isócrono DMA → HW |
| Piper neural-lite | ✅ | VITS/HiFi-GAN **não** (soft-float) |
| VAD/SER/Wake polish | ✅ | |
| jarbas cutover re-export | ✅ | **e51a48b**; truth = `jarbas/src/audio` |

Residuals abertos: soft-float/VITS, UAC isócrono AWAITING_HW, dedup HDA `k_nano`↔`k_hal` (guarda) — lifecycle ADR `completa` com residuals explícitos (não reabre sherpa/Vosk).

Também: `docs/memory/SESSION_111.md` (migração leftovers 107 → Sound).
