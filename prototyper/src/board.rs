use aclint::SifiveClint;
use core::{
    cell::RefCell,
    fmt::{Display, Formatter, Result},
    ops::Range,
    sync::atomic::{AtomicBool, AtomicPtr, Ordering},
};
use serde_device_tree::Dtb;
use sifive_test_device::SifiveTestDevice;
use spin::Mutex;
use uart16550::Uart16550;
use uart_xilinx::uart_lite::uart::MmioUartAxiLite;

use crate::fail;
use crate::sbi::console::{ConsoleDevice, SbiConsole};
use crate::sbi::extensions;
use crate::sbi::hsm::SbiHsm;
use crate::sbi::ipi::{IpiDevice, SbiIpi};
use crate::sbi::logger;
use crate::sbi::reset::{ResetDevice, SbiReset};
use crate::sbi::trap_stack;
use crate::sbi::trap_stack::NUM_HART_MAX;
use crate::sbi::SBI;
use crate::{dt, sbi::rfence::SbiRFence};

pub(crate) const UART16650_COMPATIBLE: &str = "ns16550a";
pub(crate) const UARTAXILITE_COMPATIBLE: &str = "xlnx,xps-uartlite-1.00.a";
pub(crate) const SIFIVETEST_COMPATIBLE: &str = "sifive,test0";
pub(crate) const SIFIVECLINT_COMPATIBLE: &str = "riscv,clint0";

type BaseAddress = usize;
/// Store finite-length string on the stack.
pub(crate) struct StringInline<const N: usize>(usize, [u8; N]);

impl<const N: usize> Display for StringInline<N> {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        write!(f, "{}", unsafe {
            core::str::from_utf8_unchecked(&self.1[..self.0])
        })
    }
}

type CpuEnableList = [bool; trap_stack::NUM_HART_MAX];

pub struct BoardInfo {
    pub memory_range: Option<Range<usize>>,
    pub console: Option<(BaseAddress, MachineConsoleType)>,
    pub reset: Option<BaseAddress>,
    pub ipi: Option<BaseAddress>,
    pub cpu_num: Option<usize>,
    pub cpu_enabled: Option<CpuEnableList>,
    pub model: StringInline<128>,
}

impl BoardInfo {
    pub const fn new() -> Self {
        BoardInfo {
            memory_range: None,
            console: None,
            reset: None,
            ipi: None,
            cpu_enabled: None,
            cpu_num: None,
            model: StringInline(0, [0u8; 128]),
        }
    }
}

pub struct Board {
    pub info: BoardInfo,
    pub sbi: SBI<MachineConsole, SifiveClint, SifiveTestDevice>,
    pub ready: AtomicBool,
}

#[allow(unused)]
impl Board {
    pub const fn new() -> Self {
        Board {
            info: BoardInfo::new(),
            sbi: SBI::new(),
            ready: AtomicBool::new(false),
        }
    }

    pub fn init(&mut self, dtb: &RefCell<Dtb>) {
        self.info_init(dtb);
        self.sbi_init();
        logger::Logger::init().unwrap();
        trap_stack::prepare_for_trap();
        self.ready.swap(true, Ordering::Release);
    }

    pub fn have_console(&self) -> bool {
        match self.sbi.console {
            None => false,
            Some(_) => true,
        }
    }

    pub fn have_reset(&self) -> bool {
        match self.sbi.reset {
            None => false,
            Some(_) => true,
        }
    }

    pub fn have_ipi(&self) -> bool {
        match self.sbi.ipi {
            None => false,
            Some(_) => true,
        }
    }

    pub fn have_hsm(&self) -> bool {
        match self.sbi.hsm {
            None => false,
            Some(_) => true,
        }
    }

    pub fn have_rfence(&self) -> bool {
        match self.sbi.rfence {
            None => false,
            Some(_) => true,
        }
    }

    pub fn ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }

    pub fn print_board_info(&self) {
        info!("RustSBI version {}", rustsbi::VERSION);
        rustsbi::LOGO.lines().for_each(|line| info!("{}", line));
        info!("Initializing RustSBI machine-mode environment.");
        info!("Number of CPU: {:?}", self.info.cpu_num);
        info!("Enabled hart: {:?}", self.info.cpu_enabled);
        info!("Model: {}", self.info.model);
        info!("Clint device: {:x?}", self.info.ipi);
        info!("Console device: {:x?}", self.info.console);
    }

    fn info_init(&mut self, dtb: &RefCell<Dtb>) {
        // TODO: should remove `fail:device_tree_deserialize`.
        let root: serde_device_tree::buildin::Node = serde_device_tree::from_raw_mut(&dtb)
            .unwrap_or_else(fail::device_tree_deserialize_root);
        let tree: dt::Tree = root.deserialize();

        //  Get console device info
        for console_path in tree.chosen.stdout_path.iter() {
            if let Some(node) = root.find(console_path) {
                let info = dt::get_compatible_and_range(&node);
                let result = info.is_some_and(|info| {
                    let (compatible, regs) = info;
                    for device_id in compatible.iter() {
                        if device_id == UART16650_COMPATIBLE {
                            self.info.console = Some((regs.start, MachineConsoleType::Uart16550));
                            return true;
                        }
                        if device_id == UARTAXILITE_COMPATIBLE {
                            self.info.console = Some((regs.start, MachineConsoleType::UartAxiLite));
                            return true;
                        }
                    }
                    false
                });
                if result {
                    break;
                }
            }
        }

        // Get ipi and reset device info
        let mut find_device = |node: &serde_device_tree::buildin::Node| {
            let info = dt::get_compatible_and_range(node);
            if let Some(info) = info {
                let (compatible, regs) = info;
                let base_address = regs.start;
                for device_id in compatible.iter() {
                    // Initialize clint device.
                    if device_id == SIFIVECLINT_COMPATIBLE {
                        self.info.ipi = Some(base_address);
                    }
                    // Initialize reset device.
                    if device_id == SIFIVETEST_COMPATIBLE {
                        self.info.reset = Some(base_address);
                    }
                }
            }
        };
        root.search(&mut find_device);

        // Get memory info
        // TODO: More than one memory node or range?
        let memory_reg = tree
            .memory
            .iter()
            .next()
            .unwrap()
            .deserialize::<dt::Memory>()
            .reg;
        let memory_range = memory_reg.iter().next().unwrap().0;
        self.info.memory_range = Some(memory_range);

        // Get cpu number info
        self.info.cpu_num = Some(tree.cpus.cpu.len());

        // Get model info
        if let Some(model) = tree.model {
            let model = model.iter().next().unwrap_or("<unspecified>");
            self.info.model.0 = model.as_bytes().len();
            self.info.model.1[..self.info.model.0].copy_from_slice(model.as_bytes());
        } else {
            let model = "<unspecified>";
            self.info.model.0 = model.as_bytes().len();
            self.info.model.1[..self.info.model.0].copy_from_slice(model.as_bytes());
        }

        // TODO: Need a better extension initialization method
        extensions::init(&tree.cpus.cpu);

        // Find which hart is enabled by fdt
        let mut cpu_list: CpuEnableList = [false; trap_stack::NUM_HART_MAX];
        for cpu_iter in tree.cpus.cpu.iter() {
            use dt::Cpu;
            let cpu = cpu_iter.deserialize::<Cpu>();
            let hart_id = cpu.reg.iter().next().unwrap().0.start;
            cpu_list.get_mut(hart_id).map(|x| *x = true);
        }
        self.info.cpu_enabled = Some(cpu_list);
    }

    fn sbi_init(&mut self) {
        self.sbi_console_init();
        self.sbi_ipi_init();
        self.sbi_hsm_init();
        self.sbi_reset_init();
        self.sbi_rfence_init();
    }

    fn sbi_console_init(&mut self) {
        if let Some((base, console_type)) = self.info.console {
            let new_console = match console_type {
                MachineConsoleType::Uart16550 => MachineConsole::Uart16550(base as _),
                MachineConsoleType::UartAxiLite => {
                    MachineConsole::UartAxiLite(MmioUartAxiLite::new(base))
                }
            };
            self.sbi.console = Some(SbiConsole::new(Mutex::new(new_console)));
        } else {
            self.sbi.console = None;
        }
    }

    fn sbi_reset_init(&mut self) {
        if let Some(base) = self.info.reset {
            self.sbi.reset = Some(SbiReset::new(AtomicPtr::new(base as _)));
        } else {
            self.sbi.reset = None;
        }
    }

    fn sbi_ipi_init(&mut self) {
        if let Some(base) = self.info.ipi {
            self.sbi.ipi = Some(SbiIpi::new(
                AtomicPtr::new(base as _),
                self.info.cpu_num.unwrap_or(NUM_HART_MAX),
            ));
        } else {
            self.sbi.ipi = None;
        }
    }

    fn sbi_hsm_init(&mut self) {
        // TODO: Can HSM work properly when there is no ipi device?
        if let Some(_) = self.info.ipi {
            self.sbi.hsm = Some(SbiHsm);
        } else {
            self.sbi.hsm = None;
        }
    }

    fn sbi_rfence_init(&mut self) {
        // TODO: Can rfence work properly when there is no ipi device?
        if let Some(_) = self.info.ipi {
            self.sbi.rfence = Some(SbiRFence);
        } else {
            self.sbi.rfence = None;
        }
    }
}

pub(crate) static mut BOARD: Board = Board::new();

/// Console Device: Uart16550
#[doc(hidden)]
#[allow(unused)]
#[derive(Clone, Copy, Debug)]
pub enum MachineConsoleType {
    Uart16550,
    UartAxiLite,
}
#[doc(hidden)]
#[allow(unused)]
pub enum MachineConsole {
    Uart16550(*const Uart16550<u8>),
    UartAxiLite(MmioUartAxiLite),
}

unsafe impl Send for MachineConsole {}
unsafe impl Sync for MachineConsole {}

impl ConsoleDevice for MachineConsole {
    fn read(&self, buf: &mut [u8]) -> usize {
        match self {
            Self::Uart16550(uart16550) => unsafe { (**uart16550).read(buf) },
            Self::UartAxiLite(axilite) => axilite.read(buf),
        }
    }

    fn write(&self, buf: &[u8]) -> usize {
        match self {
            MachineConsole::Uart16550(uart16550) => unsafe { (**uart16550).write(buf) },
            Self::UartAxiLite(axilite) => axilite.write(buf),
        }
    }
}

/// Ipi Device: Sifive Clint
impl IpiDevice for SifiveClint {
    #[inline(always)]
    fn read_mtime(&self) -> u64 {
        self.read_mtime()
    }

    #[inline(always)]
    fn write_mtime(&self, val: u64) {
        self.write_mtime(val)
    }

    #[inline(always)]
    fn read_mtimecmp(&self, hart_idx: usize) -> u64 {
        self.read_mtimecmp(hart_idx)
    }

    #[inline(always)]
    fn write_mtimecmp(&self, hart_idx: usize, val: u64) {
        self.write_mtimecmp(hart_idx, val)
    }

    #[inline(always)]
    fn read_msip(&self, hart_idx: usize) -> bool {
        self.read_msip(hart_idx)
    }

    #[inline(always)]
    fn set_msip(&self, hart_idx: usize) {
        self.set_msip(hart_idx)
    }

    #[inline(always)]
    fn clear_msip(&self, hart_idx: usize) {
        self.clear_msip(hart_idx)
    }
}

/// Reset Device: SifiveTestDevice
impl ResetDevice for SifiveTestDevice {
    #[inline]
    fn fail(&self, code: u16) -> ! {
        self.fail(code)
    }

    #[inline]
    fn pass(&self) -> ! {
        self.pass()
    }

    #[inline]
    fn reset(&self) -> ! {
        self.reset()
    }
}
