mod facade {
    pub use rustsbi::*;
}

use facade::{HartMask, RustSBI, SbiRet};

#[derive(facade::RustSBI)]
#[rustsbi(crate = crate::facade)]
struct Static {
    ipi: EchoIpi,
    info: Info,
}

#[derive(facade::RustSBI)]
#[rustsbi(dynamic, crate = crate::facade)]
struct Dynamic {
    ipi: Option<EchoIpi>,
    info: Info,
}

struct EchoIpi;

impl facade::Ipi for EchoIpi {
    fn send_ipi(&self, mask: HartMask) -> SbiRet {
        let (mask, base) = mask.into_inner();
        SbiRet::success(mask + base)
    }
}

struct Info;

impl facade::EnvInfo for Info {
    fn mvendorid(&self) -> usize {
        1
    }

    fn marchid(&self) -> usize {
        2
    }

    fn mimpid(&self) -> usize {
        3
    }
}

fn check(sbi: &impl RustSBI, available: bool) {
    let eid = facade::spec::spi::EID_SPI;
    assert_eq!(
        sbi.handle_ecall(eid, 0, [1, 4, 0, 0, 0, 0]),
        if available {
            SbiRet::success(5)
        } else {
            SbiRet::not_supported()
        }
    );
    let probe = sbi.handle_ecall(0x10, 3, [eid, 0, 0, 0, 0, 0]);
    assert!(probe.is_ok());
    assert_eq!(probe.value != 0, available);
    assert_eq!(sbi.handle_ecall(0x10, 6, [0; 6]), SbiRet::success(3));
}

#[test]
fn static_dispatch_through_reexport() {
    check(
        &Static {
            ipi: EchoIpi,
            info: Info,
        },
        true,
    );
}

#[test]
fn dynamic_dispatch_through_reexport() {
    check(
        &Dynamic {
            ipi: Some(EchoIpi),
            info: Info,
        },
        true,
    );
    check(
        &Dynamic {
            ipi: None,
            info: Info,
        },
        false,
    );
}
