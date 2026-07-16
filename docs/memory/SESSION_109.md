# SESSION 109 — ADR-0045 Sound Voice Stack (docs sync)

**Data:** 2026-07-16  
**Sprint:** 107  
**Tipo:** Documentação / decisão arquitetural (sem mudança de runtime)

## Objetivo

Gravar o estado real do stack de áudio/voz como ADR e alinhar IDEA_BANK / STATE / TECNOLOGIAS / CHANGELOG — desativar caminhos obsoletos (sherpa, Vosk, Kokoro-primário, Wyoming, Rustpotter) sem apagar histórico.

## Decisões

1. **ADR-0045** (`docs/architecture/0045-sound-voice-stack.md`) — Accepted.
2. Truth = `neural-kernel/src/audio/*`; `jarbas/audio` = espelho de migração não wired.
3. Primário: HDA + Piper (+formant) + STT CTC + VAD + mixer + FB TTS paint.
4. WakeWord: código sim, registro no AgentFleet **não**.
5. UAC (#84): stub futuro — mantido 🟡.
6. B-01 não bloqueia voz; B-01 NIC morto via SLIP (#415).

## IDEA_BANK tocado

#75 ✅ · #83 ✅ · #84 🟡 futuro · #315.21–25 ❌/✅ parcial · #315.N+1 ❌ · #360 ❌ · bloco 29+ B-01 notas.

## Versão

CHANGELOG **[1.7.1]** docs-only; Cargo package permanece `1.0.0`; tag `v1.7.1` alinhada ao hábito 1.5.7 / 1.7.0.

## Não feito nesta sessão

- Registrar WakeWordAgent  
- Fix Piper `emb.weight`  
- Wiring binário → `jarbas::audio`
