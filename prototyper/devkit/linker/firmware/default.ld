OUTPUT_ARCH(riscv)
ENTRY(_start)

SECTIONS {
    . = 0x80000000;

    . = ALIGN(0x1000);
    sbi_start = .;

    .text : ALIGN(0x1000) {
        *(.text.entry)
        *(.text .text.*)
    }

    . = ALIGN(0x1000);
    sbi_rodata_start = .;

    .rodata : ALIGN(0x1000) {
        *(.rodata .rodata.*)
        *(.srodata .srodata.*)
        . = ALIGN(0x1000);
    }

    .dynsym : ALIGN(8) {
        *(.dynsym)
    }

    .rela.dyn : ALIGN(8) {
        __rel_dyn_start = .;
        *(.rela*)
        __rel_dyn_end = .;
    }

    . = ALIGN(0x1000);
    sbi_rodata_end = .;

    .data : ALIGN(0x1000) {
        sbi_data_start = .;
        *(.data .data.*)
        *(.sdata .sdata.*)
        . = ALIGN(0x1000);
        sbi_data_end = .;
    }
    sidata = LOADADDR(.data);

    .bss (NOLOAD) : ALIGN(0x1000) {
        *(.bss.stack)
        . = ALIGN(0x1000);
        sbi_heap_start = .;
        *(.bss.heap)
        sbi_heap_end = .;
        . = ALIGN(0x1000);
        sbi_bss_start = .;
        *(.bss .bss.*)
        *(.sbss .sbss.*)
        sbi_bss_end = .;
    }

    /DISCARD/ : {
        *(.eh_frame)
    }

    . = ALIGN(0x1000);

    .text : ALIGN(0x1000) {
        *(.fdt)
    }
    . = ALIGN(0x1000);
    sbi_end = .;

    .handoff (NOLOAD) : ALIGN(0x1000) {
        sbi_handoff_start = .;
        KEEP(*(.handoff))
        . = ALIGN(0x1000);
        sbi_handoff_end = .;
    }
    ASSERT(sbi_handoff_end <= 0x80200000, "firmware handoff storage overlaps payload")

    .text 0x80200000 : ALIGN(0x1000) {
        *(.payload)
    }
}
