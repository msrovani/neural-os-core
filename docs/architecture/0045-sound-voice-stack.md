# ADR-0045: Sound — Stack de Voz Nativo (HDA + Piper + STT CTC)

**Data:** 2026-07-16  
**Status:** Accepted  
**Sprint:** Sound (base ✅) + 107 Voice ✅ FECHADA — leftovers → **Sprint Sound (reaberta)**  
**Depende de:** ADR-0036 (JARVIS Interaction), ADR-0042 (K²CHJ N5 voz como expressão)  
**Supersede / invalida como caminho primário:** sherpa-onnx, Pocket TTS, Kokoro-82M como TTS padrão, Vosk, Wyoming protocol, Rustpotter crate  
**Release:** documentado em CHANGELOG `[1.7.1]` (docs); clima e2e `[1.7.2]`; handoff Sound `[1.7.3]` docs

---

## 1. Contexto

O projeto acumulou ideias de voz via ecossistema host (sherpa-onnx, Vosk, Wyoming, Kokoro ONNX, Rustpotter). Em bare-metal `no_std`, essas rotas **não são o stack bootável**. O que realmente roda no binário é o pipeline nativo sob `neural-kernel/src/audio/*`.

Documentação e IDEA_BANK ainda apontavam engines externos como 🟡/🔴 “pós B-01”, embora **B-01 (DHCP/RX NIC) esteja morto** (#415 — SLIP serial tunnel) e a voz **não dependa de rede** para TTS/STT local.

### Fonte da verdade vs espelho

| Caminho | Papel |
|---------|--------|
| `crates/neural-kernel/src/audio/*` | **Truth** — compilado e registrado no boot do monólito |
| `crates/jarbas/src/audio/*` | **Espelho de migração** K²CHJ — cópia estrutural; **ainda não wired** como binário de voz |

Até o wiring explícito `main` → crate `jarbas` para áudio, qualquer mudança de comportamento deve ir primeiro (ou só) em `neural-kernel`.

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
- Migrar o binário para `jarbas/audio` nesta ADR (só documentar o espelho; wire → Sound).  
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
| Intel HDA capture+playback | `audio/hda.rs` | ✅ driver + agente registrado |
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
| jarbas/audio wire | `jarbas/src/audio/*` | ⏳ espelho; allocator conflict → Sound |

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

## 5. Gaps → Sprint Sound (reaberta) — ordem sugerida

Sprint 107 Voice está **FECHADA** (PASS parcial forte+). Pendências abaixo **não** bloqueiam ADR-0042 N2→N5.

1. **STT retrain** — PCM→MFCC real (`tools/train_stt.py`; hoje synth).  
2. **Mic→Wake→STT→LLM→TTS runtime e2e** — skinny EventBus ✅; falta mic real + WHPX.  
3. **Piper VITS pleno** — neural-lite ≠ HiFi-GAN (`convert_piper_to_bitnet.py`).  
4. **Soft-float latency** — known blocker; defer sob Sound (sem fake).  
5. **UAC real** (#84) quando HDA ausente.  
6. **Wiring jarbas** — `jarbas-bridge` / resolver `#[global_allocator]`.  
7. **VAD / SER / Wake ML polish**.

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
| #438 N5 voz | ⏳ parcial | UI/persona = ADR-42; stack voz pleno = Sound |

---

## 7. Consequências

- Docs (`IDEA_BANK`, `TECNOLOGIAS`, `TODO`, `STATE`, `AGENTS`) alinhados a esta verdade.  
- Novas PRs de voz devem citar ADR-0045 e tocar `neural-kernel/src/audio` até o cutover jarbas.  
- Não reabrir “integrar sherpa/Vosk/Kokoro no kernel” sem ADR nova que justifique exceção (ex. tool host-only fora do boot).

---

## 8. Referências

- `crates/neural-kernel/src/audio/`  
- ADR-0036, ADR-0042 (N5)  
- `docs/memory/STATE.md` (evidência TTS/FB Sprint 107; handoff Sound)  
- `docs/memory/SESSION_109.md` (sync docs desta ADR)  
- `docs/memory/SESSION_111.md` (migração leftovers 107 → Sound)
