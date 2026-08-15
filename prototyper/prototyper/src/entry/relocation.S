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
   ld  t4, 8(t0)
   bne t4, t3, 2f
   ld t4, 0(t0)  // Get target address
   ld t5, 16(t0) // Get addend
   add t4, t4, t2 // Add load offset to the target address
   add t5, t5, t2 // Add load offset to the addend
   sd t5, 0(t4)  // Write the relocated address
2:
   addi t0, t0, 24 // Get next rela item
   blt t0, t1, 1b
   fence.i

// Return
   ret
