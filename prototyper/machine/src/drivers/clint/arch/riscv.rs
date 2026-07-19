//! RISC-V CLINT-local CSR and ordering operations.

pub(in crate::drivers::clint) fn device_fence() {
    // SAFETY: conservative I/O and memory ordering fence.
    unsafe { core::arch::asm!("fence iorw, iorw", options(nostack)) };
}

pub(in crate::drivers::clint) fn enable_machine_timer() {
    const MTIE: usize = 1 << 7;
    const STIP: usize = 1 << 5;
    // SAFETY: clears the prior S timer manifestation before enabling MTIE.
    unsafe {
        core::arch::asm!(
            "csrc mip, {stip}",
            "csrs mie, {mtie}",
            stip = in(reg) STIP,
            mtie = in(reg) MTIE,
            options(nostack),
        )
    }
}

pub(in crate::drivers::clint) fn current_hart_id() -> usize {
    let hart_id;
    // SAFETY: `mhartid` is a mandatory read-only machine CSR.
    unsafe {
        core::arch::asm!("csrr {hart_id}, mhartid", hart_id = out(reg) hart_id, options(nomem, nostack))
    };
    hart_id
}

pub(in crate::drivers::clint) fn enable_machine_software_interrupt() {
    const MSIE: usize = 1 << 3;
    // SAFETY: the claimed CLINT source was cleared before this local enable.
    unsafe { core::arch::asm!("csrs mie, {msie}", msie = in(reg) MSIE, options(nostack)) }
}

pub(in crate::drivers::clint) fn manifest_supervisor_timer() {
    const MTIE: usize = 1 << 7;
    const STIP: usize = 1 << 5;
    // SAFETY: masks MTIE before publishing the supervisor pending bit.
    unsafe {
        core::arch::asm!(
            "csrc mie, {mtie}",
            "csrs mip, {stip}",
            mtie = in(reg) MTIE,
            stip = in(reg) STIP,
            options(nostack),
        )
    }
}

pub(in crate::drivers::clint) fn read_time_csr() -> u64 {
    #[cfg(target_pointer_width = "64")]
    {
        let value;
        // SAFETY: `time` is read-only.
        unsafe {
            core::arch::asm!("rdtime {value}", value = out(reg) value, options(nomem, nostack))
        };
        value
    }
    #[cfg(target_pointer_width = "32")]
    loop {
        let high_before: u32;
        let low: u32;
        let high_after: u32;
        // SAFETY: the high-low-high sequence is the stable RV32 time read.
        unsafe {
            core::arch::asm!(
                "rdtimeh {high_before}",
                "rdtime {low}",
                "rdtimeh {high_after}",
                high_before = out(reg) high_before,
                low = out(reg) low,
                high_after = out(reg) high_after,
                options(nomem, nostack),
            )
        };
        if high_before == high_after {
            break (u64::from(high_after) << 32) | u64::from(low);
        }
    }
}
