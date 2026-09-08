// Handle relocations for position-independent code.

// Get load offset.
   li t0, {START_ADDRESS}
   lla t1, sbi_start
   sub t2, t1, t0

// Foreach rela.dyn and update relocation.
   lla t0, __rel_dyn_start
   lla t1, __rel_dyn_end
   li  t3, {R_RISCV_RELATIVE}
1:
   bgeu t0, t1, 3f
.if {XLEN} == 64
   ld  t4, 8(t0)
   bne t4, t3, 2f
   ld t4, 0(t0)  // Get target address
   ld t5, 16(t0) // Get addend
   add t4, t4, t2 // Add load offset to the target address
   add t5, t5, t2 // Add load offset to the addend
   sd t5, 0(t4)  // Write the relocated address
2:
   addi t0, t0, 24 // Get next rela item
.else
   lw t4, 4(t0)  // Elf32_Rela.r_info
   bne t4, t3, 2f
   lw t4, 0(t0)  // Elf32_Rela.r_offset
   lw t5, 8(t0)  // Elf32_Rela.r_addend
   add t4, t4, t2
   add t5, t5, t2
   sw t5, 0(t4)
2:
   addi t0, t0, 12
.endif
   j 1b
3:
   fence.i

// Return
   ret
