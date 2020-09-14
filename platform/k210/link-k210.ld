MEMORY {
    /* 存储单元的物理地址 */
    SRAM : ORIGIN = 0x80000000, LENGTH = 128K
}

_max_hart_id = 1; 

PROVIDE(_stext = 0x80000000);
PROVIDE(_heap_size = 32K);
PROVIDE(_hart_stack_size = 16K);

REGION_ALIAS("REGION_TEXT", SRAM);
REGION_ALIAS("REGION_RODATA", SRAM);
REGION_ALIAS("REGION_DATA", SRAM);
REGION_ALIAS("REGION_BSS", SRAM);
REGION_ALIAS("REGION_HEAP", SRAM);
REGION_ALIAS("REGION_STACK", SRAM);

OUTPUT_ARCH(riscv)

ENTRY(_start)

PROVIDE(_stack_start = ORIGIN(REGION_STACK) + LENGTH(REGION_STACK));

SECTIONS
{
    /* .text 字段 */
    .text _stext : {
        /* 把 entry 函数放在最前面 */
        *(.text.entry)
        /* 要链接的文件的 .text 字段集中放在这里 */
        *(.text .text.*)
        _etext = .;
    } > REGION_TEXT

    /* .rodata 字段 */
    .rodata : ALIGN(4) {
        _srodata = .;
        /* 要链接的文件的 .rodata 字段集中放在这里 */
        *(.rodata .rodata.*)
        . = ALIGN(4);
        _erodata = .;
    } > REGION_RODATA

    /* .data 字段 */
    .data : ALIGN(4) { 
        _sidata = LOADADDR(.data);
        _sdata = .;
        /* Must be called __global_pointer$ for linker relaxations to work. */
        PROVIDE(__global_pointer$ = . + 0x800);
        /* 要链接的文件的 .data 字段集中放在这里 */
        *(.sdata .sdata.* .sdata2 .sdata2.*);
        *(.data .data.*)
        . = ALIGN(4);
        _edata = .;
    } > REGION_DATA

    /* .bss 字段 */
    .bss (NOLOAD) : {
        _sbss = .;
        /* 要链接的文件的 .bss 字段集中放在这里 */
        *(.sbss .bss .bss.*)
        . = ALIGN(4);
        _ebss = .;
    } > REGION_BSS

    .heap (NOLOAD) : {
        _sheap = .;
        . += _heap_size;
        . = ALIGN(4);
        _eheap = .;
    } > REGION_HEAP

    /* fictitious region that represents the memory available for the stack */
    .stack (NOLOAD) : {
        _estack = .;
        . = _stack_start;
        . = ALIGN(4);
        _sstack = .;
    } > REGION_STACK

    /* Discard .eh_frame, we are not doing unwind on panic so it is not needed */
    /DISCARD/ :
    {
        *(.eh_frame .eh_frame_hdr);
    }
}
