//! Validated CLINT installation shared by the timer and IPI roles.

use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ops::Range;
use core::ptr;
use core::sync::atomic::{AtomicPtr, Ordering};

use dtoolkit::fdt::{Fdt, FdtNode};
use dtoolkit::{Node, Property};

use crate::boot::device_tree::{BindingError, enabled, exact_node, hart_ids, model, reg_ranges};
use crate::boot::{BootInfo, MachineRangeError};
use crate::config::TRUSTED_TARGET;
use crate::hart::{
    HartAdmission, HartControl, Ipi, IpiDevice, IpiError, Notification, RemoteFence,
};
use crate::timer::{Operations as TimerOperations, Timer};

mod riscv;

use riscv::{
    current_hart_id, device_fence, enable_machine_software_interrupt, enable_machine_timer,
    manifest_supervisor_timer, read_time_csr,
};

const QEMU_MODEL: &str = "riscv-virtio,qemu";
const QEMU_CLINT_BASE: usize = 0x0200_0000;
const QEMU_CLINT_SIZE: usize = 0x1_0000;
const MSIP_OFFSET: usize = 0;
const MTIMECMP_OFFSET: usize = 0x4000;
const MTIME_OFFSET: usize = 0xbff8;

const SIFIVE_COMPATIBLE: [&str; 3] = ["riscv,clint0", "starfive,jh7110-clint", "sifive,clint0"];
const THEAD_COMPATIBLE: [&str; 1] = ["thead,c900-clint"];

/// Failure while installing one selected CLINT.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InstallError {
    /// The owned device tree is malformed or the exact node is absent.
    DeviceTree,
    /// The node does not describe a supported CLINT register convention.
    Unsupported,
    /// The register range cannot contain every admitted hart's registers.
    InvalidLayout,
    /// The binding is outside the configured trusted platform.
    Unauthorized,
    /// The range or singleton CLINT installation was already claimed.
    AlreadyOwned,
    /// Hart admission could not be constructed.
    Hardware,
}

/// Validates and binds one CLINT exactly once, returning independent timer and
/// IPI capabilities over one stable private driver allocation.
pub fn install(
    boot: &mut BootInfo,
    node_path: &str,
) -> Result<(Timer, Ipi, RemoteFence, HartControl), InstallError> {
    let layout = ClintLayout::from_dtb(boot, node_path)?;
    boot.ensure_runtime_unbound()
        .map_err(|_| InstallError::AlreadyOwned)?;
    boot.claim_machine_range(layout.range.clone())
        .map_err(|error| match error {
            MachineRangeError::Invalid => InstallError::InvalidLayout,
            MachineRangeError::AlreadyClaimed => InstallError::AlreadyOwned,
        })?;

    let harts = layout.harts;
    let clint = Box::leak(Box::new(Clint {
        base: layout.range.start,
        registers: layout.registers,
        harts: harts.clone(),
    }));
    let device: Arc<dyn IpiDevice> = Arc::new(ClintIpi(clint));
    let wake_by_ipi = alloc::vec![true; harts.len()];
    let admission = HartAdmission::new(device, &harts, boot.init_hart_id(), &wake_by_ipi)
        .map_err(|_| InstallError::Hardware)?;
    INSTALLED_CLINT
        .compare_exchange(
            ptr::null_mut(),
            ptr::from_mut(clint),
            Ordering::AcqRel,
            Ordering::Relaxed,
        )
        .map_err(|_| InstallError::AlreadyOwned)?;
    boot.install_runtime(admission.clone(), &CLINT_TIMER)
        .map_err(|_| InstallError::AlreadyOwned)?;
    Ok((
        Timer::new(&CLINT_TIMER),
        Ipi::new(admission.clone()),
        RemoteFence::new(admission.clone()),
        HartControl::new(admission),
    ))
}

struct ClintLayout {
    range: Range<usize>,
    registers: RegisterLayout,
    harts: Vec<usize>,
}

impl ClintLayout {
    fn from_dtb(boot: &BootInfo, path: &str) -> Result<Self, InstallError> {
        let fdt = Fdt::new(boot.dtb().as_bytes()).map_err(|_| InstallError::DeviceTree)?;
        let node = exact_node(&fdt, path).map_err(map_binding_error)?;
        if !enabled(&node) {
            return Err(InstallError::Unsupported);
        }

        let registers = RegisterLayout::from_node(&node).ok_or(InstallError::Unsupported)?;
        let ranges = reg_ranges(node).map_err(map_binding_error)?;
        if ranges.len() != 1 {
            return Err(InstallError::InvalidLayout);
        }
        let range = ranges.into_iter().next().unwrap();
        let harts = hart_ids(&fdt).map_err(map_binding_error)?;
        validate_registers(&range, registers, &harts)?;

        let canonical_qemu = model(&fdt) == QEMU_MODEL
            && range.start == QEMU_CLINT_BASE
            && range.end >= QEMU_CLINT_BASE + QEMU_CLINT_SIZE;
        if !canonical_qemu && !TRUSTED_TARGET {
            return Err(InstallError::Unauthorized);
        }

        Ok(Self {
            range,
            registers,
            harts,
        })
    }
}

fn map_binding_error(error: BindingError) -> InstallError {
    match error {
        BindingError::DeviceTree => InstallError::DeviceTree,
        BindingError::Unsupported => InstallError::Unsupported,
        BindingError::InvalidRange => InstallError::InvalidLayout,
    }
}

#[derive(Clone, Copy)]
enum RegisterLayout {
    SiFive,
    THead,
}

impl RegisterLayout {
    fn from_node(node: &FdtNode<'_>) -> Option<Self> {
        node.property("compatible")?
            .as_str_list()
            .find_map(|compatible| {
                if THEAD_COMPATIBLE.contains(&compatible)
                    || (SIFIVE_COMPATIBLE.contains(&compatible)
                        && node.property("clint,has-no-64bit-mmio").is_some())
                {
                    Some(Self::THead)
                } else if SIFIVE_COMPATIBLE.contains(&compatible) {
                    Some(Self::SiFive)
                } else {
                    None
                }
            })
    }
}

fn validate_registers(
    range: &Range<usize>,
    registers: RegisterLayout,
    harts: &[usize],
) -> Result<(), InstallError> {
    for &hart_id in harts {
        let msip_end = hart_id
            .checked_mul(4)
            .and_then(|offset| MSIP_OFFSET.checked_add(offset))
            .and_then(|offset| offset.checked_add(4))
            .and_then(|offset| range.start.checked_add(offset))
            .ok_or(InstallError::InvalidLayout)?;
        let compare_end = hart_id
            .checked_mul(8)
            .and_then(|offset| MTIMECMP_OFFSET.checked_add(offset))
            .and_then(|offset| offset.checked_add(8))
            .and_then(|offset| range.start.checked_add(offset))
            .ok_or(InstallError::InvalidLayout)?;
        if msip_end > range.end || compare_end > range.end {
            return Err(InstallError::InvalidLayout);
        }
    }
    if matches!(registers, RegisterLayout::SiFive)
        && range
            .start
            .checked_add(MTIME_OFFSET + 8)
            .is_none_or(|end| end > range.end)
    {
        return Err(InstallError::InvalidLayout);
    }
    Ok(())
}

struct Clint {
    base: usize,
    registers: RegisterLayout,
    harts: Vec<usize>,
}

static INSTALLED_CLINT: AtomicPtr<Clint> = AtomicPtr::new(ptr::null_mut());

static CLINT_TIMER: TimerOperations = TimerOperations {
    prepare_current_hart: timer_prepare_current_hart,
    read_time: timer_read_time,
    set_deadline: timer_set_deadline,
    handle_interrupt: timer_handle_interrupt,
};

impl Clint {
    fn contains_hart(&self, hart_id: usize) -> bool {
        self.harts.contains(&hart_id)
    }

    fn msip(&self, hart_id: usize) -> *mut u32 {
        (self.base + MSIP_OFFSET + hart_id * 4) as *mut u32
    }

    fn mtimecmp(&self, hart_id: usize) -> *mut u32 {
        (self.base + MTIMECMP_OFFSET + hart_id * 8) as *mut u32
    }

    fn read_time_register(&self) -> u64 {
        let register = (self.base + MTIME_OFFSET) as *const u32;
        loop {
            // SAFETY: construction proves that both words lie in this
            // exclusively claimed CLINT range. The high-low-high sequence is
            // stable across a low-word rollover on RV32 and RV64.
            let high_before = unsafe { register.add(1).read_volatile() };
            // SAFETY: same validated two-word register as above.
            let low = unsafe { register.read_volatile() };
            // SAFETY: same validated two-word register as above.
            let high_after = unsafe { register.add(1).read_volatile() };
            if high_before == high_after {
                return (u64::from(high_after) << 32) | u64::from(low);
            }
        }
    }
}

fn installed() -> Option<&'static Clint> {
    let clint = INSTALLED_CLINT.load(Ordering::Acquire);
    // SAFETY: a non-null pointer names the leaked, immutable CLINT installed
    // before publication and retained for the firmware lifetime.
    unsafe { clint.as_ref() }
}

fn timer_prepare_current_hart() -> Result<(), crate::TimerError> {
    let Some(clint) = installed() else {
        return Err(crate::TimerError::Unavailable);
    };
    clint
        .contains_hart(current_hart_id())
        .then_some(())
        .ok_or(crate::TimerError::InvalidHart)
}

fn timer_read_time() -> u64 {
    installed().map_or(0, |clint| match clint.registers {
        RegisterLayout::SiFive => clint.read_time_register(),
        RegisterLayout::THead => read_time_csr(),
    })
}

fn timer_set_deadline(deadline: u64) {
    let hart_id = current_hart_id();
    let Some(clint) = installed().filter(|clint| clint.contains_hart(hart_id)) else {
        return;
    };
    let register = clint.mtimecmp(hart_id);
    // The low-max/high/low sequence prevents an intermediate compare below
    // both the old and new deadlines on a 32-bit register interface.
    // SAFETY: construction validates this hart's complete compare pair.
    unsafe { register.write_volatile(u32::MAX) };
    // SAFETY: second word of the same validated pair.
    unsafe { register.add(1).write_volatile((deadline >> 32) as u32) };
    // SAFETY: first word of the same validated pair.
    unsafe { register.write_volatile(deadline as u32) };
    enable_machine_timer();
}

fn timer_handle_interrupt() -> bool {
    manifest_supervisor_timer();
    true
}

struct ClintIpi(&'static Clint);

impl IpiDevice for ClintIpi {
    fn prepare_current_hart(&self) -> Result<(), IpiError> {
        let hart_id = current_hart_id();
        if !self.0.contains_hart(hart_id) {
            return Err(IpiError::InvalidHart);
        }
        self.claim(hart_id);
        enable_machine_software_interrupt();
        Ok(())
    }

    fn notify(&self, hart_id: usize) {
        if !self.0.contains_hart(hart_id) {
            return;
        }
        device_fence();
        // SAFETY: construction validates this hart's MSIP word and claims the
        // complete CLINT range for this one driver.
        unsafe { self.0.msip(hart_id).write_volatile(1) };
        device_fence();
    }

    fn claim(&self, hart_id: usize) {
        if !self.0.contains_hart(hart_id) {
            return;
        }
        // SAFETY: construction validates this hart's MSIP word and claims the
        // complete CLINT range for this one driver.
        unsafe { self.0.msip(hart_id).write_volatile(0) };
        device_fence();
    }

    #[inline(never)]
    fn notification(&self) -> Notification {
        Notification::Software
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dtoolkit::model::{DeviceTree, DeviceTreeNode, DeviceTreeProperty};

    fn qemu_dtb(clint_name: &str) -> Vec<u8> {
        let mut tree = DeviceTree::new();
        tree.root.add_property(
            DeviceTreeProperty::new("model", b"riscv-virtio,qemu\0".to_vec()).unwrap(),
        );
        tree.root
            .add_property(DeviceTreeProperty::new("#address-cells", 2u32.to_be_bytes()).unwrap());
        tree.root
            .add_property(DeviceTreeProperty::new("#size-cells", 2u32.to_be_bytes()).unwrap());
        let cpu = DeviceTreeNode::builder("cpu@0")
            .unwrap()
            .property(DeviceTreeProperty::new("reg", 0u32.to_be_bytes()).unwrap())
            .build();
        let cpus = DeviceTreeNode::builder("cpus")
            .unwrap()
            .property(DeviceTreeProperty::new("#address-cells", 1u32.to_be_bytes()).unwrap())
            .child(cpu)
            .build();
        tree.root.add_child(cpus);

        let mut reg = Vec::new();
        reg.extend_from_slice(&0u32.to_be_bytes());
        reg.extend_from_slice(&(QEMU_CLINT_BASE as u32).to_be_bytes());
        reg.extend_from_slice(&0u32.to_be_bytes());
        reg.extend_from_slice(&(QEMU_CLINT_SIZE as u32).to_be_bytes());
        let clint = DeviceTreeNode::builder(clint_name)
            .unwrap()
            .property(DeviceTreeProperty::new("compatible", b"riscv,clint0\0".to_vec()).unwrap())
            .property(DeviceTreeProperty::new("reg", reg).unwrap())
            .build();
        tree.root.add_child(clint);
        tree.to_dtb()
    }

    #[test]
    fn sparse_hart_registers_must_fit_the_complete_range() {
        assert!(
            validate_registers(&(0x200_0000..0x201_0000), RegisterLayout::SiFive, &[0, 8]).is_ok()
        );
        assert_eq!(
            validate_registers(&(0x200_0000..0x200_4040), RegisterLayout::SiFive, &[8]),
            Err(InstallError::InvalidLayout)
        );
    }

    #[test]
    fn thead_time_does_not_require_an_mtime_mmio_register() {
        assert!(validate_registers(&(0x1000..0x6000), RegisterLayout::THead, &[0]).is_ok());
        assert_eq!(
            validate_registers(&(0x1000..0x6000), RegisterLayout::SiFive, &[0]),
            Err(InstallError::InvalidLayout)
        );
    }

    #[test]
    fn binding_uses_exact_node_identity_and_claims_the_range_once() {
        let mut boot = BootInfo::from_test_dtb(qemu_dtb("timer@2000000"));
        assert!(install(&mut boot, "/timer@2000000").is_ok());
        assert_eq!(
            install(&mut boot, "/timer@2000000").err(),
            Some(InstallError::AlreadyOwned)
        );
        assert_eq!(
            install(&mut boot, "/clint@2000000").err(),
            Some(InstallError::DeviceTree)
        );
    }
}
