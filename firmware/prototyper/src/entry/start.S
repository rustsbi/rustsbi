// Machine-mode startup ceremony: from the architectural reset vector to
// safe Rust policy. The `#[entry]` attribute macro wraps this file into the
// `_start` entry point; steps are marked inline.
//
// The 6:/7: signal words are read before BSS is cleared, so they must live
// in .text.entry (loaded content), not in BSS.

.option arch, +a

// 1. Mask all interrupts.
csrw    mie, zero

// 2. Elect a single boot hart: the first hart to arrive wins the race
// (amoadd returns the previous value; nonzero means another hart won).
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
    sd      zero, 0(t0)
    addi    t0, t0, 8
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

// 5. Every hart locates its stack and enters the policy entry.
// 6. On return, publish the stack in mscratch and join the boot flow.
5:
    call    {locate_stack}
    call    {main}
    csrw    mscratch, sp
    j       {hart_boot}
    .balign  4

6: // boot hart race signal word.
  .word    0
7: // bss ready signal word.
  .word    0
