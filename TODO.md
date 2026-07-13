# 📋 TODO MASTER — neural-os-core v1.5.3

**Data:** 2026-07-13  
**Propósito:** Checklist mestre do roadmap v1.5.x → v2.0.  
**Documento oficial:** AGENTS.md (seção roadmap)  
**Legenda:** ✅ feito | 🟡 em andamento | 🔴 bloqueado | ⏳ agendado

---

## ✅ SPRINTS 1-105 — COMPLETOS

| Sprint | v | Foco | LOC | Status |
|--------|---|------|-----|--------|
| 1-100 | v1.0.0 | Gold Master — Code Freeze + Release | ~26.000 | ✅ |
| 101 | v2.0 | Cognição: TTS, STT, HDA capture, ATA fix, NVIDIA GPU | ~2.000 | ✅ |
| 102 | v1.1.x | GPU Compute, HW Expert v3, Firmware Pipeline, WiFi | ~1.500 | ✅ |
| 103-104 | v1.5.0 | K²CHJ Workspace Migration (5 crates) | ~500 | ✅ |
| 105 | v1.5.1 | Ponytail Audit: ~600 LOC removidos, 11 deps eliminadas | ~100 | ✅ |
| 105b | v1.5.2 | RingBufStore refactor + LEGACY snapshot | ~50 | ✅ |
| 105c | v1.5.3 | K²CHJ crate dead code cleanup + PICS fix | ~50 | ✅ |

---

## ▶️ SPRINT 106 — v2.0 Cognição: LLM Agent 24/7

| Item | LOC | Status |
|------|-----|--------|
| Multi-turn conversation (Cortex + Hermes) | ~800 | ⏳ |
| Persistent agent state across reboots | ~400 | ⏳ |
| LLM-powered Hermes CLI | ~300 | ⏳ |
| Auto-skill generation from natural language | ~500 | ⏳ |

## ▶️ SPRINT 107 — v2.0 Voice I/O Pipeline

| Item | LOC | Status |
|------|-----|--------|
| TTS→STT→LLM→TTS loop | ~600 | ⏳ |
| Voice activity detection improvements | ~200 | ⏳ |
| Wake word "Jarvis" refinements | ~200 | ⏳ |
| Audio pipeline hardening | ~300 | ⏳ |

## ▶️ SPRINT 108 — v2.0 Self-Evolving Agents

| Item | LOC | Status |
|------|-----|--------|
| Auto-skill generation via LLM | ~500 | ⏳ |
| Runtime skill verification | ~300 | ⏳ |
| Agent self-improvement loop | ~400 | ⏳ |
| Meta-cognition and reflection | ~400 | ⏳ |

---

## 📊 RESUMO v2.0 "Cognição"

| Sprint | Foco | LOC | Status |
|--------|------|-----|--------|
| 100 | Code Freeze v1.0.0 | ~500 | ✅ |
| 101 | TTS+STT+ATA fix+NVIDIA GPU | ~2.000 | ✅ |
| 102 | GPU Compute + HW Expert v3 + Firmware | ~1.500 | ✅ |
| 103-104 | K²CHJ Workspace Migration | ~500 | ✅ |
| 105 | Ponytail Audit + v1.5.1..v1.5.3 | ~200 | ✅ |
| 106 | LLM Agent 24/7 multi-turn | ~2.000 | ⏳ |
| 107 | Voice I/O Pipeline (TTS→STT→LLM→TTS) | ~1.500 | ⏳ |
| 108 | Self-Evolving Agents | ~1.600 | ⏳ |
| **Total v2.0** | | **~9.500 LOC** | |
