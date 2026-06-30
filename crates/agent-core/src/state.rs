//! Type-state Agent lifecycle — Theseus-inspired.
//! Estados: Boot → Running → Faulted → Boot (reset)
//! Transições inválidas são erro de compilação.

pub struct AgentBoot;
pub struct AgentRunning;
pub struct AgentFaulted;
pub struct AgentDone;

pub trait IntoAgent<State> {
    fn name(&self) -> &str;
}

pub struct TypedAgent<State> {
    pub name: &'static str,
    pub id: u64,
    _state: core::marker::PhantomData<State>,
}

impl TypedAgent<AgentBoot> {
    pub fn new(name: &'static str, id: u64) -> Self {
        TypedAgent { name, id, _state: core::marker::PhantomData }
    }

    pub fn activate(self) -> TypedAgent<AgentRunning> {
        TypedAgent { name: self.name, id: self.id, _state: core::marker::PhantomData }
    }
}

impl TypedAgent<AgentRunning> {
    pub fn complete(self) -> TypedAgent<AgentDone> {
        TypedAgent { name: self.name, id: self.id, _state: core::marker::PhantomData }
    }

    pub fn crash(self) -> TypedAgent<AgentFaulted> {
        TypedAgent { name: self.name, id: self.id, _state: core::marker::PhantomData }
    }
}

impl TypedAgent<AgentFaulted> {
    pub fn reset(self) -> TypedAgent<AgentBoot> {
        TypedAgent { name: self.name, id: self.id, _state: core::marker::PhantomData }
    }
}
