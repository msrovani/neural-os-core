# SESSION_129 — Consolidação v1.8.5 (teste)

**Data:** 2026-07-16  
**Versão:** v1.8.5  
**Canal:** teste / não estável  
**Base:** v1.8.0

## Objetivo

Consolidar, memorizar, documentar e versionar todo o trabalho realizado após o
marco K²CHJ v1.8.0, sem declarar v2.0.0 e sem promover os MVPs para produção.

## Entregas pós-v1.8.0

| Bloco | Resultado |
|---|---|
| Sprint 108 | Self-Evolve: observe→generate→verify→improve→reflect; skill verification; SIL e SleepCycle REFLECT |
| Sprint Sound | Mic→Wake→STT→LLM→TTS, STT PCM→MFCC, UAC descriptor parse, VAD/SER e Piper neural-lite |
| NeuralFS / ADR-0040 | I/O RAM, B-tree leaf com reclaim/split, ATA MBR opcional, exFAT read-MVP e MHI soft-migrate |
| Cortex | N-gram speculative decode + benchmark empírico e KV rollback |
| ADR-0047 | LatentBus, Evolve hot-swap/Genesis, NeuOS Probe, GPU work queue/SASOS/H2O/G5 e HMI embedding/splats |
| ADR-0046 | AirLLM GGUF layer-wise, prefetch soft, hot-swap ATA e Net→FAT→`set_model` |
| GPU futura | ADRs propostas 0048–0050 para NVIDIA, AMD e Intel multigeração; permanecem `por_fazer` |

## Limites e riscos preservados

- Release **não estável**, destinada a integração e testes.
- Soft-float/VITS, CTC WER, UAC isócrono e cutover `jarbas::audio` continuam abertos.
- NeuralFS físico/multinível e writes exFAT/NTFS/EXT exigem validação em hardware real.
- AirLLM mantém prefetch soft, staging Net em RAM e dependência de RX funcional.
- ADR-0047 GPU/HMI contém MVPs/PoCs; metas de latência e aceleração HW não foram comprovadas.
- ADRs 0048–0050 são propostas, não implementações.
- `v2.0.0` continua bloqueada por demandas `por_fazer`, review formal e aprovação explícita do maintainer.

## Versionamento

- Política histórica preservada: versão de produto por documentação/tag; `Cargo.toml` do bin permanece `1.0.0` (tag-only).
- Marco estável anterior: **v1.8.0**. Tree atual: **v1.8.5 (teste / não estável)**.
- Não commitar `target/`, imagens boot, nem binários de modelo gerados localmente.
- Build (`cargo nk`), commit, tag `v1.8.5` e push: pendentes enquanto o executor de shell desta sessão não retornar status.

