//! Window system — unified window content (legacy AppId, Card, Tiled, Floating).

use alloc::vec::Vec;
use alloc::string::String;
use alloc::boxed::Box;
use super::tiling::{TilingNode, WindowId, Rect};
use super::card::UiDeclaration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AppId {
    HermesChat,
    Settings,
    Power,
    Ide,
    WasmSkill(usize),
    Camera,
    AudioViz,
    None,
}

#[derive(Debug, Clone)]
pub enum WindowContent {
    App(AppId),                    // Legacy AppWindow
    Card(UiDeclaration),           // CardWindow
    Tiled(Box<TilingNode>),        // Janela no tiling
    Floating(Box<FloatingWindow>), // Janela floating
}

#[derive(Debug, Clone)]
pub struct FloatingWindow {
    pub window_id: WindowId,
    pub rect: Rect,
    pub content: WindowContent,
    pub decorated: bool,
}

impl FloatingWindow {
    pub fn new(window_id: WindowId, rect: Rect, content: WindowContent) -> Self {
        Self { window_id, rect, content, decorated: true }
    }

    pub fn hit_test(&self, x: i32, y: i32) -> HitArea {
        const H: i32 = 10;
        const TITLE_H: i32 = 28;

        let left = x - self.rect.x < H;
        let right = self.rect.x + self.rect.width as i32 - x < H;
        let top = y - self.rect.y < H + TITLE_H;
        let bottom = self.rect.y + self.rect.height as i32 - y < H;

        match (left, right, top, bottom) {
            (true, _, true, _) => HitArea::ResizeTopLeft,
            (_, true, true, _) => HitArea::ResizeTopRight,
            (true, _, _, true) => HitArea::ResizeBottomLeft,
            (_, true, _, true) => HitArea::ResizeBottomRight,
            (true, _, _, _) => HitArea::ResizeLeft,
            (_, true, _, _) => HitArea::ResizeRight,
            (_, _, true, _) => HitArea::ResizeTop,
            (_, _, _, true) => HitArea::ResizeBottom,
            _ if y - self.rect.y < TITLE_H => HitArea::TitleBar,
            _ => HitArea::Client,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitArea {
    TitleBar,
    Client,
    CloseButton,
    MaximizeButton,
    MinimizeButton,
    Body,
    ResizeTopLeft,
    ResizeTopRight,
    ResizeBottomLeft,
    ResizeBottomRight,
    ResizeLeft,
    ResizeRight,
    ResizeTop,
    ResizeBottom,
}

#[derive(Debug, Clone)]
pub struct Window {
    pub id: WindowId,
    pub app_id: Option<AppId>,
    pub content: WindowContent,
    pub rect: Rect,
    pub workspace: usize,
    pub focused: bool,
    pub decorated: bool,
    pub floating: bool,
    pub minimized: bool,
    pub maximized: bool,
    pub title: String,
    pub visible: bool,
    pub data: String,
    pub z: super::compositor::Layer,
}

impl Window {
    pub fn new(id: WindowId, app_id: Option<AppId>, content: WindowContent, title: &str, floating: bool) -> Self {
        Self {
            id,
            app_id,
            content,
            rect: Rect { x: 0, y: 0, width: 100, height: 100 },
            workspace: 0,
            focused: true,
            decorated: true,
            floating,
            minimized: false,
            maximized: false,
            title: String::from(title),
            visible: false,
            data: String::new(),
            z: super::compositor::Layer::AppWindows,
        }
    }

    pub fn app_id(&self) -> AppId {
        self.app_id.unwrap_or(AppId::None)
    }
}