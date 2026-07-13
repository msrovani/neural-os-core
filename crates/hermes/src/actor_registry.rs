//! #209 Actor Registry — subagentes com permission model.
//! Spawn, terminate, task state machine, open_work tracking.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;
use k_nano::kjson;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ActorState { Idle, Running, Blocked, Done, Crashed }
impl ActorState {
    pub fn name(&self) -> &'static str {
        match self { ActorState::Idle => "idle", ActorState::Running => "running",
                     ActorState::Blocked => "blocked", ActorState::Done => "done",
                     ActorState::Crashed => "crashed" }
    }
    pub fn is_alive(&self) -> bool { matches!(self, ActorState::Idle | ActorState::Running | ActorState::Blocked) }
}

#[derive(Clone)]
pub struct ActorEntry {
    pub id: u64,
    pub name: String,
    pub state: ActorState,
    pub parent: Option<u64>,
    pub skills: Vec<String>,
    pub ticks_used: u64,
    pub spawned_at: u64,
}

pub struct ActorRegistry {
    actors: BTreeMap<u64, ActorEntry>,
    next_id: u64,
}

impl ActorRegistry {
    pub fn new() -> Self { ActorRegistry { actors: BTreeMap::new(), next_id: 1 } }

    pub fn spawn(&mut self, name: &str, parent: Option<u64>, skills: Vec<String>) -> u64 {
        let id = self.next_id; self.next_id += 1;
        let tick = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
        self.actors.insert(id, ActorEntry {
            id, name: String::from(name), state: ActorState::Idle,
            parent, skills, ticks_used: 0, spawned_at: tick,
        });
        kjson!("ACTOR", name, "spawn", "id", id);
        id
    }

    pub fn transition(&mut self, id: u64, state: ActorState) -> bool {
        if let Some(actor) = self.actors.get_mut(&id) {
            let old = actor.state;
            if old == ActorState::Done || old == ActorState::Crashed { return false; }
            let tick = crate::interrupts::TIMER_TICKS.load(core::sync::atomic::Ordering::Relaxed) as u64;
            actor.ticks_used += tick;
            actor.state = state;
            kjson!("ACTOR", &actor.name, "trans", "id", id, "from", format_args!("\"{:?}\"", old), "to", format_args!("\"{:?}\"", state));
            true
        } else { false }
    }

    pub fn terminate(&mut self, id: u64) -> bool {
        self.transition(id, ActorState::Done)
    }

    pub fn can_message(&self, from: u64, to: u64) -> bool {
        let a = self.actors.get(&from);
        let b = self.actors.get(&to);
        match (a, b) {
            (Some(a), Some(b)) => a.state.is_alive() && b.state.is_alive()
                && (a.parent == Some(to) || b.parent == Some(from) || a.parent == b.parent),
            _ => false,
        }
    }

    pub fn get(&self, id: u64) -> Option<&ActorEntry> { self.actors.get(&id) }
    pub fn get_mut(&mut self, id: u64) -> Option<&mut ActorEntry> { self.actors.get_mut(&id) }

    pub fn by_name(&self, name: &str) -> Vec<&ActorEntry> {
        self.actors.values().filter(|a| a.name == name).collect()
    }

    pub fn alive(&self) -> Vec<&ActorEntry> {
        self.actors.values().filter(|a| a.state.is_alive()).collect()
    }

    pub fn status(&self) -> String {
        let alive = self.alive().len();
        alloc::format!("[ACTOR] {} alive / {} total", alive, self.actors.len())
    }
}
