//! Workspace manager — per-display workspaces estilo COSMIC.
//! Cada workspace tem suas proprias janelas + layout.

use alloc::vec;
use alloc::vec::Vec;
use alloc::string::String;
use alloc::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct Workspace {
    pub id: u32,
    pub name: String,
    pub window_ids: Vec<u32>,
    pub active: bool,
    pub layout: LayoutMode,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LayoutMode {
    Floating,
    Tiled,
    Grid,
    Maximized,
}

pub struct WorkspaceManager {
    pub workspaces: Vec<Workspace>,
    pub current: usize,
    pub next_id: u32,
}

impl WorkspaceManager {
    pub fn new() -> Self {
        WorkspaceManager {
            workspaces: vec![
                Workspace { id: 1, name: String::from("main"), window_ids: Vec::new(), active: true, layout: LayoutMode::Floating },
                Workspace { id: 2, name: String::from("dev"), window_ids: Vec::new(), active: false, layout: LayoutMode::Tiled },
                Workspace { id: 3, name: String::from("chat"), window_ids: Vec::new(), active: false, layout: LayoutMode::Floating },
            ],
            current: 0,
            next_id: 4,
        }
    }

    pub fn switch_to(&mut self, idx: usize) {
        if idx < self.workspaces.len() {
            self.workspaces[self.current].active = false;
            self.current = idx;
            self.workspaces[idx].active = true;
        }
    }

    pub fn add_window(&mut self, ws_idx: usize, win_id: u32) {
        if ws_idx < self.workspaces.len() {
            self.workspaces[ws_idx].window_ids.push(win_id);
        }
    }

    pub fn remove_window(&mut self, win_id: u32) {
        for ws in &mut self.workspaces {
            ws.window_ids.retain(|&w| w != win_id);
        }
    }

    pub fn set_layout(&mut self, idx: usize, layout: LayoutMode) {
        if idx < self.workspaces.len() {
            self.workspaces[idx].layout = layout;
        }
    }

    pub fn current_workspace(&self) -> &Workspace { &self.workspaces[self.current] }
    pub fn current_workspace_mut(&mut self) -> &mut Workspace { &mut self.workspaces[self.current] }
}

pub static WORKSPACE_MANAGER: spin::Mutex<Option<WorkspaceManager>> = spin::Mutex::new(None);
