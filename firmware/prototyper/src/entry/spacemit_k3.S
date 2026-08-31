// SpacemiT K3 warm-boot entry for harts released through cluster RVBADDR.
// Data cache and snoop must be enabled before AMOs or Rust code execute.
// BSS initialization and boot-hart election are already complete.

.option arch, +a

// Disable all interrupts.
csrw    mie, zero

// Set the audio buffer to cacheable.
li      t0, 0xff
slli    t0, t0, 48
csrc    0x7de, t0
li      t0, 0x20
slli    t0, t0, 48
csrs    0x7de, t0
sfence.vma

// Set the XIP address to the I/O memory attribute.
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

// Enable the data and instruction caches. Some instructions, including
// AMOs, depend on the data cache being enabled.
li      a5, 0x10073
csrs    0x7c0, a5
fence   iorw, iorw
// Synchronize this hart's instruction cache with the loaded image.
fence.i

// Enable the hypervisor extension for X100 (hart ID < 8); A100 skips it.
csrr    t0, mhartid
li      t1, 8
bgeu    t0, t1, 1f
csrr    t2, misa
li      t3, 1
slli    t3, t3, 7      // 'H' - 'A'
or      t2, t2, t3
csrw    misa, t2
1:

// A PMU-woken hart carries unspecified values in a0-a2. The secondary path
// decodes a1/a2 into BootInfo, so provide a clean secondary-hart envelope.
li      a1, 0
li      a2, 0
call    {locate_stack}
call    {main}
csrw    mscratch, sp
j       {hart_boot}
