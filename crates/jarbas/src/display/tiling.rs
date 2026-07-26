//! Tiling window manager — binary split (bsp) tree with proportional sizes.
//! Baseado no COSMIC: Group/Window/Placeholder + Orientation + sizes proporcionais.

use alloc::vec::Vec;
use alloc::boxed::Box;

/// Identificador único de janela (u64).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct WindowId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Orientation {
    Horizontal,
    Vertical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    Left,
    Right,
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SplitSizes {
    /// Proporção do primeiro filho (0–1000 = 0.000–1.000 fixed point). O segundo recebe 1000 - first.
    pub first: u32,
}

impl Default for SplitSizes {
    fn default() -> Self {
        Self { first: 500 } // 0.5
    }
}

impl SplitSizes {
    pub fn from_f32(f: f32) -> Self {
        Self { first: (f.clamp(0.1, 0.9) * 1000.0) as u32 }
    }

    pub fn to_f32(&self) -> f32 {
        self.first as f32 / 1000.0
    }
}

#[derive(Debug, Clone)]
pub enum TilingNode {
    /// Container com split binário
    Group {
        orientation: Orientation,
        children: [Box<TilingNode>; 2],
        sizes: SplitSizes,
    },
    /// Janela real (leaf)
    Window {
        window_id: WindowId,
    },
    /// Placeholder vazio (para preservar estrutura ao fechar)
    Placeholder,
}

impl TilingNode {
    pub fn new_window(window_id: WindowId) -> Self {
        Self::Window { window_id }
    }

    pub fn new_horizontal(left: TilingNode, right: TilingNode) -> Self {
        Self::Group {
            orientation: Orientation::Horizontal,
            children: [Box::new(left), Box::new(right)],
            sizes: SplitSizes::default(),
        }
    }

    pub fn new_vertical(top: TilingNode, bottom: TilingNode) -> Self {
        Self::Group {
            orientation: Orientation::Vertical,
            children: [Box::new(top), Box::new(bottom)],
            sizes: SplitSizes::default(),
        }
    }

    /// Encontra nó por WindowId (para focus/close)
    pub fn find_window(&self, id: WindowId) -> Option<&TilingNode> {
        match self {
            Self::Window { window_id } if *window_id == id => Some(self),
            Self::Group { children, .. } => {
                children[0].find_window(id).or_else(|| children[1].find_window(id))
            }
            _ => None,
        }
    }

    /// Encontra nó mutável por WindowId
    pub fn find_window_mut(&mut self, id: WindowId) -> Option<&mut TilingNode> {
        match self {
            Self::Window { window_id } if *window_id == id => Some(self),
            Self::Group { children, .. } => {
                // Avoid closure borrow conflict: check then access separately
                if children[0].find_window(id).is_some() {
                    children[0].find_window_mut(id)
                } else {
                    children[1].find_window_mut(id)
                }
            }
            _ => None,
        }
    }

    /// Substitui janela por Placeholder (close)
    pub fn replace_window_with_placeholder(&mut self, id: WindowId) -> bool {
        match self {
            Self::Window { window_id } if *window_id == id => {
                *self = Self::Placeholder;
                true
            }
            Self::Group { children, .. } => {
                children[0].replace_window_with_placeholder(id)
                    || children[1].replace_window_with_placeholder(id)
            }
            _ => false,
        }
    }

    /// Insere nova janela: split do nó focado ou root
    pub fn insert_window(&mut self, new_id: WindowId, direction: SplitDirection) {
        match self {
            Self::Placeholder => *self = Self::new_window(new_id),
            Self::Window { .. } => {
                // Split binário: cria Group com janela existente + nova
                let existing = self.clone();
                match direction {
                    SplitDirection::Left | SplitDirection::Up => {
                        *self = match direction {
                            SplitDirection::Left => Self::new_horizontal(
                                Self::new_window(new_id),
                                existing,
                            ),
                            SplitDirection::Up => Self::new_vertical(
                                Self::new_window(new_id),
                                existing,
                            ),
                            _ => unreachable!(),
                        };
                    }
                    SplitDirection::Right | SplitDirection::Down => {
                        *self = match direction {
                            SplitDirection::Right => Self::new_horizontal(
                                existing,
                                Self::new_window(new_id),
                            ),
                            SplitDirection::Down => Self::new_vertical(
                                existing,
                                Self::new_window(new_id),
                            ),
                            _ => unreachable!(),
                        };
                    }
                }
            }
            Self::Group { children, orientation, .. } => {
                // Recursivo: insere no filho que tem foco (ou primeiro não-placeholder)
                let target = if matches!(direction, SplitDirection::Left | SplitDirection::Up) {
                    &mut children[0]
                } else {
                    &mut children[1]
                };
                target.insert_window(new_id, direction);
            }
        }
    }

    /// Redimensiona split (Super+Shift+Arrow)
    pub fn resize_split(&mut self, id: WindowId, delta: i32) -> bool {
        match self {
            Self::Group { children, sizes, .. } => {
                if children[0].find_window(id).is_some() {
                    sizes.first = ((sizes.first as i32 + delta).clamp(100, 900)) as u32;
                    true
                } else if children[1].find_window(id).is_some() {
                    sizes.first = ((sizes.first as i32 - delta).clamp(100, 900)) as u32;
                    true
                } else {
                    children[0].resize_split(id, delta) || children[1].resize_split(id, delta)
                }
            }
            _ => false,
        }
    }

    /// Layout: calcula rects para cada janela
    pub fn layout(&self, rect: Rect, out: &mut Vec<(WindowId, Rect)>) {
        match self {
            Self::Window { window_id } => out.push((*window_id, rect)),
            Self::Group { orientation, children, sizes } => {
                let ratio = sizes.first as f32 / 1000.0;
                let (r1, r2) = match orientation {
                    Orientation::Horizontal => {
                        let split_x = rect.x + (rect.width as f32 * ratio) as i32;
                        (
                            Rect { x: rect.x, y: rect.y, width: (split_x - rect.x).max(1) as u32, height: rect.height },
                            Rect { x: split_x, y: rect.y, width: (rect.x + rect.width as i32 - split_x).max(1) as u32, height: rect.height },
                        )
                    }
                    Orientation::Vertical => {
                        let split_y = rect.y + (rect.height as f32 * ratio) as i32;
                        (
                            Rect { x: rect.x, y: rect.y, width: rect.width, height: (split_y - rect.y).max(1) as u32 },
                            Rect { x: rect.x, y: split_y, width: rect.width, height: (rect.y + rect.height as i32 - split_y).max(1) as u32 },
                        )
                    }
                };
                children[0].layout(r1, out);
                children[1].layout(r2, out);
            }
            Self::Placeholder => {}
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}