//! Board discovery, bounded MMIO regions, and platform-global state.

use alloc::string::String;
use alloc::{string::ToString, vec::Vec};
use core::{
    ops::Range,
    sync::atomic::{AtomicBool, Ordering},
};

use crate::cfg::NUM_HART_MAX;
use crate::devicetree::*;
use crate::driver;
use crate::sbi::features::extension_detection;
use riscv_aia::Iid;
use riscv_aia::peripheral::imsic::AddressLayout;
use spin::{Once, RwLock};

mod boot;
pub(crate) mod mmio;
pub(crate) mod qemu_aplic;

pub use boot::{init_board, memory_range, secondary_hart_init, wait_until_ready};

pub(crate) static CPU_PRIVILEGED_ENABLED: [AtomicBool; NUM_HART_MAX] =
    [const { AtomicBool::new(false) }; NUM_HART_MAX];

/// Set to true once init detects the platform is a SpacemiT K1 / Ky X1.
///
/// Written in `init_board` *before* the ready flag is released, so that a
/// secondary hart observing `READY == true` is guaranteed to also observe
/// this flag (main.rs secondary-hart path).
pub(crate) static IS_K1_PLATFORM: AtomicBool = AtomicBool::new(false);

/// Boot synchronization flag: set by the boot hart once platform
/// initialization completes; secondary harts observe it with Acquire
/// ordering in [`wait_until_ready`].
pub(crate) static READY: AtomicBool = AtomicBool::new(false);

/// Board facts discovered from the FDT.
static BOARD_INFO: Once<BoardInfo> = Once::new();

/// Enabled-hart table; rewritten once post-`READY` by `refresh_cpu_features`.
static CPU_ENABLED: RwLock<Option<CpuEnableList>> = RwLock::new(None);

/// Returns the published board information.
///
/// # Panics
///
/// Panics if called before `init_board` publishes the board; publication
/// precedes the `READY` release, so runtime readers gated on `READY`
/// cannot race it.
pub(crate) fn board_info() -> &'static BoardInfo {
    BOARD_INFO
        .get()
        .expect("board published before any runtime reader")
}

/// Returns a copy of the enabled-CPU table (`None` before `init_board`
/// publishes it).
pub(crate) fn cpu_enabled() -> Option<CpuEnableList> {
    *CPU_ENABLED.read()
}

/// Publishes the enabled-CPU list (initial, during `init_board`).
pub(crate) fn publish_cpu_enabled(cpu_list: Option<CpuEnableList>) {
    *CPU_ENABLED.write() = cpu_list;
}

/// Reconciles the enabled-CPU table with the per-hart privilege checks.
///
/// Runs post-`READY` on the boot hart while other harts may read the table;
/// the write lock makes their copies atomic snapshots of the whole list.
pub fn refresh_cpu_features() {
    let mut cpu_enabled = CPU_ENABLED.write();
    let Some(cpu_enabled) = cpu_enabled.as_mut() else {
        return;
    };
    for (hart_id, enabled) in cpu_enabled.iter_mut().enumerate() {
        if *enabled {
            *enabled = CPU_PRIVILEGED_ENABLED[hart_id].load(Ordering::Acquire);
        }
    }
}

const RISCV_MACHINE_EXTERNAL_IRQ: u32 = 11;

/// Largest 7-bit I2C slave address; the P1 PMIC driver does not support
/// 10-bit addressing.
const MAX_7BIT_I2C_ADDRESS: usize = 0x7f;

/// AIA candidate discovered from the FDT; device selection happens in the
/// driver layer.
pub struct AiaInfo {
    pub layout: AddressLayout,
    pub num_ids: u16,
    pub firmware_ipi_iid: Iid,
    pub hart_imsic_map: [Option<usize>; NUM_HART_MAX],
}

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

/// Console candidate selected from the FDT's `stdout-path` node.
pub(crate) struct ConsoleInfo {
    pub(crate) base_address: BaseAddress,
    /// Driver selected from the node's `compatible` string and register
    /// layout.
    pub(crate) kind: driver::ConsoleKind,
    /// `clock-frequency` property, when present.
    pub(crate) clock_hz: Option<u32>,
}

pub struct BoardInfo {
    pub memory_range: Option<Range<usize>>,
    pub(crate) console: Option<ConsoleInfo>,
    /// Reset (test finisher) device base address, when discovered.
    pub reset: Option<BaseAddress>,
    /// CLINT device (base address and kind), when discovered.
    pub ipi: Option<(BaseAddress, driver::ClintKind)>,
    pub aia: Option<AiaInfo>,
    pub cpu_num: Option<usize>,
    pub model: String,
    /// P1 PMIC reset info: (I2C controller base, PMIC address)
    pub pmic_reset: Option<(usize, u8)>,
    mmio_regions: Vec<(usize, usize)>,
}

impl BoardInfo {
    pub const fn new() -> Self {
        BoardInfo {
            memory_range: None,
            console: None,
            reset: None,
            ipi: None,
            aia: None,
            cpu_num: None,
            model: String::new(),
            pmic_reset: None,
            mmio_regions: Vec::new(),
        }
    }

    pub fn is_qemu_virt(&self) -> bool {
        self.model == "riscv-virtio,qemu"
    }

    fn record_mmio_range(&mut self, range: &Range<usize>) {
        let Some(length) = range.end.checked_sub(range.start) else {
            return;
        };
        let region = (range.start, length);
        if length != 0 && !self.mmio_regions.contains(&region) {
            self.mmio_regions.push(region);
        }
    }

    /// Discovers the console device from the FDT (chosen stdout path).
    pub(crate) fn discover_console(&mut self, root: &serde_device_tree::buildin::Node) {
        let Some(stdout_path) = root.chosen_stdout_path() else {
            return;
        };
        let Some(node) = root.find(stdout_path) else {
            return;
        };
        let Some((compatible, regs)) = get_compatible_and_range(&node) else {
            return;
        };

        let register_shift = node
            .get_prop("reg-shift")
            .map(|property| property.deserialize::<u32>());
        let register_width = node
            .get_prop("reg-io-width")
            .map(|property| property.deserialize::<u32>());
        for compatible in compatible.iter() {
            // Interpret the node once: the selected kind replaces the raw
            // `compatible` and layout properties in the stored board info.
            if let Some(kind) =
                driver::ConsoleKind::from_fdt(compatible, register_shift, register_width)
            {
                self.console = Some(ConsoleInfo {
                    base_address: regs.start,
                    kind,
                    clock_hz: node
                        .get_prop("clock-frequency")
                        .map(|property| property.deserialize::<u32>()),
                });
                self.record_mmio_range(&regs);
                return;
            }
        }
    }

    /// Discovers miscellaneous board info (memory, cpu number, model, enabled
    /// harts) that later platform initialization depends on; returns the
    /// enabled-hart list for the caller to publish (`publish_cpu_enabled`).
    pub(crate) fn discover_misc(&mut self, tree: &Tree) -> Option<CpuEnableList> {
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

        self.cpu_num = Some(tree.cpus.cpu.len());

        if let Some(ref model) = tree.model {
            let model = model.iter().next().unwrap_or("<unspecified>");
            self.model = model.to_string();
        } else {
            let model = "<unspecified>";
            self.model = model.to_string();
        }

        // QEMU's machine-level APLIC is fixed by the `virt` machine model
        // rather than described by the supervisor-visible FDT. Record that
        // trusted window here so it crosses the same bounded-MMIO acquisition
        // boundary as discovered devices.
        if self.is_qemu_virt()
            && let Some(end) =
                qemu_aplic::QEMU_VIRT_M_APLIC_BASE.checked_add(qemu_aplic::APLIC_SPAN)
        {
            self.record_mmio_range(&(qemu_aplic::QEMU_VIRT_M_APLIC_BASE..end));
        }

        // TODO: Need a better extension initialization method
        extension_detection(&tree.cpus.cpu);

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
        Some(cpu_list)
    }

    /// Discovers the ipi and reset devices (including the M-level IMSIC) from
    /// the FDT.
    pub(crate) fn discover_devices(&mut self, root: &serde_device_tree::buildin::Node) {
        let cpu_intc_harts = collect_cpu_intc_harts(root);
        let mut find_device =
            |node: &serde_device_tree::buildin::Node,
             parent: Option<&serde_device_tree::buildin::Node>| {
                let Some((compatible, regs)) = get_compatible_and_ranges(node) else {
                    return;
                };
                let device_range = &regs[0];
                for compatible in compatible.iter() {
                    self.discover_clint(node, compatible, device_range);
                    self.discover_reset(compatible, device_range);
                    self.discover_p1_pmic_reset(compatible, device_range.start, parent);
                    // Discover the M-level IMSIC from its CPU interrupt wiring.
                    if driver::IMSIC_COMPATIBLES.contains(&compatible) && self.aia.is_none() {
                        self.discover_imsic(node, &regs, &cpu_intc_harts);
                    }
                }
            };
        search_with_parent(root, &mut find_device);
    }

    fn discover_clint(
        &mut self,
        node: &serde_device_tree::buildin::Node,
        compatible: &str,
        range: &Range<usize>,
    ) {
        let has_no_64bit_mmio = node.get_prop("clint,has-no-64bit-mmio").is_some();
        if let Some(kind) = driver::ClintKind::from_fdt(compatible, has_no_64bit_mmio) {
            self.ipi = Some((range.start, kind));
            self.record_mmio_range(range);
        }
    }

    fn discover_reset(&mut self, compatible: &str, range: &Range<usize>) {
        if driver::SIFIVE_TEST_COMPATIBLES.contains(&compatible) {
            self.reset = Some(range.start);
            self.record_mmio_range(range);
        }
    }

    fn discover_p1_pmic_reset(
        &mut self,
        compatible: &str,
        base_address: usize,
        parent: Option<&serde_device_tree::buildin::Node>,
    ) {
        if !driver::P1_PMIC_COMPATIBLES.contains(&compatible) || base_address > MAX_7BIT_I2C_ADDRESS
        {
            return;
        }
        let Some(parent) = parent else {
            return;
        };
        let Some((parent_compatibles, parent_ranges)) = get_compatible_and_ranges(parent) else {
            return;
        };
        if !parent_compatibles
            .iter()
            .any(|parent_compatible| driver::PMIC_I2C_COMPATIBLES.contains(&parent_compatible))
        {
            return;
        }
        let Some(i2c_range) = parent_ranges.first() else {
            return;
        };
        self.pmic_reset = Some((i2c_range.start, base_address as u8));
        self.record_mmio_range(i2c_range);
    }

    fn discover_imsic(
        &mut self,
        node: &serde_device_tree::buildin::Node,
        reg_ranges: &[Range<usize>],
        cpu_intc_harts: &[(u32, usize)],
    ) {
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
        let num_ids = num_ids_prop.deserialize::<u32>();
        // AIA requires each interrupt file to support 63..=2047 identities:
        // the architectural minimum is 63, and IID selectors are 11 bits with
        // identity 0 reserved, bounding the count at 2047. The Linux
        // `riscv,imsics` DT binding encodes the same range.
        if !(63..=2047).contains(&num_ids) {
            warn!(
                "IMSIC: riscv,num-ids {} is outside 63..=2047, skipping AIA",
                num_ids
            );
            return;
        }
        let num_ids = num_ids as u16;

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
            let Some(page_end) = addr.checked_add(driver::IMSIC_FILE_SPAN) else {
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

        let Some(cpu_enabled) = cpu_enabled() else {
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

        for range in reg_ranges {
            self.record_mmio_range(range);
        }
        self.aia = Some(AiaInfo {
            layout,
            num_ids,
            firmware_ipi_iid,
            hart_imsic_map,
        });
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

    let Some(cpu_enabled) = cpu_enabled() else {
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
    if crate::sbi::ipi::uses_imsic()
        && let Some(ref aia_info) = board_info().aia
    {
        info!(
            "{:<30}: IMSIC (M-level Base Address: 0x{:x})",
            "Platform IPI Extension", aia_info.layout.machine_base
        );
        return;
    }
    match board_info().ipi.as_ref() {
        Some((base, kind)) => {
            info!(
                "{:<30}: {} (Base Address: 0x{:x})",
                "Platform IPI Extension",
                kind.name(),
                base
            );
        }
        None => warn!("{:<30}: Not Available", "Platform IPI Device"),
    }
}

#[inline]
fn print_console_info() {
    match board_info().console.as_ref() {
        Some(console) => {
            info!(
                "{:<30}: {} (Base Address: 0x{:x})",
                "Platform Console Extension",
                console.kind.name(),
                console.base_address
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
