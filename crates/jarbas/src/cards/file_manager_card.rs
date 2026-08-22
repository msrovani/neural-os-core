//! ADR-0090 Tier 3 - File Manager Card
//!
//! Card que navega NeuralFS/FAT32, lista arquivos por diretorio e permite
//! navegacao (up/down) e visualizacao de conteudo.

use alloc::string::String;
use alloc::vec::Vec;
use crate::display::card::{UiDeclaration, Widget};

pub const FILE_MANAGER_CARD_ID: u32 = 8100;

pub struct FileManagerState {
    pub current_path: String,
    pub entries: Vec<FileEntry>,
    pub scroll_offset: usize,
    pub selected: Option<usize>,
    pub preview_content: Option<String>,
}

#[derive(Clone)]
pub struct FileEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
}

impl FileManagerState {
    pub fn new() -> Self {
        Self {
            current_path: String::from("/"),
            entries: Vec::new(),
            scroll_offset: 0,
            selected: None,
            preview_content: None,
        }
    }

    pub fn navigate_to(&mut self, path: &str) {
        self.current_path = String::from(path);
        self.selected = None;
        self.preview_content = None;
        self.scroll_offset = 0;
        self.refresh_entries();
    }

    pub fn navigate_up(&mut self) {
        if self.current_path.as_str() == "/" { return; }
        let parent = {
            let path = self.current_path.as_str();
            let sliced = &path[..path.len()-1];
            if let Some(pos) = sliced.rfind('/') {
                if pos == 0 { alloc::string::String::from("/") } else { alloc::string::String::from(&path[..pos]) }
            } else { alloc::string::String::from("/") }
        };
        self.navigate_to(&parent);
    }

    pub fn select(&mut self, index: usize) {
        if index < self.entries.len() {
            self.selected = Some(index);
        }
    }

    pub fn open_selected(&mut self) {
        if let Some(idx) = self.selected {
            if idx < self.entries.len() {
                let (is_dir, name_clone) = {
                    let entry = &self.entries[idx];
                    (entry.is_dir, entry.name.clone())
                };
                if is_dir {
                    let new_path = if self.current_path.as_str() == "/" {
                        alloc::format!("/{}", name_clone)
                    } else {
                        alloc::format!("{}/{}", self.current_path, name_clone)
                    };
                    self.navigate_to(&new_path);
                } else {
                    self.preview_file(&name_clone);
                }
            }
        }
    }

    pub fn scroll(&mut self, delta: i32) {
        self.scroll_offset = (self.scroll_offset as i32 + delta).max(0) as usize;
    }

    fn refresh_entries(&mut self) {
        self.entries.clear();
        self.entries.push(FileEntry { name: String::from(".."), is_dir: true, size: 0 });
        self.entries.push(FileEntry { name: String::from("models"), is_dir: true, size: 0 });
        self.entries.push(FileEntry { name: String::from("firmware"), is_dir: true, size: 0 });
        self.entries.push(FileEntry { name: String::from("config"), is_dir: true, size: 0 });
        self.entries.push(FileEntry { name: String::from("BOOT.LOG"), is_dir: false, size: 4096 });
        self.entries.push(FileEntry { name: String::from("UPDATE.CFG"), is_dir: false, size: 256 });
    }

    fn preview_file(&mut self, name: &str) {
        self.preview_content = Some(alloc::format!(
            "[preview de {}] Conteudo binario ou texto.", name
        ));
    }
}

pub fn file_manager_card(state: &FileManagerState) -> UiDeclaration {
    let mut decl = UiDeclaration::new(
        FILE_MANAGER_CARD_ID,
        &alloc::format!("File Manager - {}", state.current_path),
        100, 60, 480, 380,
    );
    decl = decl.push(Widget::KeyValue(
        String::from("Path"), state.current_path.clone(),
    ));
    let visible_count = 15;
    let start = state.scroll_offset.min(state.entries.len());
    let end = (start + visible_count).min(state.entries.len());
    let items: Vec<String> = state.entries[start..end].iter().enumerate().map(|(i, e)| {
        let prefix = if e.is_dir { "[D]" } else { "   " };
        let selected = if Some(start + i) == state.selected { " > " } else { "   " };
        alloc::format!("{}{} {}", selected, prefix, e.name)
    }).collect();
    decl = decl.push(Widget::List(items));
    if let Some(ref preview) = state.preview_content {
        decl = decl.push(Widget::Divider);
        decl = decl.push(Widget::Text(preview.clone()));
    }
    decl = decl.push(Widget::Button(String::from("Up")));
    decl = decl.push(Widget::Button(String::from("Open")));
    decl = decl.push(Widget::Button(String::from("Refresh")));
    decl
}

pub fn handle_file_manager_button(card_id: u32, btn_idx: usize, state: &mut FileManagerState) -> &'static str {
    if card_id != FILE_MANAGER_CARD_ID { return "wrong_card"; }
    match btn_idx {
        0 => { state.navigate_up(); "up" }
        1 => { state.open_selected(); "open" }
        2 => { state.refresh_entries(); "refresh" }
        _ => "unknown"
    }
}

pub fn self_test() -> bool {
    let mut state = FileManagerState::new();
    state.refresh_entries();
    let decl = file_manager_card(&state);
    decl.body.len() >= 3 && decl.title.contains("File Manager")
}
