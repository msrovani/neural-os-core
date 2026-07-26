//! Workspaces — virtual desktops with tiling + floating windows per workspace.
//! Baseado no COSMIC: WorkspaceSet per-output + Vec<Workspace> + active/previously_active.

use alloc::vec::Vec;
use alloc::string::String;
use super::tiling::{TilingNode, WindowId, Rect, SplitDirection};
pub use super::window::{FloatingWindow, WindowContent};

#[derive(Debug, Clone)]
pub struct Workspace {
    pub id: usize,
    pub tiling_root: Option<TilingNode>,
    pub floating_windows: Vec<FloatingWindow>,
    pub output_id: Option<usize>,
    pub name: Option<String>,
}

impl Workspace {
    pub fn new(id: usize) -> Self {
        Self {
            id,
            tiling_root: None,
            floating_windows: Vec::new(),
            output_id: None,
            name: None,
        }
    }

    pub fn add_window_tiled(&mut self, window_id: WindowId, direction: SplitDirection) {
        match &mut self.tiling_root {
            Some(root) => root.insert_window(window_id, direction),
            None => self.tiling_root = Some(TilingNode::new_window(window_id)),
        }
    }

    pub fn add_window_floating(&mut self, window: FloatingWindow) {
        self.floating_windows.push(window);
    }

    pub fn remove_window(&mut self, window_id: WindowId) -> bool {
        if let Some(root) = &mut self.tiling_root {
            if root.replace_window_with_placeholder(window_id) {
                return true;
            }
        }
        if let Some(idx) = self.floating_windows.iter().position(|w| w.window_id == window_id) {
            self.floating_windows.remove(idx);
            return true;
        }
        false
    }

    pub fn layout_tiled(&self, rect: Rect) -> Vec<(WindowId, Rect)> {
        let mut out = Vec::new();
        if let Some(root) = &self.tiling_root {
            root.layout(rect, &mut out);
        }
        out
    }
}

#[derive(Debug, Clone)]
pub struct Workspaces {
    pub workspaces: Vec<Workspace>,
    pub active: usize,
    pub previously_active: Option<usize>,
    pub max_workspaces: usize,
}

impl Workspaces {
    pub fn new(count: usize) -> Self {
        let mut ws = Vec::with_capacity(count);
        for i in 0..count {
            ws.push(Workspace::new(i));
        }
        Self {
            workspaces: ws,
            active: 0,
            previously_active: None,
            max_workspaces: 9, // Super+1-9
        }
    }

    pub fn active_mut(&mut self) -> &mut Workspace {
        &mut self.workspaces[self.active]
    }

    pub fn active(&self) -> &Workspace {
        &self.workspaces[self.active]
    }

    pub fn switch(&mut self, idx: usize) -> bool {
        if idx < self.workspaces.len() && idx != self.active {
            self.previously_active = Some(self.active);
            self.active = idx;
            true
        } else {
            false
        }
    }

    pub fn switch_previous(&mut self) -> bool {
        if let Some(prev) = self.previously_active {
            self.switch(prev)
        } else {
            false
        }
    }

    pub fn next(&mut self) {
        let next = (self.active + 1) % self.workspaces.len();
        self.switch(next);
    }

    pub fn prev(&mut self) {
        let prev = if self.active == 0 { self.workspaces.len() - 1 } else { self.active - 1 };
        self.switch(prev);
    }
}

impl Default for Workspaces {
    fn default() -> Self {
        Self::new(4) // 4 workspaces default (Super+1-4)
    }
}