use alloc::string::String;
use alloc::{boxed::Box, string::ToString};
use clint::{SifiveClintWrap, THeadClintWrap};
use core::{
    ops::Range,
    sync::atomic::{AtomicBool, Ordering},
};
use reset::SifiveTestDeviceWrap;
use spin::Mutex;
use uart_xilinx::MmioUartAxiLite;

use crate::cfg::NUM_HART_MAX;
use crate::devicetree::*;
use crate::fail;
use crate::platform::clint::{MachineClintType, SIFIVE_CLINT_COMPATIBLE, THEAD_CLINT_COMPATIBLE};
use crate::platform::console::Uart16550Wrap;
use crate::platform::console::UartBflbWrap;
use crate::platform::console::UartPl011Wrap;
use crate::platform::console::UartSifiveWrap;
use crate::platform::console::{
    MachineConsoleType, UART16650U8_COMPATIBLE, UART16650U32_COMPATIBLE, UARTAXILITE_COMPATIBLE,
    UARTBFLB_COMPATIBLE, UARTPL011_COMPATIBLE, UARTSIFIVE_COMPATIBLE,
};
use crate::platform::reset::SIFIVETEST_COMPATIBLE;
use crate::sbi::SBI;
use crate::sbi::console::SbiConsole;
use crate::sbi::features::extension_detection;
use crate::sbi::hsm::SbiHsm;
use crate::sbi::ipi::SbiIpi;
use crate::sbi::logger;
use crate::sbi::pmu::{EventToCounterMap, RawEventToCounterMap};
use crate::sbi::reset::SbiReset;
use crate::sbi::rfence::SbiRFence;
use crate::sbi::suspend::SbiSuspend;

mod clint;
mod console;
mod reset;
pub static mut CPU_PRIVILEGED_ENABLED: [bool; NUM_HART_MAX] = [false; NUM_HART_MAX];

type BaseAddress = usize;

type CpuEnableList = [bool; NUM_HART_MAX];

pub struct BoardInfo {
    pub memory_range: Option<Range<usize>>,
    pub console: Option<(BaseAddress, MachineConsoleType)>,
    pub reset: Option<BaseAddress>,
    pub ipi: Option<(BaseAddress, MachineClintType)>,
    pub cpu_num: Option<usize>,
    pub cpu_enabled: Option<CpuEnableList>,
    pub model: String,
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
            model: String::new(),
        }
    }
}

pub struct Platform {
    pub info: BoardInfo,
    pub sbi: SBI,
    pub ready: AtomicBool,
}

impl Platform {
    pub const fn new() -> Self {
        Platform {
            info: BoardInfo::new(),
            sbi: SBI::new(),
            ready: AtomicBool::new(false),
        }
    }

    pub fn init(&mut self, fdt_address: usize) {
        let dtb = parse_device_tree(fdt_address).unwrap_or_else(fail::device_tree_format);
        let dtb = dtb.share();

        let root: serde_device_tree::buildin::Node = serde_device_tree::from_raw_mut(&dtb)
            .unwrap_or_else(fail::device_tree_deserialize_root);
        let tree: Tree = root.deserialize();

        // Get console device, init sbi console and logger.
        self.sbi_find_and_init_console(&root);
        // Get clint and reset device, init sbi ipi, reset, hsm, rfence and susp extension.
        self.sbi_init_ipi_reset_hsm_rfence(&root);
        // Initialize pmu extension
        self.sbi_init_pmu(&root);
        // Get other info
        self.sbi_misc_init(&tree);

        self.ready.swap(true, Ordering::Release);
    }

    fn sbi_find_and_init_console(&mut self, root: &serde_device_tree::buildin::Node) {
        //  Get console device info
        if let Some(stdout_path) = root.chosen_stdout_path() {
            if let Some(node) = root.find(stdout_path) {
                let info = get_compatible_and_range(&node);
                if let Some((compatible, regs)) = info {
                    for device_id in compatible.iter() {
                        if UART16650U8_COMPATIBLE.contains(&device_id) {
                            self.info.console = Some((regs.start, MachineConsoleType::Uart16550U8));
                        }
                        if UART16650U32_COMPATIBLE.contains(&device_id) {
                            self.info.console =
                                Some((regs.start, MachineConsoleType::Uart16550U32));
                        }
                        if UARTAXILITE_COMPATIBLE.contains(&device_id) {
                            self.info.console = Some((regs.start, MachineConsoleType::UartAxiLite));
                        }
                        if UARTBFLB_COMPATIBLE.contains(&device_id) {
                            self.info.console = Some((regs.start, MachineConsoleType::UartBflb));
                        }
                        if UARTSIFIVE_COMPATIBLE.contains(&device_id) {
                            self.info.console = Some((regs.start, MachineConsoleType::UartSifive));
                        }
                        if UARTPL011_COMPATIBLE.contains(&device_id) {
                            self.info.console = Some((regs.start, MachineConsoleType::UartPl011));
                        }
                    }
                }
            }
        }

        // init console and logger
        self.sbi_console_init();
        logger::Logger::init().unwrap();
        info!("Hello RustSBI!");
    }

    fn sbi_init_ipi_reset_hsm_rfence(&mut self, root: &serde_device_tree::buildin::Node) {
        // Get ipi and reset device info
        let mut find_device = |node: &serde_device_tree::buildin::Node| {
            let info = get_compatible_and_range(node);
            if let Some(info) = info {
                let (compatible, regs) = info;
                let base_address = regs.start;
                for device_id in compatible.iter() {
                    // Initialize clint device.
                    if SIFIVE_CLINT_COMPATIBLE.contains(&device_id) {
                        if node.get_prop("clint,has-no-64bit-mmio").is_some() {
                            self.info.ipi = Some((base_address, MachineClintType::TheadClint));
                        } else {
                            self.info.ipi = Some((base_address, MachineClintType::SiFiveClint));
                        }
                    } else if THEAD_CLINT_COMPATIBLE.contains(&device_id) {
                        self.info.ipi = Some((base_address, MachineClintType::TheadClint));
                    }
                    // Initialize reset device.
                    if SIFIVETEST_COMPATIBLE.contains(&device_id) {
                        self.info.reset = Some(base_address);
                    }
                }
            }
        };
        root.search(&mut find_device);
        self.sbi_ipi_init();
        self.sbi_hsm_init();
        self.sbi_reset_init();
        self.sbi_rfence_init();
        self.sbi_susp_init();
    }

    fn sbi_init_pmu(&mut self, root: &serde_device_tree::buildin::Node) {
        let mut pmu_node: Option<Pmu> = None;
        let mut find_pmu = |node: &serde_device_tree::buildin::Node| {
            let info = get_compatible(node);
            if let Some(compatible_strseq) = info {
                let compatible_iter = compatible_strseq.iter();
                for compatible in compatible_iter {
                    if compatible == "riscv,pmu" {
                        pmu_node = Some(node.deserialize::<Pmu>());
                    }
                }
            }
        };
        root.search(&mut find_pmu);

        if let Some(ref pmu) = pmu_node {
            let sbi_pmu = self.sbi.pmu.get_or_insert_default();
            if let Some(ref event_to_mhpmevent) = pmu.event_to_mhpmevent {
                let len = event_to_mhpmevent.len();
                for idx in 0..len {
                    let event = event_to_mhpmevent.get_event_id(idx);
                    let mhpmevent = event_to_mhpmevent.get_selector_value(idx);
                    sbi_pmu.insert_event_to_mhpmevent(event, mhpmevent);
                    debug!(
                        "pmu: insert event: 0x{:08x}, mhpmevent: {:#016x}",
                        event, mhpmevent
                    );
                }
            }

            if let Some(ref event_to_mhpmcounters) = pmu.event_to_mhpmcounters {
                let len = event_to_mhpmcounters.len();
                for idx in 0..len {
                    let events = event_to_mhpmcounters.get_event_idx_range(idx);
                    let mhpmcounters = event_to_mhpmcounters.get_counter_bitmap(idx);
                    let event_to_counter =
                        EventToCounterMap::new(mhpmcounters, *events.start(), *events.end());
                    debug!("pmu: insert event_to_mhpmcounter: {:x?}", event_to_counter);
                    sbi_pmu.insert_event_to_mhpmcounter(event_to_counter);
                }
            }

            if let Some(ref raw_event_to_mhpmcounters) = pmu.raw_event_to_mhpmcounters {
                let len = raw_event_to_mhpmcounters.len();
                for idx in 0..len {
                    let raw_event_select = raw_event_to_mhpmcounters.get_event_idx_base(idx);
                    let select_mask = raw_event_to_mhpmcounters.get_event_idx_mask(idx);
                    let counters_mask = raw_event_to_mhpmcounters.get_counter_bitmap(idx);
                    let raw_event_to_counter =
                        RawEventToCounterMap::new(counters_mask, raw_event_select, select_mask);
                    debug!(
                        "pmu: insert raw_event_to_mhpmcounter: {:x?}",
                        raw_event_to_counter
                    );
                    sbi_pmu.insert_raw_event_to_mhpmcounter(raw_event_to_counter);
                }
            }
        }
    }

    fn sbi_misc_init(&mut self, tree: &Tree) {
        // Get memory info
        // TODO: More than one memory node or range?
        let memory_reg = tree
            .memory
            .iter()
            .next()
            .unwrap()
            .deserialize::<Memory>()
            .reg;
        let memory_range = memory_reg.iter().next().unwrap().0;
        self.info.memory_range = Some(memory_range);

        // Get cpu number info
        self.info.cpu_num = Some(tree.cpus.cpu.len());

        // Get model info
        if let Some(ref model) = tree.model {
            let model = model.iter().next().unwrap_or("<unspecified>");
            self.info.model = model.to_string();
        } else {
            let model = "<unspecified>";
            self.info.model = model.to_string();
        }

        // TODO: Need a better extension initialization method
        extension_detection(&tree.cpus.cpu);

        // Find which hart is enabled by fdt
        let mut cpu_list: CpuEnableList = [false; NUM_HART_MAX];
        for cpu_iter in tree.cpus.cpu.iter() {
            let cpu = cpu_iter.deserialize::<Cpu>();
            let hart_id = cpu.reg.iter().next().unwrap().0.start;
            if let Some(x) = cpu_list.get_mut(hart_id) {
                unsafe {
                    *x = CPU_PRIVILEGED_ENABLED[hart_id];
                }
            } else {
                error!(
                    "The maximum supported hart id is {}, but the hart id {} was obtained. Please check the config!",
                    NUM_HART_MAX - 1,
                    hart_id
                );
            }
        }
        self.info.cpu_enabled = Some(cpu_list);
    }

    fn sbi_console_init(&mut self) {
        if let Some((base, console_type)) = self.info.console {
            self.sbi.console = match console_type {
                MachineConsoleType::Uart16550U8 => Some(SbiConsole::new(Mutex::new(Box::new(
                    Uart16550Wrap::<u8>::new(base),
                )))),
                MachineConsoleType::Uart16550U32 => Some(SbiConsole::new(Mutex::new(Box::new(
                    Uart16550Wrap::<u32>::new(base),
                )))),
                MachineConsoleType::UartAxiLite => Some(SbiConsole::new(Mutex::new(Box::new(
                    MmioUartAxiLite::new(base),
                )))),
                MachineConsoleType::UartBflb => Some(SbiConsole::new(Mutex::new(Box::new(
                    UartBflbWrap::new(base),
                )))),
                MachineConsoleType::UartSifive => Some(SbiConsole::new(Mutex::new(Box::new(
                    UartSifiveWrap::new(base),
                )))),
                MachineConsoleType::UartPl011 => Some(SbiConsole::new(Mutex::new(Box::new(
                    UartPl011Wrap::new(base),
                )))),
            };
        } else {
            self.sbi.console = None;
        }
    }

    fn sbi_reset_init(&mut self) {
        if let Some(base) = self.info.reset {
            self.sbi.reset = Some(SbiReset::new(Mutex::new(Box::new(
                SifiveTestDeviceWrap::new(base),
            ))));
        } else {
            self.sbi.reset = None;
        }
    }

    fn sbi_ipi_init(&mut self) {
        if let Some((base, clint_type)) = self.info.ipi {
            self.sbi.ipi = match clint_type {
                MachineClintType::SiFiveClint => Some(SbiIpi::new(
                    Mutex::new(Box::new(SifiveClintWrap::new(base))),
                    self.info.cpu_num.unwrap_or(NUM_HART_MAX),
                )),
                MachineClintType::TheadClint => Some(SbiIpi::new(
                    Mutex::new(Box::new(THeadClintWrap::new(base))),
                    self.info.cpu_num.unwrap_or(NUM_HART_MAX),
                )),
            };
        } else {
            self.sbi.ipi = None;
        }
    }

    fn sbi_hsm_init(&mut self) {
        // TODO: Can HSM work properly when there is no ipi device?
        if self.info.ipi.is_some() {
            self.sbi.hsm = Some(SbiHsm);
        } else {
            self.sbi.hsm = None;
        }
    }

    fn sbi_rfence_init(&mut self) {
        // TODO: Can rfence work properly when there is no ipi device?
        if self.info.ipi.is_some() {
            self.sbi.rfence = Some(SbiRFence);
        } else {
            self.sbi.rfence = None;
        }
    }

    fn sbi_susp_init(&mut self) {
        if self.sbi.hsm.is_some() {
            self.sbi.susp = Some(SbiSuspend);
        } else {
            self.sbi.susp = None;
        }
    }

    pub fn print_board_info(&self) {
        info!("RustSBI version {}", rustsbi::VERSION);
        rustsbi::LOGO.lines().for_each(|line| info!("{}", line));
        info!("Initializing RustSBI machine-mode environment.");

        self.print_platform_info();
        self.print_cpu_info();
        self.print_device_info();
        self.print_memory_info();
        self.print_additional_info();
    }

    #[inline]
    fn print_platform_info(&self) {
        info!("{:<30}: {}", "Platform Name", self.info.model);
    }

    fn print_cpu_info(&self) {
        info!(
            "{:<30}: {:?}",
            "Platform HART Count",
            self.info.cpu_num.unwrap_or(0)
        );

        if let Some(cpu_enabled) = &self.info.cpu_enabled {
            let mut enabled_harts = [0; NUM_HART_MAX];
            let mut count = 0;
            for (i, &enabled) in cpu_enabled.iter().enumerate() {
                if enabled {
                    enabled_harts[count] = i;
                    count += 1;
                }
            }
            info!("{:<30}: {:?}", "Enabled HARTs", &enabled_harts[..count]);
        } else {
            warn!("{:<30}: Not Available", "Enabled HARTs");
        }
    }

    #[inline]
    fn print_device_info(&self) {
        self.print_clint_info();
        self.print_console_info();
        self.print_reset_info();
        self.print_hsm_info();
        self.print_rfence_info();
        self.print_susp_info();
        self.print_pmu_info();
    }

    #[inline]
    fn print_clint_info(&self) {
        match self.info.ipi {
            Some((base, device)) => {
                info!(
                    "{:<30}: {:?} (Base Address: 0x{:x})",
                    "Platform IPI Extension", device, base
                );
            }
            None => warn!("{:<30}: Not Available", "Platform IPI Device"),
        }
    }

    #[inline]
    fn print_console_info(&self) {
        match self.info.console {
            Some((base, device)) => {
                info!(
                    "{:<30}: {:?} (Base Address: 0x{:x})",
                    "Platform Console Extension", device, base
                );
            }
            None => warn!("{:<30}: Not Available", "Platform Console Device"),
        }
    }

    #[inline]
    fn print_reset_info(&self) {
        if let Some(base) = self.info.reset {
            info!(
                "{:<30}: Available (Base Address: 0x{:x})",
                "Platform Reset Extension", base
            );
        } else {
            warn!("{:<30}: Not Available", "Platform Reset Device");
        }
    }

    #[inline]
    fn print_hsm_info(&self) {
        if self.have_hsm() {
            info!("{:<30}: {}", "Platform HSM Extension", "Available");
        } else {
            warn!("{:<30}: {}", "Platform HSM Extension", "Not Available");
        }
    }

    #[inline]
    fn print_rfence_info(&self) {
        if self.have_rfence() {
            info!("{:<30}: {}", "Platform RFence Extension", "Available");
        } else {
            warn!("{:<30}: {}", "Platform RFence Extension", "Not Available");
        }
    }

    #[inline]
    fn print_susp_info(&self) {
        if self.have_susp() {
            info!("{:<30}: {}", "Platform SUSP Extension", "Available");
        } else {
            warn!("{:<30}: {}", "Platform SUSP Extension", "Not Available");
        }
    }

    #[inline]
    fn print_pmu_info(&self) {
        if self.have_pmu() {
            info!("{:<30}: {}", "Platform PMU Extension", "Available");
        } else {
            warn!("{:<30}: {}", "Platform PMU Extension", "Not Available");
        }
    }

    #[inline]
    fn print_memory_info(&self) {
        if let Some(memory_range) = &self.info.memory_range {
            info!(
                "{:<30}: 0x{:x} - 0x{:x}",
                "Memory range", memory_range.start, memory_range.end
            );
        } else {
            warn!("{:<30}: Not Available", "Memory range");
        }
    }

    #[inline]
    fn print_additional_info(&self) {
        if !self.ready.load(Ordering::Acquire) {
            warn!(
                "{:<30}: Platform initialization is not complete.",
                "Platform Status"
            );
        } else {
            info!(
                "{:<30}: Platform initialization complete and ready.",
                "Platform Status"
            );
        }
    }
}

#[allow(unused)]
impl Platform {
    pub fn have_console(&self) -> bool {
        self.sbi.console.is_some()
    }

    pub fn have_reset(&self) -> bool {
        self.sbi.reset.is_some()
    }

    pub fn have_ipi(&self) -> bool {
        self.sbi.ipi.is_some()
    }

    pub fn have_hsm(&self) -> bool {
        self.sbi.hsm.is_some()
    }

    pub fn have_rfence(&self) -> bool {
        self.sbi.rfence.is_some()
    }

    pub fn have_susp(&self) -> bool {
        self.sbi.susp.is_some()
    }

    pub fn have_pmu(&self) -> bool {
        self.sbi.pmu.is_some()
    }

    pub fn ready(&self) -> bool {
        self.ready.load(Ordering::Acquire)
    }
}

pub(crate) static mut PLATFORM: Platform = Platform::new();
