# Análise Profunda — Pipeline de Áudio Neural OS
## Data: 2026-08-30 | Versão: v1.9.9

---

## 1. MAPA COMPLETO DA CADEIA

```
┌─────────────────────────────────────────────────────────────────────┐
│                    PIPELINE DE ÁUDIO END-TO-END                     │
├─────────────────────────────────────────────────────────────────────┤
│                                                                     │
│  [HW] HDA SD0 (MIC) ──poll_hda_audio()──► AUDIO_IN (EventBus)     │
│       │                                     │                       │
│       │                    ┌────────────────┤                       │
│       │                    ▼                ▼                       │
│       │            WakeWordAgent    JarbasVoiceAgent                │
│       │            (MLP 16→8→1)     (VAD → STT)                    │
│       │                    │                │                       │
│       │                    ▼                ▼                       │
│       │            WAKEWORD ──► wake_window=ON                      │
│       │                                    │                       │
│       │                                    ▼                       │
│       │                           STT (MFCC→LSTM→CTC)              │
│       │                                    │                       │
│       │                                    ▼                       │
│       │                           USER_INTENT (EventBus)           │
│       │                                    │                       │
│       │                    ┌───────────────┤                       │
│       │                    ▼               ▼                       │
│       │            HermesAgent      JarbasAgent                    │
│       │            (parse_command)  (LLM_REQUEST)                  │
│       │                    │               │                       │
│       │                    ▼               ▼                       │
│       │            HERMES_RESPONSE (EventBus)                      │
│       │                    │                                       │
│       │                    ├───────────────┐                       │
│       │                    ▼               ▼                       │
│       │            DisplayAgent     JarbasVoiceAgent               │
│       │            (HUD/Chat)       (TTS → PLAYBACK_RING)          │
│       │                                    │                       │
│       │                                    ▼                       │
│       │                           Piper/Formant TTS                │
│       │                                    │                       │
│       │                                    ▼                       │
│       │                           AUDIO_OUT (EventBus)             │
│       │                                    │                       │
│       │                                    ▼                       │
│       │                           AudioMixerAgent                  │
│       │                           (volume + drain)                 │
│       │                                    │                       │
│       ▼                                    ▼                       │
│  [HW] HDA SD1 (SPEAKER) ◄── write_hda_playback()                 │
│                                                                     │
│  ─── PARALELO ───                                                  │
│  Orb FFT ◄── process_audio_fft(pcm) ◄── voice.rs                  │
│  SoulMirror ◄── EmotionAnalysis ◄── voice.rs                      │
│  SGDB ◄── DataCollector (USER_INTENT/HERMES_RESPONSE pairs)       │
│                                                                     │
└─────────────────────────────────────────────────────────────────────┘
```

---

## 2. ESTADO ATUAL — O QUE FUNCIONA

| Estágio | Componente | Status | Evidência |
|---------|-----------|--------|-----------|
| **HW Capture** | HDA SD0 (poll_hda_audio) | ✅ | `poll_hda_audio()` lê DMA ring, publica `AUDIO_IN` |
| **HW Playback** | HDA SD1 (write_hda_playback) | ✅ | Mixer drena `PLAYBACK_RING` → HDA SD1 |
| **Wake Word** | MLP 16→8→1 + energy pattern | ✅ | `WakeWordAgent` Continuous, 98.4% acurácia treinada |
| **VAD** | Energy-based SpeechStart/SpeechEnd | ✅ | `VAD` com threshold configurável |
| **STT** | MFCC→LSTM→CTC decode | ✅ | DFT completo (Sprint 107 fix), vocab 28 chars |
| **Intent** | USER_INTENT → HermesAgent | ✅ | `parse_command` + IntentCache |
| **Response** | HERMES_RESPONSE → DisplayAgent | ✅ | ChatWindow + HUD |
| **TTS** | Piper (neural) / Formant (fallback) | ✅ | Piper carrega do FAT, formant como fallback |
| **Orb** | FFT audio visualization | ✅ | `process_audio_fft` em voice.rs |
| **Emotion** | SER (pitch/energy → Emotion) | ✅ | `classify_emotion` em ser.rs |
| **SGDB** | DataCollector pares (in/out) | ✅ | `k_ai::data_collector` |

---

## 3. GAPS IDENTIFICADOS — QUEBRAS NA CADEIA

### 🔴 CRÍTICO — Gaps que quebram a experiência

#### GAP 1: HDA SD0 não publica AUDIO_IN quando HW ausente (QEMU)
- **Problema:** `poll_hda_audio()` só funciona com Intel HDA real. QEMU sem `-audiodev` não tem HDA → SD0 DMA zera → `AUDIO_IN` nunca é publicado → Wake Word nunca ouve → pipeline inteiro morto.
- **Impacto:** **100% dos testes QEMU falham no estágio 1** — sem áudio de entrada, nada funciona.
- **Fix:** Adicionar `UAC_AUDIO_SIMULATED` para QEMU: gerar silêncio + pub周期rico `AUDIO_IN` vazio quando HDA indisponível, OU melhor: simular PCM de teste (sine wave) quando `testing_audio` estiver habilitado.

#### GAP 2: WakeWord → VoiceAgent sem gate de timeout efetivo
- **Problema:** `wake_window` decrementa por tick, mas se o LLM demorar > wake_window ticks, o voice agent para de escutar antes de receber a resposta.
- **Impacto:** Usuário diz "Jarvis, qual o clima?" → wake → STT → USER_INTENT → LLM processa 200 ticks → wake_window expirou → voice agent dorme → HERMES_RESPONSE chega mas voice agent não escuta mais → **sem TTS de resposta**.
- **Fix:** `wake_window` deve ser pausado/enquanto `pending_user_text` estiver Some, ou resetado quando HERMES_RESPONSE chega.

#### GAP 3: TTS não é streaming — resposta inteira primeiro
- **Problema:** `synthesize_tts(&body)` espera a resposta COMPLETA do LLM antes de sintetizar. LLM gera 50 palavras → 2 segundos de espera → TTS sintetiza tudo → MIXER drena.
- **Impacto:** **Latência percebida > 3s** para respostas simples. Usuário espera silêncio longo.
- **Fix:** TTS streaming: sintetizar por frases (split em `. ` ou `\n`) e enfileirar no PLAYBACK_RING progressivamente.

#### GAP 4: Orb não reage ao TTS playback
- **Problema:** `process_audio_fft(pcm)` é chamado APENAS para `pcm` do mic (voice.rs tick). O áudio do TTS (PLAYBACK_RING) NÃO alimenta o orb.
- **Impacto:** Orb pulsa quando usuário fala, mas FICA PARADO quando Jarbas responde — experiência visual incompleta.
- **Fix:** Alimentar `process_audio_fft` também com samples do PLAYBACK_RING (mixer ou voice agent).

### 🟡 MÉDIO — Gaps que degradam a experiência

#### GAP 5: EmotionAnalysis sem histórico temporal
- **Problema:** `classify_emotion(&features)` analisa APENAS o último chunk de fala. Se o usuário fala 3 frases com emoções diferentes, só a última importa.
- **Impacto:** Emoção do orb/persona oscila drasticamente em vez de suavizar.

#### GAP 6: Conversation context limitada a 3 turnos
- **Problema:** `conversation` no voice agent mantém apenas `max_conversation=10` pares, mas o enhanced prompt usa só `saturating_sub(3)` — últimos 3.
- **Impacto:** LLM perde contexto de conversas longas.

#### GAP 7: Piper TTS carrega no boot mas não valida qualidade
- **Problema:** Piper carrega pesos do FAT mas não há teste de "peak onset" — se os pesos estiverem corrompidos, TTS produz ruído sem erro visível.
- **Impacto:** Áudio "funciona" mas é incompreensível.

### 🟢 BAIXO — Melhorias desejáveis

#### GAP 8: Barge-in interrompe playback mas não re-inicia STT
- **Problema:** `BARGE_IN` limpa `PLAYBACK_RING` mas não seta `listening=true` no voice agent — usuário precisa dizer "Jarvis" de novo.
- **Impacto:** UX — barge-in deveria voltar direto para escuta.

#### GAP 9: AudioPipelineAgent e VoiceAgent duplicam VAD
- **Problema:** Ambos instanciam `VAD::new(500.0, 16000)` separadamente — 2 VADs processando o mesmo stream.
- **Impacto:** CPU desperdiçada + resultados potencialmente inconsistentes.

---

## 4. PLANO DE CORREÇÃO — 5 FASES

### Fase 1 — Fundação (GAP 1 + 2)
1. **QEMU Audio Sim:** quando HDA indisponível, gerar `AUDIO_IN` com silêncio + sine wave de teste a cada N ticks
2. **Wake Window fix:** pausar decremento quando `pending_user_text.is_some()` ou HERMES_RESPONSE pendente

### Fase 2 — Streaming TTS (GAP 3 + 4)
1. **TTS streaming:** split resposta em frases, sintetizar por frase, push no PLAYBACK_RING progressivamente
2. **Orb feedback:** mixer alimenta `process_audio_fft` com playback samples (não só mic)

### Fase 3 — Qualidade (GAP 5 + 6 + 7)
1. **Emotion decay:** EWMA (α=0.3) no LAST_VOICE_EMOTION em vez de substituição direta
2. **Context window:** aumentar para 6 turnos + sumarização dos anteriores
3. **Piper smoke test:** sintetizar "hello" no boot, validar amplitude > threshold

### Fase 4 — UX (GAP 8 + 9)
1. **Barge-in → listen:** ao limpar playback, setar `wake_window = wake_listen_ticks()` no voice agent
2. **VAD compartilhado:** único VAD em voice agent, pipeline lê dele em vez de instanciar outro

### Fase 5 — NSGDB Integration
1. **DataCollector** já captura pares (USER_INTENT/HERMES_RESPONSE) — validar que persiste
2. **Semantic recall** no LLM context: quando LLM recebe USER_INTENT, buscar memórias relevantes no SGDB

---

## 5. MAPA DE FLUXO COM FIXES

```
                    ┌──────────────────────┐
                    │   HDA SD0 (MIC)      │
                    │   poll_hda_audio()   │
                    └──────────┬───────────┘
                               │ AUDIO_IN
                    ┌──────────▼───────────┐
                    │  [F1] QEMU SIM FALLBACK│ ← novo: gera silêncio/sine
                    └──────────┬───────────┘
                               │
              ┌────────────────┼────────────────┐
              ▼                                  ▼
     WakeWordAgent                      JarbasVoiceAgent
     (MLP + pattern)                   (VAD compartilhado [F4])
              │                                  │
              ▼                                  ▼
     WAKEWORD ──────────────────► wake_window [F2: pausa se pending]
              │                                  │
              ▼                                  ▼
     [gate]                          STT (MFCC→LSTM→CTC)
                                             │
                                             ▼
                                    USER_INTENT
                                             │
                            ┌────────────────┼────────────────┐
                            ▼                                  ▼
                     HermesAgent                          JarbasAgent
                     (parse_command)                      (LLM_REQUEST)
                     (SGDB recall [F5])                          │
                            │                                    ▼
                            │                          LLM processes
                            │                                    │
                            ▼                                    ▼
                     HERMES_RESPONSE ◄───────────────────────────┘
                            │
              ┌─────────────┼──────────────┐
              ▼             ▼              ▼
       DisplayAgent  JarbasVoiceAgent   DataCollector
       (HUD/Chat)    [F2: TTS streaming] (SGDB persist)
                            │
                            ▼
                   Piper/Formant TTS
                   (por frase [F2])
                            │
                            ▼
                   PLAYBACK_RING
                            │
              ┌─────────────┼──────────────┐
              ▼                            ▼
       AudioMixerAgent              Orb FFT [F2: playback→fft]
       (volume + drain)
              │
              ▼
       HDA SD1 (SPEAKER)
```

---

## 6. QUESTÕES PENDENTES

1. **Piper VITS completo:** O engine atual é "neural-lite" (embedding + oscilador). HiFi-GAN real (~15M params) requer implementação de transposed convolutions em no_std — sprint futura.

2. **STT vocab limitado:** 28 chars (a-z + space + blank). Números, pontuação, acentos não reconhecidos. Treino expandido é URGENTE para PT-BR real.

3. **UAC (USB Audio):** `poll_uac_audio()` existe mas QEMU não emula UAC — teste requer HW real.

4. **Latência total estimada (com fixes):**
   - Wake detect: ~200ms (16 frames × 320 samples @ 16kHz)
   - STT: ~500ms (MFCC + LSTM forward)
   - LLM: ~2-5s (BitNet 2B soft-float) ou ~500ms (Falcon3 GPU)
   - TTS streaming: ~200ms (primeira frase) + ~100ms/frase
   - **Total percebido: ~1-2s** (com streaming TTS) vs ~4-6s (sem streaming)
