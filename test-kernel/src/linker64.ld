/* Copy from bbl-ucore : https://ring00.github.io/bbl-ucore      */

/* Simple linker script for the ucore kernel.
   See the GNU ld 'info' manual ("info ld") to learn the syntax. */

OUTPUT_ARCH(riscv)
ENTRY(_start)

BASE_ADDRESS = 0x80200000;

SECTIONS
{
    /* Load the kernel at this address: "." means the current address */
    . = BASE_ADDRESS;
    start = .;

    .text : ALIGN(4K) {
        _stext = .;
        *(.text.entry)
        *(.text .text.*)
        _etext = .;
    }

    .rodata : ALIGN(4K) {
        _srodata = .;
        *(.rodata .rodata.*)
        _erodata = .;
    }

    .data : ALIGN(4K) {
        _sdata = .;
        *(.data .data.*)
        _edata = .;
    }

    .bss (NOLOAD) : ALIGN(4K)  {
        _sbss = .;
        *(.sbss .bss .bss.*)
        _ebss = .;
    }

    PROVIDE(end = .);
}
