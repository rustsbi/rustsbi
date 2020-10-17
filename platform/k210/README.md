# K210 support module

Kendryte K210 is a dual-core RISC-V RV64GC chip with hardware accelerated AI peripheral.
According to its manual, K210 is taped out in TSMC 7nm and can speed up to 400MHz.

The K210 SoC implements version 1.9.1 of RISC-V's privileged specification.
This version differents from latest version (by current version 1.11) in the following aspects:

1. Register `sstatus.sum` (in 1.11) or `sstatus.pum` (in 1.9) bits;
2. Instruction `sfence.vma` (1.11) or `sfence.vm` (1.9), register `satp` (1.11) or `sptbr` (1.9);
3. There's no S-level external interrupt in 1.9, but there is in 1.11;
4. Independent page fault exceptions does not exist in 1.9, but they exist in 1.11.

To prolong lifecycle of K210 chip we uses RustSBI as a compatible layer. We emulate sfence.vma 
using sfence.vm to solve issue 2. We modify regster values to solve issue 4. For issue 3, 
we introduce an SBI call local to RustSBI to register S-level interrupt handler by the supervisor 
itself. The issue 1 should left to be concerned by supervisors above SBI implementations.

Machine external handler and timer set calls is modified to meet the requirement of custom S-level
interrupt handlers.

If there are mistakes or missing features in current support module, we welcome further contributions!
