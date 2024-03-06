use crate::{base, dbcn, hsm, spi, time};
use log::*;

/// Automatic SBI testing with logging enabled.
pub struct Testing {
    /// The hart ID to test most of single core extensions.
    pub hartid: usize,
    /// A list of harts to test Hart State Monitor extension.
    pub hart_mask: usize,
    /// Base of hart list to test Hart State Monitor extension.
    pub hart_mask_base: usize,
    /// Delay value to test Timer programmer extension.
    pub delay: u64,
}

const TARGET: &str = "sbi-testing";

impl Testing {
    /// Start testing process of RISC-V SBI implementation.
    pub fn test(self) -> bool {
        let mut result = true;
        base::test(|case| {
            use base::Case::*;
            match case {
                NotExist => panic!("Sbi `Base` not exist"),
                Begin => info!(target: TARGET, "Testing `Base`"),
                Pass => info!(target: TARGET, "Sbi `Base` test pass"),
                GetSbiSpecVersion(version) => {
                    info!(target: TARGET, "sbi spec version = {version}");
                }
                GetSbiImplId(Ok(name)) => {
                    info!(target: TARGET, "sbi impl = {name}");
                }
                GetSbiImplId(Err(unknown)) => {
                    warn!(target: TARGET, "unknown sbi impl = {unknown:#x}");
                }
                GetSbiImplVersion(version) => {
                    info!(target: TARGET, "sbi impl version = {version:#x}");
                }
                ProbeExtensions(exts) => {
                    info!(target: TARGET, "sbi extensions = {exts}");
                }
                GetMvendorId(id) => {
                    info!(target: TARGET, "mvendor id = {id:#x}");
                }
                GetMarchId(id) => {
                    info!(target: TARGET, "march id = {id:#x}");
                }
                GetMimpId(id) => {
                    info!(target: TARGET, "mimp id = {id:#x}");
                }
            }
        });
        time::test(self.delay, |case| {
            use time::Case::*;
            match case {
                NotExist => {
                    error!(target: TARGET, "Sbi `TIME` not exist");
                    result = false;
                }
                Begin => info!(target: TARGET, "Testing `TIME`"),
                Pass => info!(target: TARGET, "Sbi `TIME` test pass"),
                Interval { begin: _, end: _ } => {
                    info!(
                        target: TARGET,
                        "read time register successfully, set timer +1s"
                    );
                }
                ReadFailed => {
                    error!(target: TARGET, "csrr time failed");
                    result = false;
                }
                TimeDecreased { a, b } => {
                    error!(target: TARGET, "time decreased: {a} -> {b}");
                    result = false;
                }
                SetTimer => {
                    info!(target: TARGET, "timer interrupt delegate successfully");
                }
                UnexpectedTrap(trap) => {
                    error!(
                        target: TARGET,
                        "expect trap at supervisor timer, but {trap:?} was caught"
                    );
                    result = false;
                }
            }
        });
        spi::test(self.hartid, |case| {
            use spi::Case::*;
            match case {
                NotExist => {
                    error!(target: TARGET, "Sbi `sPI` not exist");
                    result = false;
                }
                Begin => info!(target: TARGET, "Testing `sPI`"),
                Pass => info!(target: TARGET, "Sbi `sPI` test pass"),
                SendIpi => info!(target: TARGET, "send ipi successfully"),
                UnexpectedTrap(trap) => {
                    error!(
                        target: TARGET,
                        "expect trap at supervisor soft, but {trap:?} was caught"
                    );
                    result = false;
                }
            }
        });
        hsm::test(self.hartid, self.hart_mask, self.hart_mask_base, |case| {
            use hsm::Case::*;
            match case {
                NotExist => {
                    error!(target: TARGET, "Sbi `HSM` not exist");
                    result = false;
                }
                Begin => info!(target: TARGET, "Testing `HSM`"),
                Pass => info!(target: TARGET, "Sbi `HSM` test pass"),
                HartStartedBeforeTest(id) => warn!(target: TARGET, "hart {id} already started"),
                NoStoppedHart => warn!(target: TARGET, "no stopped hart"),
                BatchBegin(batch) => info!(target: TARGET, "Testing harts: {batch:?}"),
                HartStarted(id) => debug!(target: TARGET, "hart {id} started"),
                HartStartFailed { hartid, ret } => {
                    error!(target: TARGET, "hart {hartid} start failed: {ret:?}");
                    result = false;
                }
                HartSuspendedNonretentive(id) => {
                    debug!(target: TARGET, "hart {id} suspended nonretentive")
                }
                HartResumed(id) => debug!(target: TARGET, "hart {id} resumed"),
                HartSuspendedRetentive(id) => {
                    debug!(target: TARGET, "hart {id} suspended retentive")
                }
                HartStopped(id) => debug!(target: TARGET, "hart {id} stopped"),
                BatchPass(batch) => info!(target: TARGET, "Testing Pass: {batch:?}"),
            }
        });
        dbcn::test(|case| {
            use dbcn::Case::*;
            match case {
                NotExist => {
                    error!(target: TARGET, "Sbi `DBCN` not exist");
                    result = false;
                }
                Begin => info!(target: TARGET, "Testing `DBCN`"),
                Pass => info!(target: TARGET, "Sbi `DBCN` test pass"),
                WriteByte => {}
                WritingByteFailed(ret) => {
                    error!(target: TARGET, "writing byte failed: {ret:?}");
                    result = false;
                }
                WriteSlice => info!(target: TARGET, "writing slice successfully"),
                WritingPartialSlice(len) => {
                    warn!(target: TARGET, "writing partial slice: {len} bytes written");
                }
                WritingSliceFailed(ret) => {
                    error!(target: TARGET, "writing slice failed: {ret:?}");
                    result = false;
                }
                Read(len) => info!(target: TARGET, "reading {len} bytes from console"),
                ReadingFailed(ret) => {
                    error!(target: TARGET, "reading failed: {ret:?}");
                    result = false;
                }
            }
        });
        result
    }
}
