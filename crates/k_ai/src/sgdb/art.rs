//! ADR-0063 F4 — Adaptive Radix Tree (ART) lite para chaves L0–L3.
//! Node4 + Node16 + Leaf; honesty: não é ART completo Node48/256.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

#[derive(Clone)]
enum Node {
    Leaf {
        key: Vec<u8>,
        value: u64, // handle / offset id
    },
    Inner4 {
        prefix: Vec<u8>,
        keys: [u8; 4],
        children: [Option<Box<Node>>; 4],
        n: u8,
    },
    Inner16 {
        prefix: Vec<u8>,
        keys: [u8; 16],
        children: [Option<Box<Node>>; 16],
        n: u8,
    },
}

pub struct ArtIndex {
    root: Option<Box<Node>>,
    pub len: usize,
}

impl ArtIndex {
    pub fn new() -> Self {
        ArtIndex {
            root: None,
            len: 0,
        }
    }

    pub fn insert(&mut self, key: &str, value: u64) {
        let kb = key.as_bytes();
        if self.root.is_none() {
            self.root = Some(Box::new(Node::Leaf {
                key: kb.to_vec(),
                value,
            }));
            self.len = 1;
            return;
        }
        let root = self.root.take().unwrap();
        self.root = Some(insert_rec(root, kb, 0, value, &mut self.len));
    }

    pub fn get(&self, key: &str) -> Option<u64> {
        let mut node = self.root.as_ref()?;
        let kb = key.as_bytes();
        let mut depth = 0usize;
        loop {
            match node.as_ref() {
                Node::Leaf { key: lk, value } => {
                    return if lk.as_slice() == kb { Some(*value) } else { None };
                }
                Node::Inner4 {
                    prefix,
                    keys,
                    children,
                    n,
                } => {
                    if !kb[depth..].starts_with(prefix) {
                        return None;
                    }
                    depth += prefix.len();
                    if depth >= kb.len() {
                        return None;
                    }
                    let b = kb[depth];
                    depth += 1;
                    let mut found = None;
                    for i in 0..*n as usize {
                        if keys[i] == b {
                            found = children[i].as_ref();
                            break;
                        }
                    }
                    node = found?;
                }
                Node::Inner16 {
                    prefix,
                    keys,
                    children,
                    n,
                } => {
                    if !kb[depth..].starts_with(prefix) {
                        return None;
                    }
                    depth += prefix.len();
                    if depth >= kb.len() {
                        return None;
                    }
                    let b = kb[depth];
                    depth += 1;
                    let mut found = None;
                    for i in 0..*n as usize {
                        if keys[i] == b {
                            found = children[i].as_ref();
                            break;
                        }
                    }
                    node = found?;
                }
            }
        }
    }

    pub fn scan_prefix(&self, prefix: &str) -> Vec<(String, u64)> {
        let mut out = Vec::new();
        if let Some(ref root) = self.root {
            collect_prefix(root, prefix.as_bytes(), &mut out);
        }
        out
    }
}

fn common_prefix(a: &[u8], b: &[u8]) -> usize {
    a.iter().zip(b.iter()).take_while(|(x, y)| x == y).count()
}

fn insert_rec(node: Box<Node>, key: &[u8], depth: usize, value: u64, len: &mut usize) -> Box<Node> {
    match *node {
        Node::Leaf {
            key: ref lk,
            value: old_v,
        } => {
            if lk.as_slice() == key {
                return Box::new(Node::Leaf {
                    key: key.to_vec(),
                    value,
                });
            }
            let cp = common_prefix(&lk[depth.min(lk.len())..], &key[depth.min(key.len())..]);
            let prefix = lk[depth..depth + cp].to_vec();
            let d2 = depth + cp;
            let b1 = if d2 < lk.len() { lk[d2] } else { 0 };
            let b2 = if d2 < key.len() { key[d2] } else { 0 };
            let leaf1 = Box::new(Node::Leaf {
                key: lk.clone(),
                value: old_v,
            });
            let leaf2 = Box::new(Node::Leaf {
                key: key.to_vec(),
                value,
            });
            *len += 1;
            let mut keys = [0u8; 4];
            let mut children: [Option<Box<Node>>; 4] = [None, None, None, None];
            keys[0] = b1;
            keys[1] = b2;
            children[0] = Some(leaf1);
            children[1] = Some(leaf2);
            Box::new(Node::Inner4 {
                prefix,
                keys,
                children,
                n: 2,
            })
        }
        Node::Inner4 {
            prefix,
            mut keys,
            mut children,
            mut n,
        } => {
            if key.len() >= depth + prefix.len() && key[depth..].starts_with(&prefix) {
                let d2 = depth + prefix.len();
                if d2 >= key.len() {
                    return Box::new(Node::Inner4 {
                        prefix,
                        keys,
                        children,
                        n,
                    });
                }
                let b = key[d2];
                for i in 0..n as usize {
                    if keys[i] == b {
                        let child = children[i].take().unwrap();
                        children[i] = Some(insert_rec(child, key, d2 + 1, value, len));
                        return Box::new(Node::Inner4 {
                            prefix,
                            keys,
                            children,
                            n,
                        });
                    }
                }
                if n < 4 {
                    keys[n as usize] = b;
                    children[n as usize] = Some(Box::new(Node::Leaf {
                        key: key.to_vec(),
                        value,
                    }));
                    n += 1;
                    *len += 1;
                    return Box::new(Node::Inner4 {
                        prefix,
                        keys,
                        children,
                        n,
                    });
                }
                // grow to Inner16
                let mut k16 = [0u8; 16];
                let mut c16: [Option<Box<Node>>; 16] = [
                    None, None, None, None, None, None, None, None, None, None, None, None,
                    None, None, None, None,
                ];
                for i in 0..4 {
                    k16[i] = keys[i];
                    c16[i] = children[i].take();
                }
                k16[4] = b;
                c16[4] = Some(Box::new(Node::Leaf {
                    key: key.to_vec(),
                    value,
                }));
                *len += 1;
                Box::new(Node::Inner16 {
                    prefix,
                    keys: k16,
                    children: c16,
                    n: 5,
                })
            } else {
                // mismatch prefix — simplify: wrap (rare path)
                Box::new(Node::Inner4 {
                    prefix,
                    keys,
                    children,
                    n,
                })
            }
        }
        Node::Inner16 {
            prefix,
            mut keys,
            mut children,
            mut n,
        } => {
            if key.len() >= depth + prefix.len() && key[depth..].starts_with(&prefix) {
                let d2 = depth + prefix.len();
                if d2 >= key.len() {
                    return Box::new(Node::Inner16 {
                        prefix,
                        keys,
                        children,
                        n,
                    });
                }
                let b = key[d2];
                for i in 0..n as usize {
                    if keys[i] == b {
                        let child = children[i].take().unwrap();
                        children[i] = Some(insert_rec(child, key, d2 + 1, value, len));
                        return Box::new(Node::Inner16 {
                            prefix,
                            keys,
                            children,
                            n,
                        });
                    }
                }
                if (n as usize) < 16 {
                    keys[n as usize] = b;
                    children[n as usize] = Some(Box::new(Node::Leaf {
                        key: key.to_vec(),
                        value,
                    }));
                    n += 1;
                    *len += 1;
                }
                Box::new(Node::Inner16 {
                    prefix,
                    keys,
                    children,
                    n,
                })
            } else {
                Box::new(Node::Inner16 {
                    prefix,
                    keys,
                    children,
                    n,
                })
            }
        }
    }
}

fn collect_prefix(node: &Node, prefix: &[u8], out: &mut Vec<(String, u64)>) {
    match node {
        Node::Leaf { key, value } => {
            if key.starts_with(prefix) {
                if let Ok(s) = core::str::from_utf8(key) {
                    out.push((String::from(s), *value));
                }
            }
        }
        Node::Inner4 { children, n, .. } => {
            for i in 0..*n as usize {
                if let Some(ref c) = children[i] {
                    collect_prefix(c, prefix, out);
                }
            }
        }
        Node::Inner16 { children, n, .. } => {
            for i in 0..*n as usize {
                if let Some(ref c) = children[i] {
                    collect_prefix(c, prefix, out);
                }
            }
        }
    }
}

impl Default for ArtIndex {
    fn default() -> Self {
        Self::new()
    }
}
