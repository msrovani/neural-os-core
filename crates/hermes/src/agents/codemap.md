# crates/hermes/src/agents/

## Responsibility

Native agent structs implementing `agent_core::Agent` (manifest + tick). The
big file is `agents.rs` (sibling of this dir, not a submodule file): it hosts
HermesAgent (chat dispatch, skill execution, LLM handoff), NetAgent,
CortexAgent, InputAgent, ConsoleAgent, boot-phase agents (Platform/Memory/
NetDriver/UsbDriver/BootSelfHeal/BootTrust/HwDetect), SpecialistAgent +
HwSpecialistAgent (The Agency), AutoLearnAgent, SleepCycleAgent,
FsBridgeAgent, GpuDriverAgent, plus `register_agency_agents`/`register_hw_agents`.

## Key symbols

`agents.rs`: `HermesAgent` (`Agent::tick` → chat/route/LLM pipeline), `NetAgent`,
`MonitorAgent`, `HwBridgeAgent`, `report_unmatched_intent`.
`mouse_agent.rs`: `MouseAgent` (IRQ12 → MOUSE_MOVED/MOUSE_CLICK events).
`log_analyst_agent.rs`: `LogAnalystAgent` (Cortex-mining of `/logs/`).

## Integration

Agents are registered into `agent_core::AgentRegistry` at boot (Phase 6
AgentFleet) and ticked by the kernel scheduler; they communicate exclusively
via the EventBus globals re-exported by `crate::globals` (`EVENT_BUS`,
`SKILL_REGISTRY`, `PENDING_SKILL`, `APPROVAL_GATE`).
