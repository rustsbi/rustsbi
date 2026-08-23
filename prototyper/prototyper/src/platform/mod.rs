use alloc::string::String;
use alloc::{string::ToString, vec::Vec};
use core::{
    ops::Range,
    sync::atomic::{AtomicBool, Ordering},
};

use crate::cfg::NUM_HART_MAX;
use crate::devicetree::*;
use crate::platform::clint::{MachineClintType, SIFIVE_CLINT_COMPATIBLE, THEAD_CLINT_COMPATIBLE};
use crate::platform::console::{
    MachineConsoleType, UART16650U8_COMPATIBLE, UART16650U32_COMPATIBLE, UARTAXILITE_COMPATIBLE,
    UARTBFLB_COMPATIBLE, UARTPL011_COMPATIBLE, UARTSIFIVE_COMPATIBLE, UARTXSCALE_COMPATIBLE,
};
use crate::platform::reset::P1_PMIC_COMPATIBLE;
use crate::platform::reset::SIFIVETEST_COMPATIBLE;
use crate::sbi::features::extension_detection;

pub(crate) mod aia;
mod boot;
pub(crate) mod clint;
pub(crate) mod console;
pub(crate) mod reset;

pub use boot::{
    init_board, memory_range, refresh_enabled_cpus, secondary_hart_init, wait_until_ready,
};

pub(crate) static CPU_PRIVILEGED_ENABLED: [AtomicBool; NUM_HART_MAX] =
    [const { AtomicBool::new(false) }; NUM_HART_MAX];

/// Set to true once init detects the platform is a SpacemiT K1 / Ky X1.
///
/// Written in `init_board` *before* the ready flag is released, so that a
/// secondary hart observing `READY == true` is guaranteed to also observe
/// this flag (main.rs secondary-hart path).
pub(crate) static IS_K1_PLATFORM: AtomicBool = AtomicBool::new(false);

/// Boot synchronization flag: set by the boot hart once platform
/// initialization is complete, spinning secondary harts observe it with
/// Acquire ordering (`wait_until_ready`).
pub(crate) static READY: AtomicBool = AtomicBool::new(false);

/// Board information discovered from the FDT, owned by the platform layer.
///
/// Invariant: written by the boot hart *before* `READY` is released, read
/// afterwards; `wait_until_ready` (Acquire) is the secondary-hart edge.
///
/// Pre-existing exception (preserved, not hardened): `refresh_cpu_features`
/// rewrites `cpu_enabled` after `READY` while other harts may read it
/// concurrently (bool array, copied out per call).
static mut BOARD_INFO: BoardInfo = BoardInfo::new();

/// Returns a shared view of the discovered board information.
pub(crate) fn board_info() -> &'static BoardInfo {
    unsafe { &BOARD_INFO }
}

/// Returns an exclusive view of the board information; used by the discovery
/// (`BoardInfo::discover_*`) and refresh (`BoardInfo::refresh_cpu_features`)
/// paths only.
pub(crate) fn board_info_mut() -> &'static mut BoardInfo {
    unsafe { &mut BOARD_INFO }
}

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
    pub console_clock: Option<u32>,
    pub reset: Option<BaseAddress>,
    pub ipi: Option<(BaseAddress, MachineClintType)>,
    pub aia: Option<aia::AiaInfo>,
    pub cpu_num: Option<usize>,
    pub cpu_enabled: Option<CpuEnableList>,
    pub model: String,
    /// P1 PMIC reset info: (I2C controller base, PMIC address)
    pub pmic_reset: Option<(usize, u8)>,
}

impl BoardInfo {
    pub const fn new() -> Self {
        BoardInfo {
            memory_range: None,
            console: None,
            console_clock: None,
            reset: None,
            ipi: None,
            aia: None,
            cpu_enabled: None,
            cpu_num: None,
            model: String::new(),
            pmic_reset: None,
        }
    }

    pub fn is_qemu_virt(&self) -> bool {
        self.model == "riscv-virtio,qemu"
    }

    /// Discovers the console device from the FDT (chosen stdout path).
    pub(crate) fn discover_console(&mut self, root: &serde_device_tree::buildin::Node) {
        //  Get console device info
        let Some(stdout_path) = root.chosen_stdout_path() else {
            return;
        };
        let Some(node) = root.find(stdout_path) else {
            return;
        };
        let Some((compatible, regs)) = get_compatible_and_range(&node) else {
            return;
        };

        for device_id in compatible.iter() {
            if UART16650U8_COMPATIBLE.contains(&device_id) {
                self.console = Some((regs.start, MachineConsoleType::Uart16550U8));
            }
            if UART16650U32_COMPATIBLE.contains(&device_id) {
                self.console = Some((regs.start, MachineConsoleType::Uart16550U32));
            }
            if UARTAXILITE_COMPATIBLE.contains(&device_id) {
                self.console = Some((regs.start, MachineConsoleType::UartAxiLite));
            }
            if UARTBFLB_COMPATIBLE.contains(&device_id) {
                self.console = Some((regs.start, MachineConsoleType::UartBflb));
            }
            if UARTSIFIVE_COMPATIBLE.contains(&device_id) {
                self.console = Some((regs.start, MachineConsoleType::UartSifive));
            }
            if UARTPL011_COMPATIBLE.contains(&device_id) {
                self.console = Some((regs.start, MachineConsoleType::UartPl011));
            }
            if UARTXSCALE_COMPATIBLE.contains(&device_id) {
                self.console = Some((regs.start, MachineConsoleType::UartXscale));
                self.console_clock = node
                    .get_prop("clock-frequency")
                    .map(|prop_item| prop_item.deserialize::<u32>());
            }
        }
    }

    /// Discovers miscellaneous board info (memory, cpu number, model, enabled
    /// harts) that later platform initialization depends on.
    pub(crate) fn discover_misc(&mut self, tree: &Tree) {
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
        self.memory_range = Some(memory_range);

        // Get cpu number info
        self.cpu_num = Some(tree.cpus.cpu.len());

        // Get model info
        if let Some(ref model) = tree.model {
            let model = model.iter().next().unwrap_or("<unspecified>");
            self.model = model.to_string();
        } else {
            let model = "<unspecified>";
            self.model = model.to_string();
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
        self.cpu_enabled = Some(cpu_list);
    }

    /// Discovers the ipi and reset devices (including the M-level IMSIC) from
    /// the FDT.
    pub(crate) fn discover_devices(&mut self, root: &serde_device_tree::buildin::Node) {
        // Get ipi and reset device info
        let cpu_intc_harts = collect_cpu_intc_harts(root);
        let mut find_device =
            |node: &serde_device_tree::buildin::Node,
             parent: Option<&serde_device_tree::buildin::Node>| {
                let Some((compatible, regs)) = get_compatible_and_ranges(node) else {
                    return;
                };
                let base_address = regs[0].start;
                for device_id in compatible.iter() {
                    self.discover_clint(node, device_id, base_address);
                    self.discover_reset(device_id, base_address);
                    self.discover_pmic_reset(device_id, base_address, parent);
                    // Discover the M-level IMSIC from its CPU interrupt wiring.
                    if aia::IMSIC_COMPATIBLE.contains(&device_id) && self.aia.is_none() {
                        self.discover_imsic(node, &regs, &cpu_intc_harts);
                    }
                }
            };
        search_with_parent(root, &mut find_device);
    }

    /// Discovers the CLINT device from a `compatible` match.
    fn discover_clint(
        &mut self,
        node: &serde_device_tree::buildin::Node,
        device_id: &str,
        base_address: usize,
    ) {
        // Initialize clint device.
        if SIFIVE_CLINT_COMPATIBLE.contains(&device_id) {
            if node.get_prop("clint,has-no-64bit-mmio").is_some() {
                self.ipi = Some((base_address, MachineClintType::TheadClint));
            } else {
                self.ipi = Some((base_address, MachineClintType::SiFiveClint));
            }
        } else if THEAD_CLINT_COMPATIBLE.contains(&device_id) {
            self.ipi = Some((base_address, MachineClintType::TheadClint));
        }
    }

    /// Discovers the sifive-test reset device from a `compatible` match.
    fn discover_reset(&mut self, device_id: &str, base_address: usize) {
        // Initialize reset device.
        if SIFIVETEST_COMPATIBLE.contains(&device_id) {
            self.reset = Some(base_address);
        }
    }

    /// Discovers the P1 PMIC reset device from a `compatible` match.
    fn discover_pmic_reset(
        &mut self,
        device_id: &str,
        base_address: usize,
        parent: Option<&serde_device_tree::buildin::Node>,
    ) {
        // Initialize P1 PMIC reset device
        if !P1_PMIC_COMPATIBLE.contains(&device_id) {
            return;
        }
        // The PMIC's own "reg" property is its 7-bit I2C slave address.
        let pmic_addr = base_address as u8;
        // The I2C controller is the PMIC's parent node; use the first
        // register range of the parent as the controller MMIO base,
        // falling back to the PMIC's own reg if no parent is found.
        let i2c_base = parent
            .and_then(|p| get_compatible_and_ranges(p))
            .and_then(|(_, parent_regs)| parent_regs.first().map(|r| r.start))
            .unwrap_or(base_address);
        self.pmic_reset = Some((i2c_base, pmic_addr));
    }

    fn discover_imsic(
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
            hart_index_bits,
            group_bits: group_index_shift,
            hart_offset_bits: 12,
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

        let Some(ref cpu_enabled) = self.cpu_enabled else {
            return;
        };
        for (hart_id, enabled) in cpu_enabled.iter().enumerate() {
            if *enabled && hart_imsic_map[hart_id].is_none() {
                warn!(
                    "IMSIC: enabled hart {} has no M-level IMSIC file, skipping AIA",
                    hart_id
                );
                return;
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

        self.aia = Some(aia::AiaInfo {
            layout,
            num_ids,
            firmware_ipi_iid,
            hart_imsic_map,
        });
    }

    /// Reconciles the enabled-CPU table with the per-hart privilege checks.
    pub(crate) fn refresh_cpu_features(&mut self) {
        let Some(cpu_enabled) = self.cpu_enabled.as_mut() else {
            return;
        };
        for (hart_id, enabled) in cpu_enabled.iter_mut().enumerate() {
            if *enabled {
                *enabled = CPU_PRIVILEGED_ENABLED[hart_id].load(Ordering::Acquire);
            }
        }
    }
}

pub(crate) fn print_board_info() {
    info!("RustSBI version {}", rustsbi::VERSION);
    rustsbi::LOGO.lines().for_each(|line| info!("{}", line));
    info!("Initializing RustSBI machine-mode environment.");

    print_platform_info();
    print_cpu_info();
    print_device_info();
    print_memory_info();
    print_additional_info();
}

#[inline]
fn print_platform_info() {
    info!("{:<30}: {}", "Platform Name", board_info().model);
}

fn print_cpu_info() {
    info!(
        "{:<30}: {:?}",
        "Platform HART Count",
        board_info().cpu_num.unwrap_or(0)
    );

    let Some(cpu_enabled) = &board_info().cpu_enabled else {
        warn!("{:<30}: Not Available", "Enabled HARTs");
        return;
    };
    let mut enabled_harts = [0; NUM_HART_MAX];
    let mut count = 0;
    for (i, &enabled) in cpu_enabled.iter().enumerate() {
        if enabled {
            enabled_harts[count] = i;
            count += 1;
        }
    }
    info!("{:<30}: {:?}", "Enabled HARTs", &enabled_harts[..count]);
}

#[inline]
fn print_device_info() {
    print_clint_info();
    print_console_info();
    print_reset_info();
    print_hsm_info();
    print_rfence_info();
    print_susp_info();
    print_pmu_info();
}

#[inline]
fn print_clint_info() {
    if aia::is_aia_active()
        && let Some(ref aia_info) = board_info().aia
    {
        info!(
            "{:<30}: IMSIC (M-level Base Address: 0x{:x})",
            "Platform IPI Extension", aia_info.layout.machine_base
        );
        return;
    }
    match board_info().ipi {
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
fn print_console_info() {
    match board_info().console {
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
fn print_reset_info() {
    if let Some(base) = board_info().reset {
        info!(
            "{:<30}: Available (Base Address: 0x{:x})",
            "Platform Reset Extension", base
        );
    } else if let Some((i2c_base, pmic_addr)) = board_info().pmic_reset {
        info!(
            "{:<30}: Available (P1 PMIC @ 0x{:02x}, I2C Base: 0x{:x})",
            "Platform Reset Extension", pmic_addr, i2c_base
        );
    } else {
        warn!("{:<30}: Not Available", "Platform Reset Device");
    }
}

#[inline]
fn print_hsm_info() {
    if crate::sbi::hsm().is_some() {
        info!("{:<30}: {}", "Platform HSM Extension", "Available");
    } else {
        warn!("{:<30}: {}", "Platform HSM Extension", "Not Available");
    }
}

#[inline]
fn print_rfence_info() {
    if crate::sbi::rfence().is_some() {
        info!("{:<30}: {}", "Platform RFence Extension", "Available");
    } else {
        warn!("{:<30}: {}", "Platform RFence Extension", "Not Available");
    }
}

#[inline]
fn print_susp_info() {
    if crate::sbi::susp().is_some() {
        info!("{:<30}: {}", "Platform SUSP Extension", "Available");
    } else {
        warn!("{:<30}: {}", "Platform SUSP Extension", "Not Available");
    }
}

#[inline]
fn print_pmu_info() {
    if crate::sbi::pmu().is_some() {
        info!("{:<30}: {}", "Platform PMU Extension", "Available");
    } else {
        warn!("{:<30}: {}", "Platform PMU Extension", "Not Available");
    }
}

#[inline]
fn print_memory_info() {
    if let Some(memory_range) = &board_info().memory_range {
        info!(
            "{:<30}: 0x{:x} - 0x{:x}",
            "Memory range", memory_range.start, memory_range.end
        );
    } else {
        warn!("{:<30}: Not Available", "Memory range");
    }
}

#[inline]
fn print_additional_info() {
    if !READY.load(Ordering::Acquire) {
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
