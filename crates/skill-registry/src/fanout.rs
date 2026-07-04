use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

type SubTaskFn = Box<dyn FnOnce() -> Result<Vec<u8>, &'static str> + Send>;

pub struct SubTask {
    pub name: String,
    pub result: Option<Result<Vec<u8>, &'static str>>,
    started: bool,
    work: Option<SubTaskFn>,
}

impl SubTask {
    pub fn new(name: &str, work: SubTaskFn) -> Self {
        SubTask {
            name: String::from(name),
            result: None,
            started: false,
            work: Some(work),
        }
    }

    pub fn poll(&mut self) -> bool {
        if self.result.is_some() {
            return true;
        }
        if !self.started {
            self.started = true;
            if let Some(f) = self.work.take() {
                self.result = Some(f());
            }
        }
        self.result.is_some()
    }
}

pub struct FanOutPool {
    tasks: BTreeMap<u64, SubTask>,
    next_id: u64,
}

impl FanOutPool {
    pub fn new() -> Self {
        FanOutPool {
            tasks: BTreeMap::new(),
            next_id: 0,
        }
    }

    pub fn spawn(&mut self, name: &str, work: SubTaskFn) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.tasks.insert(id, SubTask::new(name, work));
        id
    }

    pub fn poll_all(&mut self) {
        let done: Vec<u64> = self.tasks.iter_mut()
            .filter_map(|(&id, t)| if t.poll() { Some(id) } else { None })
            .collect();
        for id in done {
            self.tasks.remove(&id);
        }
    }

    pub fn is_done(&self, id: u64) -> bool {
        self.tasks.get(&id).map_or(true, |t| t.result.is_some())
    }

    pub fn take_result(&mut self, id: u64) -> Option<Result<Vec<u8>, &'static str>> {
        self.tasks.remove(&id).and_then(|t| t.result)
    }

    pub fn pending_count(&self) -> usize {
        self.tasks.len()
    }

    pub fn all_results(&mut self) -> Vec<(String, Result<Vec<u8>, &'static str>)> {
        self.poll_all();
        let mut results = Vec::new();
        let ids: Vec<u64> = self.tasks.keys().copied().collect();
        for id in ids {
            if let Some(t) = self.tasks.remove(&id) {
                if let Some(r) = t.result {
                    results.push((t.name, r));
                }
            }
        }
        results
    }
}
