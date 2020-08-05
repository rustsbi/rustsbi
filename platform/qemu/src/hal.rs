// Ref: MeowSBI

mod ns16550a;
pub use ns16550a::Ns16550a;

mod clint;
pub use clint::Clint;

// Ref: https://github.com/repnop/vanadinite/blob/651163fd435d97dc9de728279b64176cdd46ec28/src/arch/virt/mod.rs#L45-L71

pub struct Reset;

impl rustsbi::Reset for Reset {
    fn reset(&self) -> ! {
        // todo: only exit after all harts finished
        // loop {}
        const VIRT_TEST: *mut u64 = 0x10_0000 as *mut u64;
        // Fail = 0x3333,
        // Pass = 0x5555,
        // Reset = 0x7777,
        unsafe {
            core::ptr::write_volatile(VIRT_TEST, 0x5555);
        }
        unreachable!()
    }
}
