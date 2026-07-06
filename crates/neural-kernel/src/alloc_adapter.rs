//! Alloc Adapter — ponte para buddy-slab-allocator (Sprint 86, #355).
//! Quando ativado (feature "buddy-slab"), substitui o slab allocator custom
//! e o backend do VRAM buddy allocator pelo crate maduro do ArceOS.
//!
//! Referência: https://github.com/arceos-hypervisor/buddy-slab-allocator
//! Uso: adicionar ao Cargo.toml:
//!   buddy-slab-allocator = { version = "0.4", optional = true }
//!   [features]
//!   buddy-slab = ["buddy-slab-allocator"]
//!
//! Pipeline de migração:
//! 1. Adicionar dependência + feature
//! 2. Criar GlobalAllocator wrapper sobre buddy-slab-allocator
//! 3. Substituir GLOBAL_ALLOCATOR e slab::SlabAllocator
//! 4. Adaptar gpu/vram.rs para usar BuddyAllocator com gerenciamento de RAM física
//! 5. Remover slab.rs e código legado

#![allow(dead_code)]

use crate::serial_println;

/// Status da integração buddy-slab
pub fn status() -> &'static str {
    #[cfg(feature = "buddy-slab")]
    { "buddy-slab: ATIVO (via crate buddy-slab-allocator 0.4)" }
    #[cfg(not(feature = "buddy-slab"))]
    { "buddy-slab: DISPONIVEL (adicione feature 'buddy-slab' ao Cargo.toml)" }
}

/// Inicializa o alocador (usando buddy-slab-allocator se compilado, ou o allocator atual)
pub fn init_global() {
    serial_println!("[ALLOC] {} — usando allocator legado como fallback", status());
    #[cfg(feature = "buddy-slab")]
    {
        // TODO: integrar buddy-slab-allocator::GlobalAllocator
        // let mut alloc = buddy_slab_allocator::GlobalAllocator::<4096>::new();
        // unsafe { alloc.init(heap_region); }
        serial_println!("[ALLOC] buddy-slab-allocator disponivel mas aguardando init");
    }
}
