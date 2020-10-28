# K210 support module

Kendryte K210 is a dual-core RISC-V RV64GC chip with hardware accelerated AI peripheral.
According to its manual, K210 is taped out in TSMC 7nm and can speed up to 400MHz.

## Implementation details

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

## Implementation specific SBI functions

To solve the issue 3 in previous section, RustSBI's current implementation includes a RustSBI specific
SBI call as a function. 

The K210 supervisor-level external interrupt handler register function is declared as:

```rust
fn sbi_rustsbi_k210_sext(phys_addr: usize) -> SbiRet;
```

This function registers a device interrupt handler to machine level environment. 
On any machine-level external interrupt, the RustSBI's K210 environment would call the function provided.

The function's physical address shall be stored in register `a0` before calling this function.
RustSBI will regard `a0` as a function without any parameters and any return values, or a `phys_addr: fn()`.

This function will always return `SbiRet` value of zero and error code of `SBI_SUCCESS`.

### Function Listing

According to RISC-V SBI specification:

> Firmware Code Base Specific SBI Extension Space, Extension Ids 0x0A000000 through 0x0AFFFFFF
> 
> Low bits is SBI implementation ID. The firmware code base SBI extension is the additional SBI extensions to SBI
> implementation. That provides the firmware code base specific SBI functions which are defined in the external
> firmware specification.

Since RustSBI has the implementation ID 4, its specific SBI extension is `0x0A000004`. We add the function
mentioned above to this specific SBI extension.

| Function Name | Function ID | Extension ID |
|:-----|:----|:----|
| sbi_rustsbi_k210_sext | 0x210 | 0x0A000004 |
