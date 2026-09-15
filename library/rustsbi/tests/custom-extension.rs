use core::cell::Cell;
use rustsbi::{EnvInfo, Extension, RustSBI, SbiRet, Timer, spec};

mod facade {
    pub use rustsbi::*;
}

const EID: usize = 0x0900_031e;
const OTHER_EID: usize = 0x5445_5335;

struct Handler {
    available: usize,
    probes: Cell<usize>,
    calls: Cell<usize>,
}

impl Extension for Handler {
    fn probe(&self) -> usize {
        self.probes.set(self.probes.get() + 1);
        self.available
    }

    fn handle(&self, fid: usize, args: [usize; 6]) -> SbiRet {
        self.calls.set(self.calls.get() + 1);
        match fid {
            0 => SbiRet::success(args.iter().sum()),
            1 => SbiRet::invalid_param(),
            _ => SbiRet::not_supported(),
        }
    }
}

struct Info;

impl EnvInfo for Info {
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

struct Clock(Cell<u64>);

impl Timer for Clock {
    fn set_timer(&self, value: u64) {
        self.0.set(value);
    }
}

#[derive(facade::RustSBI)]
#[rustsbi(crate = crate::facade)]
struct Static<'a, T: Extension + ?Sized> {
    // Declaration order must not put this handler on the standard call path.
    #[rustsbi(extension(eid = EID))]
    custom: Option<&'a T>,
    timer: &'a Clock,
    #[rustsbi(extension(eid = 0x5445_5335))]
    other: &'a Handler,
    info: Info,
}

#[derive(facade::RustSBI)]
#[rustsbi(dynamic, crate = crate::facade)]
struct Dynamic<'a, T: Extension + ?Sized>(
    #[rustsbi(extension(eid = EID))] Option<&'a T>,
    #[rustsbi(timer)] &'a Clock,
    #[rustsbi(extension(eid = OTHER_EID))] &'a Handler,
    #[rustsbi(info)] Info,
);

fn probe(sbi: &impl RustSBI, eid: usize) -> SbiRet {
    sbi.handle_ecall(
        spec::base::EID_BASE,
        spec::base::PROBE_EXTENSION,
        [eid, 0, 0, 0, 0, 0],
    )
}

fn check(sbi: &impl RustSBI, custom: &Handler, other: &Handler, clock: &Clock, present: bool) {
    assert_eq!(probe(sbi, spec::time::EID_TIME), SbiRet::success(1));
    assert_eq!(probe(sbi, spec::spi::EID_SPI), SbiRet::success(0));
    assert_eq!(
        sbi.handle_ecall(spec::time::EID_TIME, 0, [123, 0, 0, 0, 0, 0]),
        SbiRet::success(0)
    );
    assert_eq!(clock.0.get(), 123);
    assert_eq!(
        sbi.handle_ecall(spec::time::EID_TIME, 99, [0; 6]),
        SbiRet::not_supported()
    );
    assert_eq!(
        sbi.handle_ecall(spec::spi::EID_SPI, 0, [0; 6]),
        SbiRet::not_supported()
    );
    assert_eq!(sbi.handle_ecall(0x10, 6, [0; 6]), SbiRet::success(3));
    assert_eq!(probe(sbi, 0xdeadbeef), SbiRet::success(0));
    assert_eq!(
        sbi.handle_ecall(0xdeadbeef, 0, [0; 6]),
        SbiRet::not_supported()
    );
    assert_eq!(custom.probes.get(), 0);
    assert_eq!(other.probes.get(), 0);

    let available = present && custom.available != 0;
    assert_eq!(
        probe(sbi, EID),
        SbiRet::success(if present { custom.available } else { 0 })
    );
    for (fid, result) in [
        (0, SbiRet::success(21)),
        (1, SbiRet::invalid_param()),
        (99, SbiRet::not_supported()),
    ] {
        assert_eq!(
            sbi.handle_ecall(EID, fid, [1, 2, 3, 4, 5, 6]),
            if available {
                result
            } else {
                SbiRet::not_supported()
            }
        );
    }
    assert_eq!(custom.calls.get(), if available { 3 } else { 0 });
    assert_eq!(other.calls.get(), 0);
    assert_eq!(other.probes.get(), 0);
    assert_eq!(probe(sbi, OTHER_EID), SbiRet::success(7));
    assert_eq!(sbi.handle_ecall(OTHER_EID, 0, [1; 6]), SbiRet::success(6));
}

fn check_modes(dynamic: bool) {
    for (present, available) in [(true, 42), (true, 0), (false, 42)] {
        let custom = Handler {
            available,
            probes: Cell::new(0),
            calls: Cell::new(0),
        };
        let other = Handler {
            available: 7,
            probes: Cell::new(0),
            calls: Cell::new(0),
        };
        let clock = Clock(Cell::new(0));
        let field: Option<&dyn Extension> = present.then_some(&custom);
        if dynamic {
            check(
                &Dynamic(field, &clock, &other, Info),
                &custom,
                &other,
                &clock,
                present,
            );
        } else {
            check(
                &Static {
                    custom: field,
                    timer: &clock,
                    other: &other,
                    info: Info,
                },
                &custom,
                &other,
                &clock,
                present,
            );
        }
    }
}

#[test]
fn static_custom_extensions() {
    check_modes(false);
}

#[test]
fn dynamic_custom_extensions() {
    check_modes(true);
}

#[test]
fn eid_paths_are_not_shadowed_by_generated_parameters() {
    #[allow(non_upper_case_globals)]
    const extension: usize = EID;
    #[allow(non_upper_case_globals)]
    const eid: usize = OTHER_EID;
    #[derive(RustSBI)]
    struct Platform {
        #[rustsbi(extension(eid = extension))]
        first: Handler,
        #[rustsbi(extension(eid = eid))]
        second: Handler,
        info: Info,
    }
    let sbi = Platform {
        first: Handler {
            available: 3,
            calls: Cell::new(0),
            probes: Cell::new(0),
        },
        second: Handler {
            available: 5,
            calls: Cell::new(0),
            probes: Cell::new(0),
        },
        info: Info,
    };
    assert_eq!(probe(&sbi, EID), SbiRet::success(3));
    assert_eq!(probe(&sbi, OTHER_EID), SbiRet::success(5));
    assert_eq!(probe(&sbi, 0xdeadbeef), SbiRet::success(0));
    assert_eq!(sbi.handle_ecall(OTHER_EID, 0, [1; 6]), SbiRet::success(6));
    assert_eq!(sbi.first.calls.get(), 0);
    assert_eq!(sbi.second.calls.get(), 1);
}
