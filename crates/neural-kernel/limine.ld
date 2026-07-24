/* ADR-0065 — linker higher-half Limine (alinhado ClaudioOS / limine-rust-template). */
OUTPUT_FORMAT(elf64-x86-64)
ENTRY(_start)

PHDRS
{
    limine_requests PT_LOAD FLAGS(4); /* R-- */
    text PT_LOAD FLAGS(5);            /* R-X */
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

    /DISCARD/ : {
        *(.eh_frame*)
        *(.note .note.*)
        *(.interp)
    }
}
