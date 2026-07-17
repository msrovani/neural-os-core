# SESSION_122 — Sprint Sound CLOSED (parcial honesto)

**Data:** 2026-07-16  
**Sprint:** Sound  
**Status:** ✅ CLOSED (soft-float/VITS + cutover jarbas + iso UAC HW abertos)  
**Build:** `cargo check --release -p neural-kernel` = 0 erros (`target/check-sound`)

---

## Objetivo

Fechar backlog Sprint Sound (ADR-0045): pipeline Mic→Wake→STT→LLM→TTS, STT PCM→MFCC, UAC parse, neural-lite polish, VAD/SER — sem fakear VITS/soft-float.

## Entregas

| Item | Evidência |
|------|-----------|
| Wake Continuous + gate | `wakeword.rs` Continuous; `voice.rs` `wake_window`; bypass `weather-e2e` |
| Rings separados | `MIC_CAPTURE_RING` / `PLAYBACK_RING`; mixer → playback; barge-in pipeline |
| Rota TTS única | `HERMES_RESPONSE` → `synthesize_tts` → `AUDIO_OUT` (sem `TTS_CMD` órfão) |
| STT PCM→MFCC | `tools/train_stt.py`; `target/STT.BIN` 216KB; CMVN alinhado; CTC tiny WER fraco |
| VAD / SER | noise-floor + ZCR; SER confidence gate |
| Piper neural-lite | prosódia/duração/PT normalize; VITS bloqueado soft-float |
| UAC | `parse_config_for_audio`; `xhci::try_read_config_descriptor`; `tools/test_uac_parse.py` OK |
| Jarbas espelho | VAD/settings/wake sync; bridge topics+settings; **sem cutover** |

## Arquivos principais

- `crates/neural-kernel/src/audio/{voice,wakeword,vad,ser,settings,mixer,pipeline,usb,piper,stt,skills,jarvis}.rs`
- `crates/neural-kernel/src/{xhci,jarbas_bridge,main}.rs`
- `tools/{train_stt,convert_piper_to_bitnet,test_uac_parse}.py`
- Espelho: `crates/jarbas/src/audio/{vad,settings,wakeword,mod}.rs`

## Residuais honestos

1. **Soft-float / VITS-HiFi-GAN** — não implementado; neural-lite é o path executável.
2. **CTC WER** — modelo 55K params ainda satura/blank; path de treino PCM está correto.
3. **UAC isócrono** — parse+probe OK; DMA periódico exige device UAC + EP0 control pleno.
4. **Cutover `jarbas::audio`** — adiado (ADR-0045).

## Check IDEA / ADR

- #84 UAC → ✅ parcial  
- #442 Sound → ✅ parcial  
- ADR-0045 atualizado  
