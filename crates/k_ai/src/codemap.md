# k_ai/src — Overview

Ring 2 (R2) autonomy source: self-healing (checkpoint restore, health-issue pipeline), trust (TrustCache with graduated enforcement), Agency (AgentSpec catalog + embedded SKILL.md seeds), inventory, on-device training (ternary fine-tuning, federated gradients), and the SGDB cognitive path database (HANR/audit/pkg/skills/episodic/RAG). ~62 `.rs` files behind `lib.rs`, `no_std`, depends on k_nano (foundation) and cortex (BitNet/tensors).

See the full crate map: `crates/k_ai/codemap.md`. Submodule maps: [`arch/codemap.md`](arch/codemap.md), [`fs/codemap.md`](fs/codemap.md), [`sgdb/codemap.md`](sgdb/codemap.md), [`vision/codemap.md`](vision/codemap.md).
