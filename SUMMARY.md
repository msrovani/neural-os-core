# Neural OS Hermes — AI Summary

**O que é:** Um sistema operacional bare-metal (`no_std` Rust, sem Linux) onde TUDO é um Agente ou uma Skill. 247+ agentes cooperativos, com MoE (Mixture of Experts) integrado ao LLM. O kernel roda IA, mas **é** IA — cada hardware é identificado por rede neural, cada decisão de alocação é inferida pelo modelo ternário BitNet.

**Estado atual (v2.0.0):** ~26.000 LOC, 180+ arquivos Rust, 0 erros. **Sprint 106 concluída** — workspace K²CHJ com 5 crates (k_nano, k_ai, cortex, hermes, jarbas). MicroPython/WASM sandbox, SkillOpt, SOUL.md via VFS. ATA PIO bug corrigido. HW Expert v3 (61.453 VID/DID). SelfHealing I3/I4. WiFi Intel AX200. 3 camadas visuais. HDA capture+playback.

**Arquitetura K²CHJ (v2.0):**
- **k_nano** (Ring 0): HAL, drivers, PCI, memory
- **k_ai** (Ring 1): SelfHeal, Trust, sondagem
- **cortex** (Ring 2): BitNet, Trinity MoE, BPE
- **hermes** (Ring 2): WASM, MicroPython, rede, skills
- **jarbas** (Ring 2): Display, áudio, persona JARVIS
- **neural-kernel**: bin de integração (migração gradual do monólito)

**Arquitetura chave:**
- 247+ agentes (20 nativos + 147 The Agency + ~80 importados + HW + FS)
- Trinity MoE Router com 6 experts + router_weight treinado
- BitNet-b1.58 850M (GQA, BitFFN) + Medusa + KV Cache
- Memory Tree v2 + Knowledge Graph bitemporal
- SDIO MoE: 95.812 entradas .inf/.sys reais
- Ed25519 identity + TPM 2.0 + Safety I1-I4
- WASM + MicroPython sandbox + SkillOpt (Python→Rust no_std)

**Próximo:** Sprint 107 — Voice I/O Pipeline (TTS→STT→LLM→TTS)

**Para IA que vai me editar:**
1. Leia `docs/TECNOLOGIAS.md` — catálogo completo de todas as tecnologias
2. Leia `AGENTS.md` — regras operacionais e plano diretor
3. Leia `docs/memory/STATE.md` — estado atual detalhado
4. Leia `TODO.md` — itens pendentes reais

**Stack:** Rust nightly, x86_64-unknown-none, bootloader 0.11.15, smoltcp 0.13, ed25519-compact. QEMU + VirtualBox para dev, HW real para validação.

> "We don't need an OS that runs AI. We need an OS that IS AI."
