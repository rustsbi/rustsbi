// Machine-mode startup from the reset vector to the Rust entry point.
//
// The 6:/7: signal words are read before BSS is cleared, so they must live
// in .text.entry (loaded content), not in BSS.
//
// The cold path must remain first because the FIT entry point is the image
// base. K3 cluster RVBADDR registers target `_start_warm_k3` separately.

.option arch, +a

// 1. Mask all interrupts.
csrw    mie, zero

// Flush TLB and I-cache before a PMU-woken hart executes the warm path.
        sfence.vma
        fence.i

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

// PMU-woken harts fetch this entry from their cluster RVBADDR. Data cache and
// snoop must be enabled before AMOs or Rust code execute. BSS and boot-hart
// election are already complete.
    .balign  4
    .globl _start_warm_k3
_start_warm_k3:
    // Mask all interrupts.
    csrw    mie, zero

    // Configure the audio window as cacheable.
    li      t0, 0xff
    slli    t0, t0, 48
    csrc    0x7de, t0
    li      t0, 0x20
    slli    t0, t0, 48
    csrs    0x7de, t0
    sfence.vma

    // Configure the XIP window as device memory.
    lui     t0, 0xff0
    csrc    0x7de, t0
    lui     t0, 0x220
    csrs    0x7de, t0
    sfence.vma

    // Enable core snoop for this hart.
    csrw    0x7f0, zero
    csrr    t0, mhartid
    andi    t1, t0, 3
    li      a5, 1
    sllw    a5, a5, t1
    csrs    0x7f0, a5

    // Enable instruction and data caches before executing AMOs.
    li      a5, 0x10073
    csrs    0x7c0, a5
    fence   iorw, iorw

    // Only X100 harts implement the hypervisor extension.
    csrr    t0, mhartid
    li      t1, 8
    bgeu    t0, t1, 20f
    csrr    t2, misa
    li      t3, 1
    slli    t3, t3, 7      // 'H' - 'A'
    or      t2, t2, t3
    csrw    misa, t2
20:
    // PMU wakeup does not preserve the boot envelope; zero it before joining
    // the common stack and Rust entry path.
    li      a1, 0
    li      a2, 0
    j       5b
