//! VirtIO-GPU — STUB. Não implementado no k_nano (fora de escopo desta tarefa;
//! reimplementação completa do driver VirtIO-GPU é sprint separada).
//! Sempre retorna `false` (driver indisponível) para permitir fallback seguro
//! ao caller (ex: framebuffer legado/VGA), sem alocar MMIO nem tocar hardware.

pub unsafe fn init_driver_virtio_gpu() -> bool {
    false
}
