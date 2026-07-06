//! Alloc Adapter — #355: integração com buddy-alloc (413K downloads).
//! Uso futuro: o crate está disponível, aguardando adaptação da API.
//! Enquanto isso, usamos o allocator custom existente.

#![allow(dead_code)]
use crate::serial_println;

pub fn init(_heap_start: usize, _heap_size: usize) {
    // TODO: migrar para buddy_alloc::BuddyAlloc quando a API for estabilizada
    serial_println!("[ALLOC] usando allocator custom (buddy-alloc disponivel como ref)");
}

pub fn status() -> &'static str { "buddy-alloc: DISPONIVEL (crate 0.6, aguardando integracao)" }
