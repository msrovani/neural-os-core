//! Agent Hooks — Pre/Post tick hooks (IDEA A-015).
//! HookRegistry com slots fixos de function pointers.
//! Hooks retornam Allow/Block/Modify.

use alloc::vec::Vec;

/// Resultado de um hook.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookResult {
    Allow,
    Block,
    Modify,
}

/// Tipo de hook.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HookType {
    PreTick,
    PostTick,
    OnCrash,
    OnSpawn,
}

/// Um hook registrado.
pub struct Hook {
    pub hook_type: HookType,
    pub name: &'static str,
    // Function pointer para callback
    pub callback: fn(agent_name: &str, tick: u64) -> HookResult,
}

/// Registry de hooks.
pub struct HookRegistry {
    hooks: Vec<Hook>,
}

impl HookRegistry {
    pub fn new() -> Self {
        Self { hooks: Vec::new() }
    }

    pub fn register(&mut self, hook: Hook) {
        self.hooks.push(hook);
    }

    pub fn run(&self, hook_type: HookType, agent_name: &str, tick: u64) -> Vec<(&str, HookResult)> {
        let mut results = Vec::new();
        for hook in &self.hooks {
            if hook.hook_type == hook_type {
                let result = (hook.callback)(agent_name, tick);
                results.push((hook.name, result));
                if result == HookResult::Block {
                    break; // Block interrompe a cadeia
                }
            }
        }
        results
    }
}
