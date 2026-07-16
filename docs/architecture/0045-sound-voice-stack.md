# ADR-0045: Sound — Stack de Voz Nativo (HDA + Piper + STT CTC)

**Data:** 2026-07-16  
**Status:** Accepted  
**Sprint:** 107 (Voice I/O) — documentação do estado real pós Sprint Sound / 101  
**Depende de:** ADR-0036 (JARVIS Interaction), ADR-0042 (K²CHJ N5 voz como expressão)  
**Supersede / invalida como caminho primário:** sherpa-onnx, Pocket TTS, Kokoro-82M como TTS padrão, Vosk, Wyoming protocol, Rustpotter crate  
**Release:** documentado em CHANGELOG `[1.7.1]` (docs); package Cargo permanece `1.0.0` (hábito tag-only)

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
3. **WakeWord:** código existe (`wakeword.rs`); **não está registrado** no AgentFleet — gap Sprint 107, não “bloqueado por B-01”.  
4. **UAC (USB Audio Class):** stub (`usb.rs` / `UsbAudioAgent`) — válido como futuro (#84); HDA é o caminho atual.  
5. IDEA_BANK / ROADMAP / TECNOLOGIAS devem marcar itens obsoletos como ❌ supersedido (histórico preservado) e apontar para esta ADR.

### Non-goals

- Declarar pipeline TTS→STT→LLM→TTS “completo” (Sprint 107 ainda aberto).  
- Afirmar Piper neural pleno (`emb.weight` / params) quando o boot usa formant fallback.  
- Migrar o binário para `jarbas/audio` nesta ADR (só documentar o espelho).

---

## 3. Stack atual (honesto)

```text
Mic (HDA SD0) → ringbuf / VAD → [WakeWord*] → STT CTC → Hermes/Cortex
                                                              ↓
Speaker (HDA SD1) / FB paint ← mixer ← Piper VITS | formant ← TTS cmd
```

\* WakeWord: módulo presente; agente **não** registrado no boot.

| Componente | Arquivo (truth) | Estado |
|------------|-----------------|--------|
| Intel HDA capture+playback | `audio/hda.rs` | ✅ driver + agente registrado |
| Piper TTS VITS | `audio/piper.rs` | ▶️ LOADED via QEMU-loader; neural fraco → **formant fallback** |
| Formant TTS fallback | `audio/tts.rs` / voice path | ✅ produz `pcm_samples>0` |
| STT CTC (MFCC→LSTM→CTC) | `audio/stt.rs` | ✅ CTC LOADED (evidência QEMU) |
| VAD | `audio/vad.rs` | ✅ usado por voice path |
| Mixer / settings | `audio/mixer.rs`, `settings.rs` | ✅ |
| FB TTS paint | jarbas/display path + `[JARBAS-TTS-FB]` | ✅ painted em e2e clima |
| WakeWord “Jarvis” | `audio/wakeword.rs` | ⏳ código; **não registrado** |
| UAC stub | `audio/usb.rs` | ⏳ probe sempre false; futuro #84 |
| Pipeline / frames | `pipeline.rs`, `frame.rs`, `ringbuf.rs` | ▶️ parcial Sprint 107 |

**Agentes registrados (boot):** `HdaAudioAgent`, `UsbAudioAgent` (stub), `JarvisVoiceAgent`.  
**Não registrado:** `WakeWordAgent`.

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

## 5. Gaps Sprint 107 e próximos passos (ordem)

1. **Generate / qualidade PT** — logits 2B soft-float ainda fracos para frase climática plena (N3).  
2. **Piper neural** — corrigir lookup `emb.weight` / params (hoje formant).  
3. **Registrar WakeWordAgent** + ligar Mic→WAKEWORD→STT no EventBus.  
4. **Fechar loop** TTS→STT→LLM→TTS com telemetria honesta (não canned).  
5. **UAC real** (#84) quando HDA ausente (HW real / fones USB).  
6. **Wiring jarbas** — binário consome `jarbas::audio` sem duplicar truth no monólito.

---

## 6. IDEA_BANK — mapa de status (pós esta ADR)

| ID | Status pós-ADR | Nota |
|----|----------------|------|
| #75 Intel HDA | ✅ feito | Absorvido por #83 / HDA real |
| #83 HDA driver | ✅ feito | Sprint Sound / 101 |
| #84 UAC | 🟡 futuro | Stub; ainda válido |
| #315.21 Pocket/sherpa TTS | ❌ supersedido | → Piper + formant |
| #315.22 sherpa Whisper / Vosk | ❌ supersedido | → STT CTC |
| #315.23 Rustpotter | ❌ supersedido | → wakeword nativo (registrar) |
| #315.24 Audio ring buffer | ✅ feito / parcial | `ringbuf.rs` no truth |
| #315.25 Voice pipeline sherpa | ❌ supersedido como spec | Pipeline nativo = Sprint 107 |
| #315.N+1 Piper+Vosk+Wyoming | ❌ supersedido | Piper+CTC nativo; sem Wyoming |
| #360 Kokoro-82M | ❌ supersedido | Piper é o TTS primário |

---

## 7. Consequências

- Docs (`IDEA_BANK`, `TECNOLOGIAS`, `TODO`, `STATE`, `AGENTS`) alinhados a esta verdade.  
- Novas PRs de voz devem citar ADR-0045 e tocar `neural-kernel/src/audio` até o cutover jarbas.  
- Não reabrir “integrar sherpa/Vosk/Kokoro no kernel” sem ADR nova que justifique exceção (ex. tool host-only fora do boot).

---

## 8. Referências

- `crates/neural-kernel/src/audio/`  
- ADR-0036, ADR-0042 (N5)  
- `docs/memory/STATE.md` (evidência TTS/FB Sprint 107)  
- `docs/memory/SESSION_109.md` (sync docs desta ADR)
