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

    .section .text.trap.expected, "ax"
    .balign 4
    .global __rustsbi_prototyper_expected_trap
    .type __rustsbi_prototyper_expected_trap, @function
__rustsbi_prototyper_expected_trap:
    # a3 is the machine-owned current-hart record and a4 is the sole scratch
    # register. The leaf's inline-assembly contract declares both clobbers.
    LOAD a4, {armed}, a3
    addi a4, a4, -1
    bnez a4, .Lexpected_fail

    # The contained instruction executes in M-mode with MPRV clear. The leaf
    # checked MPRV before vector installation; entry verifies trap origin M.
    csrr a4, mstatus
    srli a4, a4, 11
    andi a4, a4, 3
    addi a4, a4, -3
    bnez a4, .Lexpected_fail

    csrr a4, mcause
    STORE a4, {cause}, a3
    csrr a4, mtval
    STORE a4, {tval}, a3

    LOAD a4, {flags}, a3
    andi a4, a4, {flag_hypervisor}
    beqz a4, .Lexpected_no_h
    csrr a4, 0x34b
    STORE a4, {tval2}, a3
    csrr a4, 0x34a
    STORE a4, {tinst}, a3
    .if {word_size} == 8
        csrr a4, mstatus
        srli a4, a4, 38
    .else
        csrr a4, mstatush
        srli a4, a4, 6
    .endif
    andi a4, a4, 1
    STORE a4, {gva}, a3
    j .Lexpected_snapshot_done

.Lexpected_no_h:
    STORE zero, {tval2}, a3
    STORE zero, {tinst}, a3
    STORE zero, {gva}, a3

.Lexpected_snapshot_done:
    li a4, 1
    STORE a4, {faulted}, a3
    LOAD a4, {resume_pc}, a3
    csrw mepc, a4

    # Preserve a3 as the record pointer across mret. The resume label validates
    # the captured cause before restoring any CSR; clobbering a3 here would make
    # that continuation dereference the cause number as an address.
    mret

.Lexpected_fail:
    csrw mie, zero
.Lexpected_fail_loop:
    wfi
    j .Lexpected_fail_loop

    .size __rustsbi_prototyper_expected_trap, .-__rustsbi_prototyper_expected_trap

    .option pop
