# crates/neural-kernel/src/agents/

Native agent implementations that sit beside the big `agents.rs` (which itself defines
the core fleet: Monitor, HwBridge, Net, Cortex, Hermes, Platform, NetDriver, UsbDriver,
GpuDriver, BootTrust/SelfHeal, HwDetect, AutoLearn, SleepCycle, SelfEvolve, FsBridge,
DiagnosticSkill, plus `init_platform_sync()` and `register_agency_agents()`).

## Key symbols

- `mouse_agent.rs` — `MouseAgent`: polls `interrupts::LAST_MOUSE_PACKET` (IRQ12), publishes
  `TOPIC_MOUSE_MOVED/CLICK/DRAG/SCROLL`.
- `sysinfo_agent.rs` — `SysInfoAgent`: lock-free CPU/RAM/agent/uptime snapshot → Jarbas
  debug card (ID 9001) refreshed every ~50 ticks.
- `log_analyst_agent.rs` — `LogAnalystAgent` (`PollEvery(500)`): reads `/logs/` and runs
  Cortex LLM pattern/anomaly analysis, publishes findings on the EventBus.

## Integration

Registered into the `agent_core::AgentRegistry` in `kernel_boot()` (before Hermes so
continuous poll order favors them); consume/produce EventBus topics; render via
`jarbas_crate::display::card`/`compositor`.
