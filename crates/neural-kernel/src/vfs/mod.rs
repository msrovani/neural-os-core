//! VFS — Virtual Filesystem Layer.
//! Mount table, path resolution, inode management.
//! Cada montagem é um Agent com read/write skills.
//! Inspirado em: Redox Scheme, Plan 9, Linux VFS.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

pub mod path;

use spin::Mutex;

pub static VFS: Mutex<Option<VfsRegistry>> = Mutex::new(None);

/// Init standard mount points (called at boot)
pub fn init_standard_mounts() {
    let mut vfs = VFS.lock();
    if let Some(ref mut v) = *vfs {
        v.mount("/mnt/ram", "ramfs");
        v.mount("/mnt/hdd", "ata");
        v.mount("/mnt/sdhc", "usbmsc");
        v.mount("/chat", "hermes");
        v.mount("/dev", "devfs");
        v.mount("/proc", "procfs");
        v.mount("/system", "sysfs");
        v.mount("/inference", "inference");
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FileMode {
    File,
    Directory,
    Symlink,
    AgentMount,  // mount point delegado a um agente
    Virtual,     // gerado sob demanda (proc, sys)
}

#[derive(Debug, Clone)]
pub struct VfsNode {
    pub name: String,
    pub mode: FileMode,
    pub inode: u64,
    pub size: u64,
    pub children: BTreeMap<String, VfsNode>,
    pub agent: Option<&'static str>,
}

impl VfsNode {
    pub fn new_dir(name: &str) -> Self {
        VfsNode {
            name: String::from(name),
            mode: FileMode::Directory,
            inode: 0,
            size: 0,
            children: BTreeMap::new(),
            agent: None,
        }
    }

    pub fn new_file(name: &str, size: u64) -> Self {
        VfsNode {
            name: String::from(name),
            mode: FileMode::File,
            inode: 0,
            size,
            children: BTreeMap::new(),
            agent: None,
        }
    }

    pub fn new_mount(name: &str, agent: &'static str) -> Self {
        VfsNode {
            name: String::from(name),
            mode: FileMode::AgentMount,
            inode: 0,
            size: 0,
            children: BTreeMap::new(),
            agent: Some(agent),
        }
    }
}

#[derive(Debug, Clone)]
pub struct VfsMount {
    pub mount_point: &'static str,  // "/chat/", "/dev/", "/proc/"
    pub agent_name: &'static str,   // "hermes", "devfs", "procfs"
    pub flags: u8,
}

impl VfsMount {
    pub const fn new(mount_point: &'static str, agent: &'static str) -> Self {
        VfsMount { mount_point, agent_name: agent, flags: 0 }
    }
}

#[derive(Debug)]
pub struct VfsRegistry {
    mounts: Vec<VfsMount>,
    root: VfsNode,
    next_inode: u64,
}

impl VfsRegistry {
    pub fn new() -> Self {
        VfsRegistry {
            mounts: Vec::new(),
            root: VfsNode::new_dir("/"),
            next_inode: 1,
        }
    }

    pub fn mount(&mut self, mount_point: &'static str, agent: &'static str) {
        // Remove trailing slash for matching
        let mp = mount_point.trim_end_matches('/');
        self.mounts.push(VfsMount::new(mp, agent));
        // Sort by longest prefix first (most specific match wins)
        self.mounts.sort_by(|a, b| {
            b.mount_point.len().cmp(&a.mount_point.len())
        });

        // Add mount node to tree
        let parts = self.split_path(mp);
        Self::do_insert(&mut self.root, &parts, FileMode::AgentMount, agent);
    }

    fn split_path(&self, path: &str) -> Vec<String> {
        path.split('/')
            .filter(|s| !s.is_empty())
            .map(|s| String::from(s))
            .collect()
    }

    fn do_insert(parent: &mut VfsNode, parts: &[String], mode: FileMode, agent: &'static str) {
        if parts.is_empty() { return; }
        let name = &parts[0];

        if !parent.children.contains_key(name) {
            let node = if parts.len() == 1 {
                match mode {
                    FileMode::AgentMount => VfsNode::new_mount(name, agent),
                    FileMode::Directory => VfsNode::new_dir(name),
                    FileMode::File => VfsNode::new_file(name, 0),
                    _ => VfsNode::new_dir(name),
                }
            } else {
                VfsNode::new_dir(name)
            };
            parent.children.insert(name.clone(), node);
        }

        if parts.len() > 1 {
            if let Some(child) = parent.children.get_mut(name) {
                Self::do_insert(child, &parts[1..], mode, agent);
            }
        }
    }

    /// Resolve path → (mount_point, relative_path, agent_name)
    /// "/chat/send" → ("/chat", "send", "hermes")
    pub fn resolve<'a>(&'a self, path: &'a str) -> (&'a VfsMount, &'a str, Option<&'a str>) {
        let path = path.trim_end_matches('/');
        for m in &self.mounts {
            if path == m.mount_point {
                return (m, "", Some(m.agent_name));
            }
            if path.starts_with(m.mount_point) {
                let suffix = &path[m.mount_point.len()..];
                let suffix = suffix.trim_start_matches('/');
                return (m, suffix, Some(m.agent_name));
            }
        }
        // Fallback: root mount
        static ROOT_MOUNT: VfsMount = VfsMount::new("/", "root");
        (&ROOT_MOUNT, path, None)
    }

    /// Lookup node in VFS tree
    pub fn lookup(&self, path: &str) -> Option<&VfsNode> {
        let parts = self.split_path(path);
        let mut current = &self.root;
        for part in &parts {
            match current.children.get(part) {
                Some(child) => current = child,
                None => return None,
            }
        }
        Some(current)
    }

    /// List children of a path
    pub fn list_dir(&self, path: &str) -> Vec<String> {
        let parts = self.split_path(path);
        let mut current = &self.root;
        for part in &parts {
            match current.children.get(part) {
                Some(child) => current = child,
                None => return Vec::new(),
            }
        }
        current.children.keys().cloned().collect()
    }

    pub fn allocate_inode(&mut self) -> u64 {
        let ino = self.next_inode;
        self.next_inode += 1;
        ino
    }

    pub fn mount_table(&self) -> &[VfsMount] {
        &self.mounts
    }

    pub fn fmt_tree(&self) -> String {
        fn fmt_node(buf: &mut String, node: &VfsNode, depth: usize) {
            let indent = "  ".repeat(depth);
            let suffix = match node.mode {
                FileMode::Directory => "/",
                FileMode::AgentMount => " @",
                _ => "",
            };
            buf.push_str(&alloc::format!("{}{}{}\n", indent, node.name, suffix));
            for child in node.children.values() {
                fmt_node(buf, child, depth + 1);
            }
        }
        let mut buf = String::from("/\n");
        for child in self.root.children.values() {
            fmt_node(&mut buf, child, 1);
        }
        buf
    }
}
