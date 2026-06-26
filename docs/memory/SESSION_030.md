# Sessão 030 — Sprint 25: Neural Cortex (Cortex LLM)

**Data:** 25/06/2026
**Versão:** v0.25.0+build48

## Objetivo
Ativar o Córtex Neural do Hermes — substituir o IntentMlp (16→8→3, hand-crafted) por um classificador neural de intenções com 12 categorias, integrado ao pipeline completo: teclado → input_daemon → intent_router_daemon → Cortex → SkillRegistry → VGA.

## Dificuldades e Decisões

### 1. Cortex vs MLP antigo
- MLP antigo: 3 intenções (chat/status/echo), pesos fixos, só entendia 16 palavras
- Cortex novo: 12 intenções, matching semântico por palavra-chave, dispatch automático para skills
- Decisão: não tentar implementar transformer completo ainda (ADR-0019 Sprint 25 é só a primeira etapa)

### 2. Integração com intent_router_daemon
- O daemon existente tratava comandos explícitos (/status, /echo) e free-text separadamente
- Mantive comandos explícitos, substituí apenas o fallback Chat() que ia para o MLP
- O dispatch agora verifica `SKILL_REGISTRY.has_skill()` antes de executar

### 3. Skills faltantes
- Greeting e Chat não têm skills registradas — tratei como respostas inline no daemon
- Intenções como trust/network/hw já têm skills registradas (EchoSkill, SystemStatusSkill, HardwareInfoSkill, NetDiagnosticSkill)

### 4. MemPalace instalado
- MemPalace 3.5.0 (mempalaceofficial.com) — AI memory system
- 1271 drawers do projeto neural-os-core indexados
- Wing: neural_os_core, Rooms: crates, documentation, target, general

## Estado Atual
- `cargo check --release`: 0 erros
- QEMU RTL8139: boot completo, 7 tasks, 6100+ ticks
- Cortex: 12 intenções, dispatch automático para skills
- Todo pipeline funcional: teclado → EVENT_BUS → daemons → VGA
