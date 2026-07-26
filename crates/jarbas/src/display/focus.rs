//! Focus model — MRU stack per seat, focus policies (follows-mouse / click-to-focus).
//! Baseado no design COSMIC adaptado para bare-metal (single seat).

use alloc::vec::Vec;
use super::tiling::WindowId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusPolicy {
    FollowsMouse,
    ClickToFocus,
}

#[derive(Debug, Clone)]
pub struct FocusStack {
    /// MRU order: [0] = most recently focused
    pub stack: Vec<WindowId>,
    pub policy: FocusPolicy,
}

impl FocusStack {
    pub fn new(policy: FocusPolicy) -> Self {
        Self { stack: Vec::new(), policy }
    }

    pub fn focus(&mut self, window_id: WindowId) {
        self.stack.retain(|&id| id != window_id);
        self.stack.insert(0, window_id);
    }

    pub fn unfocus(&mut self, window_id: WindowId) {
        self.stack.retain(|&id| id != window_id);
    }

    pub fn focused(&self) -> Option<WindowId> {
        self.stack.first().copied()
    }

    pub fn cycle_next(&self) -> Option<WindowId> {
        if self.stack.len() >= 2 {
            Some(self.stack[1])
        } else {
            None
        }
    }

    pub fn cycle_prev(&self) -> Option<WindowId> {
        if self.stack.len() >= 2 {
            Some(self.stack[self.stack.len() - 1])
        } else {
            None
        }
    }

    pub fn on_mouse_enter(&mut self, window_id: WindowId) {
        if self.policy == FocusPolicy::FollowsMouse {
            self.focus(window_id);
        }
    }

    pub fn on_click(&mut self, window_id: WindowId) {
        if self.policy == FocusPolicy::ClickToFocus {
            self.focus(window_id);
        }
    }
}