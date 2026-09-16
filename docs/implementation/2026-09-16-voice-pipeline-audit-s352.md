# Voice Pipeline — Auditoria e Plano de Ataque (SESSION_352)

**Data:** 2026-09-16 · **Base:** `main` @ `050e1018` (v1.9.99-s351)
**Escopo:** `wake-word → mic → STT → emoção → LLM → resposta → TTS → alto-falante`
**Método:** leitura do caminho real (7 módulos) + aritmética de taxa/clock. **Nenhuma linha de código alterada nesta sessão.**
**Estado de referência:** s346 entregou o dono único do mic, frames fixos, VAD único, barge-in por geração, STT honesto (sem texto inventado). Esta auditoria é o que **sobrou**.

Convenção: cada item tem **evidência** (arquivo:linha), **efeito** e **aceite**. Onde o aceite é metal-only, está dito.

---

## Onda V1 — Playback: o clock do dispositivo tem de mandar (2 itens)

### V1.1 O mixer drena por *tick*, não por *clock* de áudio
- **Evidência:** `crates/jarbas/src/audio/mixer.rs:56-62` — `pop(&mut buf[..1024])` a cada tick e `write_hda_playback(&buf[..n])`. O lado do dispositivo é correto e honesto: `crates/k_nano/src/audio/hda.rs:1459-1472` calcula `free_frames` por **LPIB** e, quando não cabe, **descarta** contabilizando `PLAY_SAMPLES_DROPPED`.
- **Efeito (aritmética):** o TTS produz ~16 000 amostras mono/s (o dispositivo consome exatamente isso = 48 000 frames estéreo/s). Com tick a 60 Hz o mixer **puxa** 61 440/s; o produtor (síntese por frase) entrega em rajadas de 0,3–3 s. Uma rajada maior que o buffer do DMA (**341 ms** = `FRAME_PER_ENTRY 1024 × BDL_ENTRIES 16`) enche o anel do DMA → `to_write < need` → **~74% da fala descartada a 60 Hz**; a 120 Hz piora (~87%); abaixo de ~15,6 Hz inverte para *starvation* (silêncios). O equilíbrio existe por coincidência numérico (1024 × 15,6 ≈ 16 k), não por desenho.
- **Agravante:** `ADR-0104` tornou o tick **adaptativo (30/60/120 Hz)** — ou seja, a taxa de ticks da UI altera silenciosamente a fidelidade do áudio.
- **Fix:** o mixer puxa **apenas o que o dispositivo aceita** (expor `free_frames` de `k_nano::audio::hda` e puxar `min(free, ...)`), ou puxar `expected = (now_us - last_us) * VOICE_RATE / 1e6` por TSC. Starvation → preencher com silêncio, nunca rajada.
- **Aceite:** com `/tick 30`, `60` e `120`: `PLAY_SAMPLES_DROPPED == 0` e TTS contínuo. QEMU dá o contador; **a audição é metal**.

### V1.2 Upsample por *zero-order hold* na rota de escrita
- **Evidência:** `hda.rs:1475-1486` — `for _ in 0..VOICE_DECIM { write(s); write(s) }` (hold ×3).
- **Efeito:** imaging/alias audível em sibilantes; pior ainda porque o TTS fala 16 kHz quando modelos Piper costumam ser 22,05 kHz e o dispositivo roda 48 kHz.
- **Fix:** sintetizar na taxa do dispositivo (48 kHz) **ou** um resample linear/polifásico uma vez no DSP, fora do laço de escrita do driver.

---

## Onda V2 — Não destruir o turno do usuário (2 itens)

### V2.1 Auto-barge-in: o mic escuta o próprio alto-falante
- **Evidência:** `crates/jarbas/src/audio/voice.rs` (ramo `VAD start`) — `if PLAYBACK_RING.available() > 0 { publish_state(BargeIn); request_interrupt(); settings::force_wake_open(); }`. `request_interrupt()` limpa o ring, incrementa `TTS_GENERATION` **e chama `cortex::infer_queue::cancel_active()`**. Não existe AEC nenhum no repositório (grep: zero ocorrências).
- **Efeito:** em HW real com alto-falante, a própria voz do assistente cruza o mic acima do piso do VAD (o piso adaptativo converge para o *próprio* eco) → o assistente **se interrompe** e, pior, cancela a inferência que estava produzindo a resposta. O usuário perde a resposta e ouve silêncio.
- **Fix mínimo (sem DSP):** política de duplexidade como dado, não como acidente — (a) *half-duplex* por padrão: ignorar `AUDIO_FRAME`/VAD enquanto `PLAYBACK_RING.available() > 0` ou <150 ms depois de esvaziar; (b) barge-in como *opt-in* em `settings` + gate de energia **acima** do piso de eco medido na primeira fala (calibração em runtime, não constante); (c) full-duplex só com AEC. **Não** cancelar a inferência no caminho de eco — interromper fala ≠ abortar raciocínio.
- **Aceite (metal):** `BARGE_IN_COUNT` só incrementa com fala real; nenhuma TTS é truncada por ela mesma; `infer_cancel` só em barge-in confirmado.

### V2.2 `STT_UNCERTAIN` é publicado e ninguém escuta
- **Evidência:** `crates/jarbas/src/audio/stt.rs:769` publica `STT_UNCERTAIN`; grep de consumidores = **zero**.
- **Efeito:** quando a transcrição não passa no limiar de confiança, o usuário por voz recebe **silêncio**. A correção de honestidade da s346 (não inventar texto) está feita na origem, mas não chega ao usuário. `[vazio]`/`[stt-offline]` vão para `STT_TEXT` (janela de chat, `display/agent.rs:212`) — quem fala não vê.
- **Fix:** consumir `STT_UNCERTAIN` no `JarbasAgent` → TTS curto ("Não entendi, pode repetir?") + linha no chat. É o HITL honesto que a premissa 0088 pede.

---

## Onda V3 — Estado ilimitado e topologia de áudio (2 itens)

### V3.1 `pcm_buffer` sem teto + matriz de logits CTC
- **Evidência:** `voice.rs` (acúmulo de frames) — cresce enquanto `listening`, e a única saída é `VAD end` (15 frames de silêncio). `stt.rs:640-653` copia (`pcm.to_vec()`) e aloca `logits: vec![0.0f32; n_frames_total * vocab]`.
- **Efeito:** ruído contínuo (ventilador, TV) → `Vec` sem limite; uma fala de 30 s → `logits` = 750 frames × 256 × 4 B ≈ **768 KB** (e 3 MB para 60 s) — exatamente a classe de cliff já paga em s339/s349/s351 (`capacity overflow`, OOM no `raw_vec`).
- **Fix:** (a) teto de utterance (`MAX_UTTERANCE_S ≈ 12`) → endpoint forçado + `VOICE_STATE` + log; (b) CTC *greedy* não precisa da matriz: basta a **emissão anterior** (collapse de repetições) → elimina `n_frames × vocab` por construção.
- **Aceite:** monólogo de 30 s com RAM plana e log de endpoint forçado; teste host com 2 utterances (forçada + normal).

### V3.2 Áudio de 50 Hz no barramento de controle
- **Evidência:** `crates/event-bus/src/bus.rs:59-63` — `publish()` faz `event.clone()` **por assinante** em `VecDeque` **ilimitado**. `AUDIO_FRAME` (640 B, ~50/s) tem ≥2 assinantes (`wakeword`, `voice`) → ≥100 alocações/s; `capture.rs` monta o payload com `flat_map(...).collect()` (Vec novo a cada 20 ms).
- **Efeito:** churn de heap no caminho quente e **vazamento sem limite** se um consumidor parar de drenar (agente dormente/rebaixado a EventDriven — a própria política de "Continuous não-essencial >5%").
- **Fix:** áudio é **stream, não evento**: publicar frames num ring SPSC (consumidor único = sessão de voz) e deixar o barramento para controle (`VAD_TRANSITION`, `VOICE_STATE`, `WAKEWORD`). Double-buffer estático mata o `collect()` por frame.

---

## Onda V4 — Wake-word de verdade (1 item, 3 defeitos)

- **V4.1 Latência e janela errada.** `wakeword.rs:154-181`: o MLP é avaliado **só quando a janela de 64 frames enche**, lendo `energy_history[..16]` — os **16 mais VELHOS** de 1,28 s — e em seguida a janela é zerada. Latência de detecção até **1,28 s** e o classificador nunca vê os frames recentes (o "JARBAS" que acabou de ser dito). **Fix:** janela deslizante dos últimos 16 frames + avaliação a cada 4 frames (≤80 ms).
- **V4.2 O classificador não tem informação espectral.** As 16 entradas são RMS — não separam "JARBAS" de qualquer frase de 2 picos; o detector de fato é `detect_wakeword_pattern` (2 picos de energia com ≥3 frames de separação, `wakeword.rs:110-133`). O comentário afirma "98,4% de acurácia (2000 jarvis + 8000 não-jarvis)" **sem script nem dataset no repositório** (`tools/` tem só STT/Piper) — mesma classe de overclaim do corpus sintético de STT (s346). **Fix:** `tools/train_wakeword.py` com log-mel/MFCC + corpus de confusáveis PT-BR ("serviço", "alvará", "já vi", "jarra") + FPPH medido em holdout **disjunto**; registrar o modelo.
- **V4.3 Instrumento morto:** `last_score_log` é escrito e nunca usado (`wakeword.rs`) — o score do MLP não aparece em lugar nenhum, então não há como julgar a sensibilidade em campo.

---

## Onda V5 — Medir, memorizar, confiar (3 itens)

- **V5.1 Não existe orçamento de latência de voz.** Nenhum estágio mede: wake→VAD start→end→transcrição→1º token→1ª amostra de TTS→alto-falante. O projeto tem TSC/telemetria (`frame_cost_us`, InferQueue), e a rota mais sensível a UX não tem número — nem dá para julgar o endpointing atual (5 frames p/ entrar = 100 ms; 15 p/ sair = 300 ms). **Entregável:** `VOICE_LATENCY` (deltas por estágio) + linha no HUD; alvo proposto: wake→1º áudio ≤1,5 s (LLM domina), endpoint→transcrição ≤400 ms.
- **V5.2 Contexto de diálogo sem orçamento.** `voice.rs::deliver_transcript` prefixa os últimos 3 turnos sem limite de tamanho; o LLM clampa ctx (512 no Falcon3 1,58-bit do lab / 4096 heavy — SESSION_349). Com 3 turnos longos, o clamp corta **a pergunta atual** (ou o tail). **Fix:** orçamento por chars/tokens, descartando os turnos **mais antigos** primeiro — nunca o turno corrente. E o histórico é RAM-only (10 turnos): a premissa AIOS manda memorizar → gravar turnos no SGDB episódico (ADR-0063) para sobreviver a reboot.
- **V5.3 A voz não tem identidade própria.** `voice.rs:274,289,313` publica `USER_INTENT`/`STT_TEXT` com `CapabilityToken::Legacy(1)` — um comando falado é indistinguível de um evento interno confiável, e skills privilegiadas (instalar/formatar/shell) podem ser disparadas por voz sem confirmação nem verificação de locutor. Contra a própria doutrina ("Trust por `(token, agent, skill)`"). **Fix:** identidade `voice_input` no token + HITL de confirmação para skills privilegiadas; verificação de locutor fica como item 💰 (depende de modelo).

---

## Itens menores (baratos, mesma passada)

| Item | Evidência | Ação |
|---|---|---|
| Doc drift do módulo | `jarbas/src/audio/mod.rs` ainda documenta `AudioPipelineAgent` e `MIC_CAPTURE_RING` (removidos na s346) | reescrever o cabeçalho com o fluxo real |
| `wake_window.min(120)` | `voice.rs` (`deliver_transcript`) | confirmar intenção (parece typo de clamp) e comentar |
| Saída dupla | `mixer.rs` escreve em HDA **e** UAC a cada tick | escolher o endpoint ativo (senão fala dobrada quando há headset + on-board) |

---

## Ordem de execução e aceite por onda

| Onda | Conteúdo | Verificação | Metal? |
|---|---|---|---|
| V1 | pacing por clock + resample | `/tick 30/60/120` com `PLAY_SAMPLES_DROPPED == 0` (QEMU dá o contador) | audição = HW |
| V2 | política de duplexidade + `STT_UNCERTAIN` falado | `BARGE_IN_COUNT` só com fala real; resposta falada no "não entendi" | sim |
| V3 | teto de utterance + CTC incremental + ring p/ áudio | RAM plana em 30 s; alocações/frame medidas | QEMU |
| V4 | wake-word (janela, modelo, script) | FPPH + latência ≤80 ms medida no host | HW (mic real) |
| V5 | latência medida + orçamento de contexto/SGDB + identidade | `VOICE_LATENCY` nos logs + turnos no SGDB + confirmação HITL | QEMU+metal |

**Bloqueio conhecido:** PHASE 6/7 não chegam no boot 8c (stall pré-existente em `K33[28] sgdb`, frente pins/FAT/tickv). Enquanto durar, **V1–V3 não têm onde aparecer no 8c** — o aceite vira teste host + boot mínimo com áudio forçado, ou espera o destravamento.
