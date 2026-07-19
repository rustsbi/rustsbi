    .option push
    .option norvc
    .option arch, +a

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
    # `mscratch` is the initialized trap-only state. Preserve every temporary
    # before using it, and retain the interrupted sp in mscratch until saved.
    csrrw sp, mscratch, sp
    STORE t0, {saved_t0}, sp
    STORE t1, {saved_t1}, sp
    STORE t2, {saved_t2}, sp
    STORE t3, {saved_t3}, sp
    csrr t0, mscratch
    STORE t0, {saved_sp}, sp

    lw t0, 0(sp)
    li t1, {trap_state_ready}
    bne t0, t1, .Ltrap_fail

    # A first-level trap starts at the private trap-stack top, including a
    # notification that interrupts the M-mode warm loop. An M-origin trap with
    # a published frame is nested and allocates below the interrupted stack sp.
    csrr t2, mstatus
    srli t2, t2, 11
    andi t2, t2, 3
    li t3, 3
    bne t2, t3, .Ltrap_primary
    LOAD t0, {depth}, sp
    beqz t0, .Ltrap_primary
    j .Ltrap_nested

.Ltrap_primary:
    LOAD t0, {depth}, sp
    bnez t0, .Ltrap_fail
    LOAD t0, {current_frame}, sp
    bnez t0, .Ltrap_fail
    LOAD t1, {stack_top}, sp
    li t0, {frame_size}
    sub t1, t1, t0
    li t0, {frame_align_mask}
    and t1, t1, t0
    LOAD t0, {stack_bottom}, sp
    bltu t1, t0, .Ltrap_fail
    li t2, 0
    li t3, 1
    j .Ltrap_frame

.Ltrap_nested:
    LOAD t0, {depth}, sp
    li t1, 1
    bne t0, t1, .Ltrap_fail
    LOAD t2, {current_frame}, sp
    beqz t2, .Ltrap_fail
    LOAD t1, {saved_sp}, sp
    LOAD t0, {stack_top}, sp
    bltu t0, t1, .Ltrap_fail
    LOAD t0, {stack_bottom}, sp
    bgeu t0, t1, .Ltrap_fail
    li t0, {frame_size}
    sub t1, t1, t0
    li t0, {frame_align_mask}
    and t1, t1, t0
    LOAD t0, {stack_bottom}, sp
    bltu t1, t0, .Ltrap_fail
    li t3, 2

.Ltrap_frame:
    # t1 is the checked candidate frame, t2 the opaque previous-frame value,
    # t3 the admitted depth, and sp remains the state until publication.
    STORE t2, {previous}, t1
    STORE zero, {x0}, t1
    STORE ra, {x1}, t1
    LOAD t0, {saved_sp}, sp
    STORE t0, {x2}, t1
    STORE gp, {x3}, t1
    STORE tp, {x4}, t1
    LOAD t0, {saved_t0}, sp
    STORE t0, {x5}, t1
    LOAD t0, {saved_t1}, sp
    STORE t0, {x6}, t1
    LOAD t0, {saved_t2}, sp
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
    LOAD t0, {saved_t3}, sp
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

    LOAD t0, {flags}, sp
    andi t0, t0, {flag_hypervisor}
    beqz t0, .Ltrap_no_h
    csrr t0, 0x34b
    STORE t0, {tval2}, t1
    csrr t0, 0x34a
    STORE t0, {tinst}, t1
    .if {word_size} == 8
        LOAD t0, {mstatus}, t1
        srli t0, t0, 38
    .else
        LOAD t0, {mstatus_high}, t1
        srli t0, t0, 6
    .endif
    andi t0, t0, 1
    STORE t0, {gva}, t1
    j .Ltrap_publish

.Ltrap_no_h:
    STORE zero, {tval2}, t1
    STORE zero, {tinst}, t1
    STORE zero, {gva}, t1

.Ltrap_publish:
    # Every frame field is initialized before this current-frame relation is
    # published. Only now may MDT be relaxed and Rust observe the frame.
    STORE t1, {current_frame}, sp
    STORE t4, {depth}, sp
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
    mv a2, t4
    mv sp, t1
    call {dispatch}

.Ltrap_fail:
    csrw mie, zero
.Ltrap_fail_loop:
    wfi
    j .Ltrap_fail_loop
    .size __rustsbi_prototyper_trap_entry, .-__rustsbi_prototyper_trap_entry

    .align 2
    .globl __rustsbi_prototyper_trap_restore
    .type __rustsbi_prototyper_trap_restore, @function
__rustsbi_prototyper_trap_restore:
    # a0 is the consumed complete frame and a1 its trap-only state. Keep sp on
    # the machine stack through status re-arming and publication removal.
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

    STORE zero, {current_frame}, t0
    STORE zero, {depth}, t0
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
