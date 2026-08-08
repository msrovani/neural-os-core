/* ADR-0065 — linker higher-half Limine (alinhado ClaudioOS / limine-rust-template). */
OUTPUT_FORMAT(elf64-x86-64)
ENTRY(_start)

PHDRS
{
    limine_requests PT_LOAD FLAGS(4); /* R-- */
    text PT_LOAD FLAGS(5);            /* R-X — W^X enforced */
    rodata PT_LOAD FLAGS(4);          /* R-- */
    data PT_LOAD FLAGS(6);            /* RW- */
}

SECTIONS
{
    . = 0xffffffff80000000;

    /* Requests primeiro — Limine acha barato (markers + .requests). */
    .requests : {
        KEEP(*(.requests_start_marker))
        KEEP(*(.requests))
        KEEP(*(.requests_end_marker))
    } :limine_requests

    . = ALIGN(CONSTANT(MAXPAGESIZE));

    .text : {
        *(.text .text.*)
    } :text

    . = ALIGN(CONSTANT(MAXPAGESIZE));

    .rodata : {
        *(.rodata .rodata.*)
    } :rodata

    . = ALIGN(CONSTANT(MAXPAGESIZE));

    .data : {
        *(.data .data.*)
        *(.got .got.*)
    } :data

    .bss : {
        *(.bss .bss.*)
        *(COMMON)
    } :data

    /* SESSION_233: HEAP_BUFFER (512MB) em secao propria no FIM da imagem.
       O bump allocator estende alem dele (resize_bump_heap) mapeando frames
       novos — como nada vem depois no .bss, a extensao NAO corrompe statics
       adjacentes (GLOBAL_ALLOCATOR, PHYS_MEM_OFFSET, TOTAL_RAM_MB). */
    .bss.heap : {
        *(.bss.heap)
    } :data

    /* SESSION_252/ora-1: fim da imagem do kernel — usado pelo frame allocator
       para marcar a RAM do kernel como OCUPADA (nunca entregar a DMA). O
       KernelAddressRequest dá physical_base; KERNEL_END - 0xffffffff80000000
       dá o tamanho da imagem. */
    KERNEL_END = .;

    /DISCARD/ : {
        *(.eh_frame*)
        *(.note .note.*)
        *(.interp)
    }
}
