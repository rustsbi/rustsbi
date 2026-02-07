OUTPUT_ARCH(riscv)

BASE_ADDRESS = 0xffffffc080200000;

ENTRY(_start)
SECTIONS
{
    . = BASE_ADDRESS;
    _skernel = .;

    .text : ALIGN(4K) {
        _stext = .;
        *(.text.boot)
        *(.text .text.*)
        . = ALIGN(4K);
        _etext = .;
    }

    _srodata = .;
    .rodata : ALIGN(4K) {
        *(.rodata .rodata.*)
        *(.srodata .srodata.*)
        *(.sdata2 .sdata2.*)
    }

    .init_array : ALIGN(0x10) {
        __init_array_start = .;
        *(.init_array .init_array.*)
        __init_array_end = .;
    }

    . = ALIGN(4K);
    _erodata = .;

    .data : ALIGN(4K) {
        _sdata = .;
        *(.data.boot_page_table)
        . = ALIGN(4K);
        *(.data .data.*)
        *(.sdata .sdata.*)
        *(.got .got.*)
    }

    .tdata : ALIGN(0x10) {
        _stdata = .;
        *(.tdata .tdata.*)
        _etdata = .;
    }

    .tbss : ALIGN(0x10) {
        _stbss = .;
        *(.tbss .tbss.*)
        *(.tcommon)
        _etbss = .;
    }

    . = ALIGN(4K);
    _percpu_start = .;
    _percpu_end = _percpu_start + SIZEOF(.percpu);
    .percpu 0x0 : AT(_percpu_start) {
        _percpu_load_start = .;
        *(.percpu .percpu.*)
        _percpu_load_end = .;
        . = _percpu_load_start + ALIGN(64) * 1;
    }
    . = _percpu_end;

    . = ALIGN(4K);
    _edata = .;

    .bss : AT(.) ALIGN(4K) {
        boot_stack = .;
        *(.bss.stack)
        . = ALIGN(4K);
        boot_stack_top = .;

        _sbss = .;
        *(.bss .bss.*)
        *(.sbss .sbss.*)
        *(COMMON)
        . = ALIGN(4K);
        _ebss = .;
    }

    _ekernel = .;

	/DISCARD/ : {
        *(.comment) *(.gnu*) *(.note*) *(.eh_frame*)
    }
}

SECTIONS {
    linkme_IRQ : { *(linkme_IRQ) }
    linkm2_IRQ : { *(linkm2_IRQ) }
    linkme_PAGE_FAULT : { *(linkme_PAGE_FAULT) }
    linkm2_PAGE_FAULT : { *(linkm2_PAGE_FAULT) }
    linkme_SYSCALL : { *(linkme_SYSCALL) }
    linkm2_SYSCALL : { *(linkm2_SYSCALL) }
    axns_resource : { *(axns_resource) }
}
INSERT AFTER .tbss;
