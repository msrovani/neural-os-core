# SESSION_352 — Auditoria do pipeline de voz (o que sobrou depois da s346)

**Data:** 2026-09-16
**Sprint:** s352
**Base:** `main` @ `050e1018` (v1.9.99-s351)
**Objetivo:** com a pergunta "o que ainda podemos melhorar em todo o processo de voz?", ler o caminho real
de ponta a ponta e listar o que falta — **sem alterar código**.
**Artefato:** `docs/implementation/2026-09-16-voice-pipeline-audit-s352.md` (achados + evidência + ondas V1–V5).

---

## Método

Leitura dos 7 módulos do caminho (`capture.rs`, `vad.rs`, `wakeword.rs`, `voice.rs`, `stt.rs`, `mixer.rs`,
`k_nano/src/audio/hda.rs`) + `event-bus/src/bus.rs`, com **aritmética de taxa/clock** em vez de opinião.
Nenhum arquivo de código foi tocado nesta sessão.

## Achados (top, por ordem de dano)

1. **Playback é drenado por tick, não pelo clock do áudio.** `mixer.rs` puxa 1024 amostras/tick;
   `write_hda_playback` é LPIB-aware e **descarta** o excedente (`PLAY_SAMPLES_DROPPED`). A 60 Hz o
   mixer puxa 61 440/s contra 16 000/s do dispositivo → rajadas maiores que o buffer do DMA
   (**341 ms** = 1024 × 16 entradas) perdem até **~74% da fala**; a 120 Hz, ~87%; abaixo de ~15,6 Hz
   vira starvation. **E o ADR-0104 tornou o tick adaptativo** — a taxa da UI muda a fidelidade do áudio.
2. **Auto-barge-in.** VAD `start` com `PLAYBACK_RING.available() > 0` → `request_interrupt()`, que
   limpa a fala **e** cancela a inferência. Não há AEC no repositório: em HW real o assistente se
   interrompe (e destrói a própria resposta). O piso do VAD ainda adapta para o próprio eco.
3. **`STT_UNCERTAIN` sem consumidor.** A honestidade da s346 (não inventar texto) existe na origem mas
   não chega ao usuário: incerteza = silêncio.
4. **Estado ilimitado.** `pcm_buffer` sem teto + `logits = n_frames × vocab` (768 KB p/ 30 s) — a mesma
   classe de cliff de s339/s349/s351.
5. **Wake-word: janela errada e sem informação espectral.** O MLP lê `energy_history[..16]` (os 16 mais
   **velhos** de 1,28 s) e só é avaliado no fim da janela → latência até 1,28 s; o detector real é o
   padrão de 2 picos. O "98,4%" do comentário não tem script nem dataset no repo.
6. **Áudio de 50 Hz no barramento de controle.** `publish()` clona o payload por assinante num
   `VecDeque` ilimitado → ≥100 alocações/s e vazamento se um consumidor parar de drenar.
7. **Sem orçamento de latência** (wake→áudio) e **contexto de diálogo sem limite** (clamp de ctx pode
   cortar a pergunta atual); histórico RAM-only; e a voz publica tudo com `Legacy(1)` — sem identidade
   nem HITL para skills privilegiadas.

## Estado do aceite (herdado)

PHASE 6/7 não chegam no boot 8c — stall pré-existente em `K33[28] sgdb` (frente pins/FAT/tickv).
Consequência: nem os contadores de captura da s346 nem `VOICE_STATE` têm onde aparecer hoje; o aceite
das ondas V1–V3 é teste host + boot mínimo, ou espera o destravamento.

## Correção de doc

A lição "QEMU não tem codec HDA — áudio é metal-only" (AGENTS.md) foi **provada errada** no bring-up da
s346: o QEMU sobe `hda-duplex` (vendor `0x1af40022`) e os 4 bugs eram do driver (CRST invertido, ICW
sem kick, verbo em [11:0], enumeração parando no root). Substituída pela lição correta — o mesmo texto
já estava no PR #16 (`fix/hda-bringup-s346`), para o merge não conflitar.

## Aberto

- Ondas V1–V5 do plano (nenhuma implementada).
- PR #16 (bring-up HDA) segue aberto e é a **base** de V1: sem o caminho de playback armando SD1, os
  contadores de pacing não existem.
