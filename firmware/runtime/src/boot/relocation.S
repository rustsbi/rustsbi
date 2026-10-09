// Apply the linker's position-independent relative relocations.
// RISC-V ELF psABI, Relocations:
// https://riscv-non-isa.github.io/riscv-elf-psabi-doc/#_relocations
// t0: relocation cursor; t1: table end; t2: load offset; t3: relocation kind.

.set .Lrela_word_bytes, {XLEN} / 8
.set .Lrela_entry_bytes, 3 * .Lrela_word_bytes

// The actual load address minus the linker's original image address.
    lla t0, .Llinked_image_start
.if {XLEN} == 64
    ld t0, 0(t0)
.else
    lw t0, 0(t0)
.endif
    lla t1, sbi_start
    sub t2, t1, t0

    lla t0, __rel_dyn_start
    lla t1, __rel_dyn_end
    li t3, {R_RISCV_RELATIVE}
.Lrelocation_loop:
    bgeu t0, t1, .Lrelocation_done
.if {XLEN} == 64
    ld t4, .Lrela_word_bytes(t0) // Elf64_Rela.r_info
    bne t4, t3, .Lnext_relocation
    ld t4, 0(t0)               // Elf64_Rela.r_offset
    ld t5, 2 * .Lrela_word_bytes(t0) // Elf64_Rela.r_addend
.else
    lw t4, .Lrela_word_bytes(t0) // Elf32_Rela.r_info
    bne t4, t3, .Lnext_relocation
    lw t4, 0(t0)               // Elf32_Rela.r_offset
    lw t5, 2 * .Lrela_word_bytes(t0) // Elf32_Rela.r_addend
.endif
    add t4, t4, t2             // Loaded destination address.
    add t5, t5, t2             // Loaded value to store at that destination.
.if {XLEN} == 64
    sd t5, 0(t4)
.else
    sw t5, 0(t4)
.endif
.Lnext_relocation:
    addi t0, t0, .Lrela_entry_bytes
    j .Lrelocation_loop
.Lrelocation_done:
    fence.i
    ret

// This absolute linker value has no Runtime relocation. PC-relative access
// recovers the original link address even when the image is loaded elsewhere.
    .balign 8
.Llinked_image_start:
.if {XLEN} == 64
    .dword sbi_link_start
.else
    .word sbi_link_start
.endif
