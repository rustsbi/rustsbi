OUTPUT_ARCH(riscv)
ENTRY(_start)

SECTIONS {
    . = 0x80200000;
    istart = .;

    .head.text : ALIGN(8) {
        KEEP(*(.head.text))
    }

    .text : ALIGN(8) {
        *(.text.entry)
        *(.text .text.*)
    }

    .rodata : ALIGN(8) {
        srodata = .;
        *(.rodata .rodata.*)
        *(.srodata .srodata.*)
        . = ALIGN(8);
        erodata = .;
    }

    .data : ALIGN(8) {
        sdata = .;
        *(.data .data.*)
        *(.sdata .sdata.*)
        . = ALIGN(8);
        edata = .;
    }
    sidata = LOADADDR(.data);

    .bss (NOLOAD) : ALIGN(8) {
        *(.bss.uninit)
        sbss = .;
        *(.bss .bss.*)
        *(.sbss .sbss.*)
        ebss = .;
    }
    iend = .;

    /DISCARD/ : {
        *(.eh_frame)
    }
}
