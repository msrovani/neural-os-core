use alloc::boxed::Box;
use alloc::vec::Vec;
use alloc::collections::BTreeMap;
use crate::pipeline::{Pipeline, Stage, Provider};

pub struct DagScheduler {
    pub deps: BTreeMap<&'static str, Vec<&'static str>>,
    pub ordered: Vec<&'static str>,
}

impl DagScheduler {
    pub fn new() -> Self { DagScheduler { deps: BTreeMap::new(), ordered: Vec::new() } }

    pub fn add(&mut self, name: &'static str, depends_on: &[&'static str]) {
        self.deps.insert(name, depends_on.to_vec());
    }

    pub fn resolve(&mut self) -> bool {
        let mut visited: BTreeMap<&'static str, bool> = BTreeMap::new();
        let mut result = Vec::new();
        for name in self.deps.keys().copied() {
            if !self.visit(name, &mut visited, &mut result) { return false; }
        }
        self.ordered = result;
        true
    }

    fn visit(&self, name: &'static str, visited: &mut BTreeMap<&'static str, bool>, result: &mut Vec<&'static str>) -> bool {
        match visited.get(name) {
            Some(&true) => return true,
            Some(&false) => return false,
            None => {}
        }
        visited.insert(name, false);
        if let Some(deps) = self.deps.get(name) {
            for &d in deps { if !self.visit(d, visited, result) { return false; } }
        }
        visited.insert(name, true);
        result.push(name);
        true
    }

    pub fn run(&self, registry: &mut BTreeMap<&'static str, Box<dyn FnMut() -> bool>>) -> bool {
        for name in &self.ordered {
            if let Some(f) = registry.get_mut(name) {
                if !f() { return false; }
            }
        }
        true
    }
}
