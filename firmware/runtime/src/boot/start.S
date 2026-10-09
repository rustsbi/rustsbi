// Machine-mode startup from reset to per-hart Rust initialization.
//
// The 6:/7: signal words are read before BSS is cleared, so they must live
// in .text.entry (loaded content), not in BSS.

.option arch, +a

// 1. Mask all interrupts and install the Runtime's stack-independent
//    fail-stop vector: any trap before per-hart initialization parks.
li      t0, {entry_mstatus_clear}
csrc    mstatus, t0
csrw    mie, zero
csrw    mscratch, zero
lla     t0, {early_vector}
csrw    mtvec, t0

// 2. Elect a bootstrap hart: the first hart to arrive wins the race
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
    amoadd.w.rl t0, t1, 0(t0)
    j       5f

// 4. Secondary harts wait for the bss-ready signal.
4:
    lla     t0, 7f
    lw      t0, 0(t0)
    beqz    t0, 4b
    fence   r, rw

// 5. Serialize boot candidates on the linker-owned stack. The relocation
//    winner need not be the boot hart designated by fw_dynamic.
5:
    mv      s2, a1
    mv      s3, a2
8:
    la      t0, {boot_ready}
    lw      t1, 0(t0)
    bnez    t1, 11f
    la      t0, {boot_lock}
    li      t1, 1
    amoswap.w.aq t1, t1, 0(t0)
    bnez    t1, 8b
    // Recheck after acquiring the lock: the owner may have published while
    // this hart was attempting to acquire it.
    la      t0, {boot_ready}
    lw      t1, 0(t0)
    bnez    t1, 10f
    lla     sp, sbi_boot_stack_end
    mv      a0, s2
    mv      a1, s3
    call    {initialize}
    beqz    a0, 10f
    // Discovery's Rust frames are dead before BOOT_READY releases anyone.
    call    {locate_stack}
    la      t0, {boot_ready}
    li      t1, 1
    amoswap.w.rl zero, t1, 0(t0)
    j       12f
10:
    la      t0, {boot_lock}
    amoswap.w.rl zero, zero, 0(t0)
9:
    la      t0, {boot_ready}
    lw      t1, 0(t0)
    beqz    t1, 9b
11:
    fence   r, rw
    call    {locate_stack}
12:
    mv      a0, s2
    mv      a1, s3
    call    {main}
    j       {finish_boot}
    .balign  4

6: // Relocation election signal word.
  .word    0
7: // BSS-ready signal word.
  .word    0
