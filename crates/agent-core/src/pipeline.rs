use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Clone, Debug)]
pub struct Provider {
    pub name: &'static str,
    pub score: u8,
    pub activate: fn() -> bool,
}

#[derive(Clone, Debug)]
pub struct Stage {
    pub name: &'static str,
    pub providers: Vec<Provider>,
    pub required: bool,
}

impl Stage {
    pub fn run(&self) -> bool {
        let mut best: Option<&Provider> = None;
        for p in &self.providers {
            if best.is_none() || p.score > best.unwrap().score {
                best = Some(p);
            }
        }
        if let Some(p) = best {
            if (p.activate)() { return true; }
        }
        for p in &self.providers {
            if (p.activate)() { return true; }
        }
        !self.required
    }
}

pub struct Pipeline {
    pub stages: Vec<Stage>,
}

impl Pipeline {
    pub const fn new() -> Self { Pipeline { stages: Vec::new() } }

    pub fn add(&mut self, stage: Stage) { self.stages.push(stage); }

    pub fn run(&mut self) -> bool {
        for s in &self.stages {
            if !s.run() { return false; }
        }
        true
    }
}
