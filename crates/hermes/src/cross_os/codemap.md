# crates/hermes/src/cross_os/

## Responsibility

Cross-OS ecosystem layer: runtime discovery and execution of skills across
external ecosystems (package hub, P2P marketplaces, GitHub, crates.io) —
"search at runtime, learn from use, evolve alone".

## Key symbols

`mod.rs` re-exports: `CrossOsAgent` (Continuous agent cycling
ANALYZE→SEARCH→EXECUTE→LEARN→EVOLVE on `USER_INTENT`), `CrossOsDiscoverer`
(with `SkillCandidate`, `SkillSource`, `SkillFormat`, `DiscoverResult`),
`CrossOsIntent` + `IntentCategory` + `IntentResult`. Discovery talks to MCP
servers via `mcp_client` rather than direct coupling (ADR-0076 F3/F6).

## Integration

`CrossOsDiscoverer` queries the local `package_hub` and remote sources through
`mcp_client`/`net_bridge`; found skills are registered into
`k_nano::SKILL_REGISTRY` and executed through the standard skill path.
