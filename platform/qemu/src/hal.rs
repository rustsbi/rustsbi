// Ref: MeowSBI

mod ns16550a;
pub use ns16550a::Ns16550a;

mod clint;
pub use clint::Clint;

// Ref: https://github.com/repnop/vanadinite/blob/651163fd435d97dc9de728279b64176cdd46ec28/src/arch/virt/mod.rs#L45-L71

pub struct Reset;

const TEST_FAIL: u32 = 0x3333;
const TEST_PASS: u32 = 0x5555;
const TEST_RESET: u32 = 0x7777;

impl rustsbi::Reset for Reset {
    fn system_reset(&self, reset_type: usize, reset_reason: usize) -> rustsbi::SbiRet {
        // todo: only exit after all harts finished
        // loop {}
        const VIRT_TEST: *mut u32 = 0x10_0000 as *mut u32;
        // Fail = 0x3333,
        // Pass = 0x5555,
        // Reset = 0x7777,
        let mut value = match reset_type {
            rustsbi::reset::RESET_TYPE_SHUTDOWN => TEST_PASS,
            rustsbi::reset::RESET_TYPE_COLD_REBOOT => TEST_RESET,
            rustsbi::reset::RESET_TYPE_WARM_REBOOT => TEST_RESET,
            _ => TEST_FAIL,
        };
        if reset_reason == rustsbi::reset::RESET_REASON_SYSTEM_FAILURE {
            value = TEST_FAIL;
        };
        unsafe {
            core::ptr::write_volatile(VIRT_TEST, value);
        }
        unreachable!()
    }
}
