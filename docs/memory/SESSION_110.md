# SESSION 110 — Sprint 107 Loops 1–5 (clima e2e + Voice milestone)

**Data:** 2026-07-16  
**Sprint:** 107  
**Tipo:** Runtime milestone + docs/version **v1.7.2**  
**Log canônico:** `logs/boot_whpx_20260716_033322.txt` (Loop 5)

## Objetivo

Fechar evidência honesta dos loops 1–5 do clima e2e (WHPX + bridge), versionar **1.7.2**, e listar gaps restantes sem declarar `v2.0.0`.

## Evolução dos loops

| Loop | GEN texto | Notas |
|------|-----------|-------|
| L1 | panic STT | path STT quebrava boot |
| L2 | `' tempo Tempo dia'` | weatherish parcial |
| L4 | `' tempo esta bom'` | PT climática parcial |
| **L5** | **`'O tempo esta'`** | `decoded_len=12`; logits reais + máscara posicional; **não** canned |

## Resultado Loop 5 (PASS parcial forte)

| Item | Estado |
|------|--------|
| Bridge WHPX | ✅ `run_weather_e2e.ps1` `-Window -Smp 2`; kill ~18 min |
| GEN | ✅ frase PT climática `'O tempo esta'` |
| TTS | ✅ Piper **neural-lite** (`emb.weight` vocab=256) · `pcm_samples=15428` |
| FB | ✅ `[JARBAS-TTS-FB] painted len=12 1280x800` |
| STT | ▶️ CTC LOADED 10 tensors 55K; path real mas `ctc=''` → seed prompt |
| WakeWord | ✅ `WakeWordAgent` **registrado** no AgentFleet |
| Experts | ✅ RUSTCODER · STT · BGE · ❌ HWEXPERT parse FAILED (header vocab u16) |
| Soft-float | ❌ known blocker — tkn/s baixo; **sem “fix” falso** nesta sessão |

**Veredito:** meta clima 1+2+3+6 atendida parcialmente; loop TTS↔STT↔LLM ainda usa seed quando CTC vazio.

## Gaps Sprint 107 (pós-1.7.2) — ordem de trabalho

1. Soft-float latency (tkn/s) — **SKIP / known blocker**
2. STT CTC empty (`ctc=''`)
3. Generate livre PT (máscara mais frouxa, soft-float budget)
4. Mic→WakeWord→STT→LLM→TTS EventBus real
5. Piper VITS pleno (além de neural-lite)
6. Unificar TTS Continuous (`JarvisVoice` → `synthesize_tts`)
7. UAC além de stub puro
8. HW Expert parse FAILED (re-export header u32)
9. `jarbas/audio` wired ao bin (ou passo incremental)
10. Doc drift WakeWord (corrigido nesta sessão)
11. Docs + version + commit loops — **este milestone**

## Ops

- `CARGO_TARGET_DIR=repo\target` + `cargo nk` + `cargo build --release -p boot`
- Piper: `python tools/convert_piper_to_bitnet.py` → `target/PIPER_PT_BR.BIN`
- BPE: `target/bpe_vocab.bin` (BPB1) @ `0x150000000`
- HW Expert: `target/hw_expert_v3.bitnet` @ `0x160000000` — magic OK, parse FAIL (vocab)

## Versão

CHANGELOG **[1.7.2]**; tag anotada `v1.7.2`; Cargo package `neural-kernel` permanece `1.0.0`.
