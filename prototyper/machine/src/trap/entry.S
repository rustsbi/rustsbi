    .option push
    .option norvc

    .if {word_size} == 8
    .macro LOAD rd, offset, base
        ld \rd, \offset(\base)
    .endm
    .macro STORE rs, offset, base
        sd \rs, \offset(\base)
    .endm
    .else
    .macro LOAD rd, offset, base
        lw \rd, \offset(\base)
    .endm
    .macro STORE rs, offset, base
        sw \rs, \offset(\base)
    .endm
    .endif

    .section .text.trap.entry, "ax"
    .balign 4
    .global __rustsbi_prototyper_trap_entry
    .type __rustsbi_prototyper_trap_entry, @function
__rustsbi_prototyper_trap_entry:
    # mscratch contains only this hart's fixed trap-stack top. The small
    # top-of-stack scratch area preserves temporaries while entry selects the
    # fixed primary frame or the disjoint emergency frame.
    csrrw sp, mscratch, sp
    STORE t0, -{scratch_t0}, sp
    STORE t1, -{scratch_t1}, sp
    STORE t2, -{scratch_t2}, sp
    STORE t3, -{scratch_t3}, sp
    csrr t0, mscratch
    STORE t0, -{scratch_sp}, sp

    # An interrupted sp inside this private trap stack denotes a nested
    # machine trap. The bottom frame is reserved for its bounded fatal path.
    li t1, {stack_size}
    sub t2, sp, t1
    li t1, {frame_size}
    add t3, t2, t1
    bltu t0, t3, .Ltrap_primary
    bgeu t0, sp, .Ltrap_primary
    mv t1, t2
    li t3, 1
    j .Ltrap_save_frame

.Ltrap_primary:
    li t0, {primary_frame_offset}
    sub t1, sp, t0
    li t3, 0

.Ltrap_save_frame:
    STORE zero, {previous}, t1
    STORE zero, {x0}, t1
    STORE ra, {x1}, t1
    LOAD t0, -{scratch_sp}, sp
    STORE t0, {x2}, t1
    STORE gp, {x3}, t1
    STORE tp, {x4}, t1
    LOAD t0, -{scratch_t0}, sp
    STORE t0, {x5}, t1
    LOAD t0, -{scratch_t1}, sp
    STORE t0, {x6}, t1
    LOAD t0, -{scratch_t2}, sp
    STORE t0, {x7}, t1
    STORE s0, {x8}, t1
    STORE s1, {x9}, t1
    STORE a0, {x10}, t1
    STORE a1, {x11}, t1
    STORE a2, {x12}, t1
    STORE a3, {x13}, t1
    STORE a4, {x14}, t1
    STORE a5, {x15}, t1
    STORE a6, {x16}, t1
    STORE a7, {x17}, t1
    STORE s2, {x18}, t1
    STORE s3, {x19}, t1
    STORE s4, {x20}, t1
    STORE s5, {x21}, t1
    STORE s6, {x22}, t1
    STORE s7, {x23}, t1
    STORE s8, {x24}, t1
    STORE s9, {x25}, t1
    STORE s10, {x26}, t1
    STORE s11, {x27}, t1
    LOAD t0, -{scratch_t3}, sp
    STORE t0, {x28}, t1
    STORE t4, {x29}, t1
    STORE t5, {x30}, t1
    STORE t6, {x31}, t1
    mv t4, t3

    csrr t0, mepc
    STORE t0, {mepc}, t1
    csrr t0, mstatus
    STORE t0, {mstatus}, t1
    .if {word_size} == 4
        csrr t0, mstatush
        STORE t0, {mstatus_high}, t1
    .else
        STORE zero, {mstatus_high}, t1
    .endif
    csrr t0, mcause
    STORE t0, {cause}, t1
    csrr t0, mtval
    STORE t0, {tval}, t1
    # H-extension metadata is captured by the first Rust instruction path only
    # after the per-hart feature fact has been proved.
    STORE zero, {tval2}, t1
    STORE zero, {tinst}, t1
    STORE zero, {gva}, t1

    bnez t4, .Ltrap_fatal

    # Re-arm mscratch before Rust and permit a nested machine trap to take the
    # disjoint assembly-only fatal path.
    csrw mscratch, sp
    .if {word_size} == 8
        li t0, 0x40000000000
        csrc mstatus, t0
    .else
        li t0, 0x400
        csrc mstatush, t0
    .endif
    mv a0, t1
    mv a1, sp
    mv sp, t1
    call {dispatch}

.Ltrap_fatal:
    csrw mscratch, sp
    csrw mie, zero
.Ltrap_fatal_loop:
    wfi
    j .Ltrap_fatal_loop
    .size __rustsbi_prototyper_trap_entry, .-__rustsbi_prototyper_trap_entry

    .align 2
    .global __rustsbi_prototyper_trap_restore
    .type __rustsbi_prototyper_trap_restore, @function
__rustsbi_prototyper_trap_restore:
    # a0 is the consumed primary frame and a1 the fixed trap-stack top.
    mv sp, a0
    mv t0, a1
    .if {word_size} == 4
        LOAD t1, {mstatus_high}, sp
        li t2, 0x400
        or t1, t1, t2
        csrw mstatush, t1
    .endif
    LOAD t1, {mstatus}, sp
    li t2, 0x20008
    not t2, t2
    and t1, t1, t2
    .if {word_size} == 8
        li t2, 0x40000000000
        or t1, t1, t2
    .endif
    csrw mstatus, t1
    csrw mscratch, t0
    LOAD t1, {mepc}, sp
    csrw mepc, t1

    LOAD ra, {x1}, sp
    LOAD gp, {x3}, sp
    LOAD tp, {x4}, sp
    LOAD s0, {x8}, sp
    LOAD s1, {x9}, sp
    LOAD a0, {x10}, sp
    LOAD a1, {x11}, sp
    LOAD a2, {x12}, sp
    LOAD a3, {x13}, sp
    LOAD a4, {x14}, sp
    LOAD a5, {x15}, sp
    LOAD a6, {x16}, sp
    LOAD a7, {x17}, sp
    LOAD s2, {x18}, sp
    LOAD s3, {x19}, sp
    LOAD s4, {x20}, sp
    LOAD s5, {x21}, sp
    LOAD s6, {x22}, sp
    LOAD s7, {x23}, sp
    LOAD s8, {x24}, sp
    LOAD s9, {x25}, sp
    LOAD s10, {x26}, sp
    LOAD s11, {x27}, sp
    LOAD t3, {x28}, sp
    LOAD t4, {x29}, sp
    LOAD t5, {x30}, sp
    LOAD t6, {x31}, sp
    LOAD t0, {x5}, sp
    LOAD t1, {x6}, sp
    LOAD t2, {x7}, sp
    LOAD sp, {x2}, sp
    mret
    .size __rustsbi_prototyper_trap_restore, .-__rustsbi_prototyper_trap_restore

    .option pop
