# crates/k_nano/src/hardware/ — CPU Topology & Capability Probe

**Responsibility**: CPU microarchitecture and topology detection — Intel Xeon/Epyc
family identification, cache/CCX topology, and a consolidated `HardwareReport`
(`hardware::probe::probe()`).

**Key symbols**: `probe::{probe, HardwareReport}`, `topology::{...cache/core topology}`,
`xeon::{...Xeon family detect}`, `epyc::{...Epyc family detect}`.

**Integration**: feeds `platform_probe::HardwareInfo` (boot-time ISA/hypervisor gate,
ADR-0055) and `cpufreq`/`core_pinning` decisions; pure detection — no hardware state
beyond what ACPI/CPUID provides.
