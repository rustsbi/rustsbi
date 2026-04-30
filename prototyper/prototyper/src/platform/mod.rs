use alloc::string::String;
use alloc::{boxed::Box, string::ToString, vec::Vec};
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

pub(crate) mod aia;
mod clint;
mod console;
mod reset;

pub(crate) static CPU_PRIVILEGED_ENABLED: [AtomicBool; NUM_HART_MAX] =
    [const { AtomicBool::new(false) }; NUM_HART_MAX];

const RISCV_MACHINE_EXTERNAL_IRQ: u32 = 11;

type BaseAddress = usize;

type CpuEnableList = [bool; NUM_HART_MAX];

fn collect_cpu_intc_harts(root: &serde_device_tree::buildin::Node) -> Vec<(u32, usize)> {
    let mut cpu_intc_harts = Vec::new();
    let Some(cpus) = root.find("/cpus") else {
        return cpu_intc_harts;
    };

    for cpu_item in cpus.nodes() {
        let (node_name, _) = cpu_item.get_parsed_name();
        if node_name != "cpu" {
            continue;
        }
        let cpu = cpu_item.deserialize::<Cpu>();
        let hart_id = cpu.reg.iter().next().unwrap().0.start;
        let cpu_node = cpu_item.deserialize::<serde_device_tree::buildin::Node>();
        for child_item in cpu_node.nodes() {
            let (child_name, _) = child_item.get_parsed_name();
            if child_name != "interrupt-controller" {
                continue;
            }
            let child = child_item.deserialize::<serde_device_tree::buildin::Node>();
            if !is_cpu_intc(&child) {
                continue;
            }
            if let Some(phandle) = node_phandle(&child) {
                cpu_intc_harts.push((phandle, hart_id));
            }
        }
    }

    cpu_intc_harts
}

fn is_cpu_intc(node: &serde_device_tree::buildin::Node) -> bool {
    get_compatible(node).is_some_and(|compatible| {
        compatible
            .iter()
            .any(|device_id| device_id == "riscv,cpu-intc")
    })
}

fn node_phandle(node: &serde_device_tree::buildin::Node) -> Option<u32> {
    node.get_prop("phandle")
        .or_else(|| node.get_prop("linux,phandle"))
        .map(|prop| prop.deserialize::<u32>())
}

fn prop_u32_cells(node: &serde_device_tree::buildin::Node, name: &str) -> Option<Vec<u32>> {
    let prop = node.get_prop(name)?;
    let data = prop.deserialize::<&[u8]>();
    let mut cells = Vec::new();
    let mut chunks = data.chunks_exact(4);
    for chunk in &mut chunks {
        cells.push(u32::from_be_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }
    if chunks.remainder().is_empty() {
        Some(cells)
    } else {
        None
    }
}

fn hart_for_cpu_intc(cpu_intc_harts: &[(u32, usize)], phandle: u32) -> Option<usize> {
    cpu_intc_harts
        .iter()
        .find(|(intc_phandle, _)| *intc_phandle == phandle)
        .map(|(_, hart_id)| *hart_id)
}

fn imsic_machine_hart_files(
    node: &serde_device_tree::buildin::Node,
    cpu_intc_harts: &[(u32, usize)],
) -> Option<Vec<(usize, u32)>> {
    let cells = prop_u32_cells(node, "interrupts-extended")?;
    let mut chunks = cells.chunks_exact(2);
    let mut hart_files = Vec::new();

    for (file_index, interrupt) in chunks.by_ref().enumerate() {
        let phandle = interrupt[0];
        let interrupt_id = interrupt[1];
        if interrupt_id != RISCV_MACHINE_EXTERNAL_IRQ {
            continue;
        }
        let hart_id = hart_for_cpu_intc(cpu_intc_harts, phandle)?;
        hart_files.push((hart_id, file_index as u32));
    }

    if chunks.remainder().is_empty() {
        Some(hart_files)
    } else {
        None
    }
}

pub struct BoardInfo {
    pub memory_range: Option<Range<usize>>,
    pub console: Option<(BaseAddress, MachineConsoleType)>,
    pub reset: Option<BaseAddress>,
    pub ipi: Option<(BaseAddress, MachineClintType)>,
    pub aia: Option<aia::AiaInfo>,
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
            aia: None,
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
        // Get other info that later platform initialization depends on.
        self.sbi_misc_init(&tree);
        // Get clint and reset device, init sbi ipi, reset, hsm, rfence and susp extension.
        self.sbi_init_ipi_reset_hsm_rfence(&root);
        // Initialize pmu extension
        self.sbi_init_pmu(&root);

        self.ready.swap(true, Ordering::Release);
    }

    fn init_sbi_console_and_logger(&mut self) {
        // init console and logger
        self.sbi_console_init();
        logger::Logger::init().unwrap();
        info!("Hello RustSBI!");
    }

    fn sbi_find_and_init_console(&mut self, root: &serde_device_tree::buildin::Node) {
        //  Get console device info
        let Some(stdout_path) = root.chosen_stdout_path() else {
            self.init_sbi_console_and_logger();
            return;
        };
        let Some(node) = root.find(stdout_path) else {
            self.init_sbi_console_and_logger();
            return;
        };
        let Some((compatible, regs)) = get_compatible_and_range(&node) else {
            self.init_sbi_console_and_logger();
            return;
        };

        for device_id in compatible.iter() {
            if UART16650U8_COMPATIBLE.contains(&device_id) {
                self.info.console = Some((regs.start, MachineConsoleType::Uart16550U8));
            }
            if UART16650U32_COMPATIBLE.contains(&device_id) {
                self.info.console = Some((regs.start, MachineConsoleType::Uart16550U32));
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

        self.init_sbi_console_and_logger();
    }

    fn sbi_init_ipi_reset_hsm_rfence(&mut self, root: &serde_device_tree::buildin::Node) {
        // Get ipi and reset device info
        let cpu_intc_harts = collect_cpu_intc_harts(root);
        let mut find_device = |node: &serde_device_tree::buildin::Node| {
            let info = get_compatible_and_ranges(node);
            if let Some(info) = info {
                let (compatible, regs) = info;
                let base_address = regs[0].start;
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
                    // Discover the M-level IMSIC from its CPU interrupt wiring.
                    if aia::IMSIC_COMPATIBLE.contains(&device_id) && self.info.aia.is_none() {
                        self.sbi_discover_imsic(node, &regs, &cpu_intc_harts);
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
                *x = true;
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

    pub fn sbi_cpu_init_with_feature(&mut self) {
        if let Some(cpu_enabled) = self.info.cpu_enabled.as_mut() {
            for (hart_id, enabled) in cpu_enabled.iter_mut().enumerate() {
                if *enabled {
                    *enabled = CPU_PRIVILEGED_ENABLED[hart_id].load(Ordering::Acquire);
                }
            }
        }
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

    fn sbi_discover_imsic(
        &mut self,
        node: &serde_device_tree::buildin::Node,
        reg_ranges: &[Range<usize>],
        cpu_intc_harts: &[(u32, usize)],
    ) {
        use riscv_aia::Iid;
        use riscv_aia::peripheral::imsic::system::AddressLayout;

        let Some(first_reg_range) = reg_ranges.first() else {
            warn!("IMSIC: missing reg ranges, skipping");
            return;
        };
        for reg_range in reg_ranges {
            let reg_size = reg_range.end.saturating_sub(reg_range.start);
            if reg_range.start & 0xFFF != 0 {
                warn!(
                    "IMSIC: base 0x{:x} not 4 KiB aligned, skipping",
                    reg_range.start
                );
                return;
            }

            if reg_size == 0 || reg_size & 0xFFF != 0 {
                warn!(
                    "IMSIC: reg size 0x{:x} is not a positive 4 KiB multiple, skipping",
                    reg_size
                );
                return;
            }
        }

        let reg_range = first_reg_range;
        let base_address = reg_range.start;

        let Some(num_ids_prop) = node.get_prop("riscv,num-ids") else {
            warn!("IMSIC: missing required riscv,num-ids property, skipping");
            return;
        };
        let num_ids = num_ids_prop.deserialize::<u32>() as u16;

        if num_ids == 0 {
            warn!("IMSIC: riscv,num-ids is 0, skipping AIA");
            return;
        }

        let Some(machine_hart_files) = imsic_machine_hart_files(node, cpu_intc_harts) else {
            warn!("IMSIC: malformed interrupts-extended property, skipping AIA");
            return;
        };

        if machine_hart_files.is_empty() {
            debug!(
                "IMSIC: node at 0x{:x} is not wired to MachineExternal, skipping",
                base_address
            );
            return;
        }

        let machine_hart_count = machine_hart_files.len() as u32;
        let default_hart_index_bits = if machine_hart_count <= 1 {
            0
        } else {
            u32::BITS - (machine_hart_count - 1).leading_zeros()
        };

        let hart_index_bits: u32 = node
            .get_prop("riscv,hart-index-bits")
            .map(|p| p.deserialize::<u32>())
            .unwrap_or(default_hart_index_bits);

        let group_index_bits: u32 = node
            .get_prop("riscv,group-index-bits")
            .map(|p| p.deserialize::<u32>())
            .unwrap_or(0);

        let group_index_shift: u32 = node
            .get_prop("riscv,group-index-shift")
            .map(|p| p.deserialize::<u32>())
            .unwrap_or(24);

        if hart_index_bits >= u32::BITS
            || group_index_bits >= u32::BITS
            || hart_index_bits + group_index_bits > u32::BITS
            || group_index_shift >= u32::BITS
        {
            warn!(
                "IMSIC: invalid topology hart-index-bits={}, group-index-bits={}, group-index-shift={}",
                hart_index_bits, group_index_bits, group_index_shift
            );
            return;
        }

        let firmware_ipi_iid = Iid::new(1).unwrap();

        if firmware_ipi_iid.number() >= num_ids {
            warn!(
                "IMSIC: firmware IPI IID {} outside riscv,num-ids {}",
                firmware_ipi_iid.number(),
                num_ids
            );
            return;
        }

        let layout = AddressLayout {
            machine_base: base_address,
            supervisor_base: 0,
            guest_base: None,
            hart_index_bits,
            group_bits: group_index_shift,
            hart_offset_bits: 12,
            guest_offset_bits: 0,
        };

        let mut hart_imsic_map = [None; NUM_HART_MAX];
        let topology_bits = hart_index_bits + group_index_bits;
        let max_file_count = if topology_bits == 0 {
            1
        } else {
            1u64 << topology_bits
        };
        for &(hart_id, file_index) in machine_hart_files.iter() {
            if hart_id >= NUM_HART_MAX {
                warn!(
                    "IMSIC: hart {} exceeds NUM_HART_MAX {}, skipping AIA",
                    hart_id, NUM_HART_MAX
                );
                return;
            }
            if (file_index as u64) >= max_file_count {
                warn!(
                    "IMSIC: file index {} exceeds topology capacity {}, skipping AIA",
                    file_index, max_file_count
                );
                return;
            }

            let hart_index_mask = if hart_index_bits == 0 {
                0
            } else {
                (1u32 << hart_index_bits) - 1
            };
            let hart_index = file_index & hart_index_mask;
            let group_index_mask = if group_index_bits == 0 {
                0
            } else {
                (1u32 << group_index_bits) - 1
            };
            let group_index = (file_index >> hart_index_bits) & group_index_mask;
            let addr = layout.machine_interrupt_file_address(hart_index, group_index);
            let Some(page_end) = addr.checked_add(0x1000) else {
                warn!(
                    "IMSIC: hart {} file {} page 0x{:x} overflows address space, skipping AIA",
                    hart_id, file_index, addr
                );
                return;
            };
            if !reg_ranges
                .iter()
                .any(|range| addr >= range.start && page_end <= range.end)
            {
                warn!(
                    "IMSIC: hart {} file {} page 0x{:x} outside reg ranges, skipping AIA",
                    hart_id, file_index, addr
                );
                return;
            }
            hart_imsic_map[hart_id] = Some(addr);
        }

        if let Some(ref cpu_enabled) = self.info.cpu_enabled {
            for (hart_id, enabled) in cpu_enabled.iter().enumerate() {
                if *enabled && hart_imsic_map[hart_id].is_none() {
                    warn!(
                        "IMSIC: enabled hart {} has no M-level IMSIC file, skipping AIA",
                        hart_id
                    );
                    return;
                }
            }
        }

        info!(
            "IMSIC: base=0x{:x}, num-ids={}, hart-index-bits={}, group-index-bits={}, group-index-shift={}, firmware-ipi-iid={}",
            base_address,
            num_ids,
            hart_index_bits,
            group_index_bits,
            group_index_shift,
            firmware_ipi_iid.number()
        );

        self.info.aia = Some(aia::AiaInfo {
            layout,
            num_ids,
            firmware_ipi_iid,
            hart_imsic_map,
        });
    }

    fn sbi_ipi_init(&mut self) {
        let max_hart_id = self
            .info
            .cpu_enabled
            .as_ref()
            .and_then(|hart_list| hart_list.iter().rposition(|enabled| *enabled))
            .unwrap_or(NUM_HART_MAX - 1);

        if let Some(ref aia_info) = self.info.aia {
            use crate::sbi::features::{Extension, hart_extension_probe};
            let mut aia_usable = true;
            if let Some(ref cpu_enabled) = self.info.cpu_enabled {
                for (hart_id, enabled) in cpu_enabled.iter().enumerate() {
                    if *enabled {
                        if !hart_extension_probe(hart_id, Extension::Smaia) {
                            warn!("AIA: hart {} lacks Smaia, rejecting AIA", hart_id);
                            aia_usable = false;
                            break;
                        }
                        if !hart_extension_probe(hart_id, Extension::Sstc) {
                            warn!("AIA: hart {} lacks Sstc, rejecting AIA", hart_id);
                            aia_usable = false;
                            break;
                        }
                    }
                }
            }
            if aia_usable {
                let ipi_dev =
                    aia::ImsicDevice::new(aia_info.firmware_ipi_iid, aia_info.hart_imsic_map);
                self.sbi.ipi = Some(SbiIpi::new(Mutex::new(Box::new(ipi_dev)), max_hart_id));
                aia::init_qemu_m_aplic_delegation(
                    aia_info.layout.machine_base,
                    aia_info.layout.hart_index_bits,
                );
                aia::set_aia_active(true);
                info!("AIA: IMSIC IPI + Sstc timer backend initialized");
                return;
            }
            warn!("AIA: requirements not met, falling back to CLINT");
        }
        if let Some((base, clint_type)) = self.info.ipi {
            let ipi_dev: Box<dyn crate::sbi::ipi::IpiDevice> = match clint_type {
                MachineClintType::SiFiveClint => Box::new(SifiveClintWrap::new(base)),
                MachineClintType::TheadClint => Box::new(THeadClintWrap::new(base)),
            };
            self.sbi.ipi = Some(SbiIpi::new(Mutex::new(ipi_dev), max_hart_id));
        }
    }

    fn sbi_hsm_init(&mut self) {
        if self.sbi.ipi.is_some() {
            self.sbi.hsm = Some(SbiHsm);
        } else {
            self.sbi.hsm = None;
        }
    }

    fn sbi_rfence_init(&mut self) {
        if self.sbi.ipi.is_some() {
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
        if aia::is_aia_active()
            && let Some(ref aia_info) = self.info.aia
        {
            info!(
                "{:<30}: IMSIC (M-level Base Address: 0x{:x})",
                "Platform IPI Extension", aia_info.layout.machine_base
            );
            return;
        }
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
