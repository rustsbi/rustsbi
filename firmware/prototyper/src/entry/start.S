// Machine-mode startup ceremony: from the architectural reset vector to
// safe Rust policy. The `#[entry]` attribute macro wraps this file into the
// `_start` entry point; steps are marked inline.
//
// The 6:/7: signal words are read before BSS is cleared, so they must live
// in .text.entry (loaded content), not in BSS.

.option arch, +a

// 1. Mask all interrupts and install the Runtime's stack-independent
//    fail-stop vector: any trap before per-hart initialization parks.
csrw    mie, zero
la      t0, {early_vector}
csrw    mtvec, t0

// 2. Ask the boot protocol for a preferred relocation hart, then elect one.
// -1 means no preference; the first arriving hart wins the lottery. Save a0
// across the helper call because it is the current hart ID and must reach Rust.
    mv       s0, a0
    call     {preferred_relocation_hart}
    mv       t2, a0
    mv       a0, s0
    li       t1, -1
    beq      t2, t1, 8f
    bne      a0, t2, 4f
8:
    lla      t0, 6f
    li       t1, 1
    amoadd.w t0, t1, 0(t0)
    bnez     t0, 4f
    call     {relocation_update}

// 3. The elected hart zeroes BSS and releases the bss-ready signal.
1:
    lla     t0, sbi_bss_start
    lla     t1, sbi_bss_end
2:
    bgeu    t0, t1, 3f
.if {XLEN} == 64
    sd      zero, 0(t0)
    addi    t0, t0, 8
.else
    sw      zero, 0(t0)
    addi    t0, t0, 4
.endif
    j       2b
3:
    lla     t0, 7f
    li      t1, 1
    amoadd.w t0, t1, 0(t0)
    j       5f

// 4. Secondary harts wait for the bss-ready signal.
4:
    lla     t0, 7f
    lw      t0, 0(t0)
    beqz    t0, 4b

// 5. Every hart locates its Runtime-owned stack and enters the policy
//    entry. When main returns, boot is over: the Runtime's finish_boot
//    discards the boot call chain, arms the reused stack as the trap
//    stack, and enters the staged S/HS next stage (or parks until one is
//    staged); mscratch keeps the zero sentinel until then.
5:
    call    {locate_stack}
    call    {main}
    j       {finish_boot}
    .balign  4

6: // boot hart race signal word.
  .word    0
7: // bss ready signal word.
  .word    0
