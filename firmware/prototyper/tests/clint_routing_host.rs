//! Compile the production CLINT routing module against mock MMIO endpoints.
extern crate alloc;
extern crate self as runtime;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Error {
    InvalidArgs,
    IoError,
}
pub type Result<T> = std::result::Result<T, Error>;

pub mod hart {
    use super::{Error, Result};
    #[derive(Copy, Clone)]
    pub struct HartId(usize, usize);
    impl HartId {
        pub fn from_raw(raw: usize) -> Result<Self> {
            match raw {
                0 => Ok(Self(0, 0)),
                2 => Ok(Self(2, 1)),
                5 => Ok(Self(5, 2)),
                _ => Err(Error::InvalidArgs),
            }
        }
        pub fn index(self) -> usize {
            self.1
        }
        pub fn as_usize(self) -> usize {
            self.0
        }
        pub fn current() -> Result<Self> {
            Self::from_raw(5)
        }
    }
}
pub mod ipi {
    use super::Result;
    #[derive(Clone)]
    pub struct IpiRequest {
        targets: Vec<usize>,
    }
    impl IpiRequest {
        pub fn for_test(targets: Vec<usize>) -> Self {
            Self { targets }
        }
        pub fn harts(self) -> impl Iterator<Item = usize> {
            self.targets.into_iter()
        }
    }
    pub trait IpiDevice: Send + Sync {
        fn send(&self, req: IpiRequest) -> Result<()>;
        fn acknowledge(&self) -> Result<bool>;
    }
}
pub mod timer {
    use super::hart::HartId;
    pub trait TimerDevice: Send + Sync {
        fn set_compare(&self, hart: HartId, deadline: u64);
        fn read_time(&self) -> Option<u64> {
            None
        }
    }
}

pub trait LocalTimer: Send + Sync {
    fn set_compare_slot(&self, slot: usize, deadline: u64);
    fn read_time(&self) -> Option<u64> {
        None
    }
}
pub trait LocalIpi: Send + Sync {
    fn send_slot(&self, slot: usize) -> Result<()>;
    fn clear_slot(&self, slot: usize) -> Result<()>;
}
#[path = "../src/driver/clint/routing.rs"]
mod routing;
use hart::HartId;
use ipi::{IpiDevice, IpiRequest};
use routing::{RoutedIpi, RoutedTimer};
use timer::TimerDevice;

struct TimerMock {
    device: usize,
    log: Arc<Mutex<Vec<(usize, usize, u64)>>>,
}
impl LocalTimer for TimerMock {
    fn set_compare_slot(&self, slot: usize, v: u64) {
        self.log.lock().unwrap().push((self.device, slot, v))
    }
    fn read_time(&self) -> Option<u64> {
        Some(100 + self.device as u64)
    }
}
struct IpiMock {
    device: usize,
    log: Arc<Mutex<Vec<(usize, usize, bool)>>>,
}
impl LocalIpi for IpiMock {
    fn send_slot(&self, slot: usize) -> Result<()> {
        self.log.lock().unwrap().push((self.device, slot, true));
        Ok(())
    }
    fn clear_slot(&self, slot: usize) -> Result<()> {
        self.log.lock().unwrap().push((self.device, slot, false));
        Ok(())
    }
}
fn routes() -> Vec<(usize, usize)> {
    vec![(0, 0), (1, 1), (0, 2)]
}
fn timers(log: Arc<Mutex<Vec<(usize, usize, u64)>>>) -> RoutedTimer {
    RoutedTimer::new(
        (0..2)
            .map(|d| {
                Box::new(TimerMock {
                    device: d,
                    log: log.clone(),
                }) as Box<dyn LocalTimer>
            })
            .collect(),
        routes(),
    )
}
fn ipis(log: Arc<Mutex<Vec<(usize, usize, bool)>>>) -> RoutedIpi {
    RoutedIpi::new(
        (0..2)
            .map(|d| {
                Box::new(IpiMock {
                    device: d,
                    log: log.clone(),
                }) as Box<dyn LocalIpi>
            })
            .collect(),
        routes(),
    )
}
#[test]
fn routes_nonidentity_hart_ids_to_device_local_timer_slots() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let timer = timers(log.clone());
    for (hart, deadline) in [(0, 11), (2, 22), (5, 55)] {
        timer.set_compare(HartId::from_raw(hart).unwrap(), deadline);
    }
    assert_eq!(
        *log.lock().unwrap(),
        vec![(0, 0, 11), (1, 1, 22), (0, 2, 55)]
    );
    assert_eq!(timer.read_time(), Some(100));
}
#[test]
fn ipi_send_and_current_ack_use_different_controller_slots() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let ipi = ipis(log.clone());
    assert_eq!(ipi.send(IpiRequest::for_test(vec![0, 2, 5])), Ok(()));
    assert_eq!(ipi.acknowledge(), Ok(true));
    assert_eq!(
        *log.lock().unwrap(),
        vec![(0, 0, true), (1, 1, true), (0, 2, true), (0, 2, false)]
    );
}
#[test]
fn unknown_hart_fails_without_register_write() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let ipi = ipis(log.clone());
    assert_eq!(
        ipi.send(IpiRequest::for_test(vec![9])),
        Err(Error::InvalidArgs)
    );
    assert!(log.lock().unwrap().is_empty());
}
