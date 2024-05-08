use sifive_test_device::SifiveTestDevice;
use spin::Mutex;

static RESET: Mutex<MachineReset> = Mutex::new(MachineReset::DeadLoop);

pub fn fail() -> ! {
    let lock = RESET.lock();
    match *lock {
        MachineReset::DeadLoop => {
            trace!("test fail, begin dead loop");
            loop {
                core::hint::spin_loop()
            }
        }
        MachineReset::SifiveTest(test) => {
            trace!("test fail, invoke process exit procedure on SiFive Test device");
            unsafe { &*test }.fail(0)
        }
    }
}

enum MachineReset {
    DeadLoop,
    #[allow(unused)] // TODO use on FDT parsing
    SifiveTest(*const SifiveTestDevice),
}

unsafe impl Send for MachineReset {}
unsafe impl Sync for MachineReset {}
