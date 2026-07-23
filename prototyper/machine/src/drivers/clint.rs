//! Validated CLINT binding shared by the timer and IPI roles.

use alloc::sync::Arc;
use alloc::vec::Vec;
use core::ops::Range;

use dtoolkit::fdt::{Fdt, FdtNode};
use dtoolkit::{Node, Property};

use super::DriverError;
use crate::boot::device_tree::{BindingError, enabled, exact_node, hart_ids, model, reg_ranges};
use crate::boot::{BootInfo, MachineRangeError};
use crate::config::TRUSTED_TARGET;
use crate::hart::{
    HartAdmission, HartControl, Ipi, IpiDevice, IpiError, Notification, RemoteFence,
};
use crate::timer::{Timer, TimerDevice};

mod arch;

use arch::{
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

/// Validates and binds one CLINT exactly once, returning independent timer and
/// IPI capabilities over one stable private driver allocation.
pub fn build(
    boot: &mut BootInfo,
    node_path: &str,
) -> Result<(Timer, Ipi, RemoteFence, HartControl), DriverError> {
    let binding = Binding::from_dtb(boot, node_path)?;
    boot.ensure_runtime_unbound()
        .map_err(|_| DriverError::AlreadyOwned)?;
    boot.claim_machine_range(binding.range.clone())
        .map_err(|error| match error {
            MachineRangeError::Invalid => DriverError::InvalidRange,
            MachineRangeError::AlreadyClaimed => DriverError::AlreadyOwned,
        })?;

    let harts = binding.harts;
    let driver = Arc::new(Clint {
        base: binding.range.start,
        kind: binding.kind,
        harts: harts.clone(),
    });
    let timer: Arc<dyn TimerDevice> = driver.clone();
    let device: Arc<dyn IpiDevice> = driver;
    let wake_by_ipi = alloc::vec![true; harts.len()];
    let admission = HartAdmission::new(device, &harts, boot.init_hart_id(), &wake_by_ipi)
        .map_err(|_| DriverError::Hardware)?;
    boot.install_runtime(admission.clone(), timer.clone())
        .map_err(|_| DriverError::AlreadyOwned)?;
    Ok((
        Timer::new(timer),
        Ipi::new(admission.clone()),
        RemoteFence::new(admission.clone()),
        HartControl::new(admission),
    ))
}

struct Binding {
    range: Range<usize>,
    kind: Kind,
    harts: Vec<usize>,
}

impl Binding {
    fn from_dtb(boot: &BootInfo, path: &str) -> Result<Self, DriverError> {
        let fdt = Fdt::new(boot.dtb().as_bytes()).map_err(|_| DriverError::DeviceTree)?;
        let node = exact_node(&fdt, path).map_err(map_binding_error)?;
        if !enabled(&node) {
            return Err(DriverError::Unsupported);
        }

        let kind = Kind::from_node(&node).ok_or(DriverError::Unsupported)?;
        let ranges = reg_ranges(node).map_err(map_binding_error)?;
        if ranges.len() != 1 {
            return Err(DriverError::InvalidRange);
        }
        let range = ranges.into_iter().next().unwrap();
        let harts = hart_ids(&fdt).map_err(map_binding_error)?;
        validate_registers(&range, kind, &harts)?;

        let canonical_qemu = model(&fdt) == QEMU_MODEL
            && range.start == QEMU_CLINT_BASE
            && range.end >= QEMU_CLINT_BASE + QEMU_CLINT_SIZE;
        if !canonical_qemu && !TRUSTED_TARGET {
            return Err(DriverError::Unauthorized);
        }

        Ok(Self { range, kind, harts })
    }
}

fn map_binding_error(error: BindingError) -> DriverError {
    match error {
        BindingError::DeviceTree => DriverError::DeviceTree,
        BindingError::Unsupported => DriverError::Unsupported,
        BindingError::InvalidRange => DriverError::InvalidRange,
    }
}

#[derive(Clone, Copy)]
enum Kind {
    SiFive,
    THead,
}

impl Kind {
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
    kind: Kind,
    harts: &[usize],
) -> Result<(), DriverError> {
    for &hart_id in harts {
        let msip_end = hart_id
            .checked_mul(4)
            .and_then(|offset| MSIP_OFFSET.checked_add(offset))
            .and_then(|offset| offset.checked_add(4))
            .and_then(|offset| range.start.checked_add(offset))
            .ok_or(DriverError::InvalidRange)?;
        let compare_end = hart_id
            .checked_mul(8)
            .and_then(|offset| MTIMECMP_OFFSET.checked_add(offset))
            .and_then(|offset| offset.checked_add(8))
            .and_then(|offset| range.start.checked_add(offset))
            .ok_or(DriverError::InvalidRange)?;
        if msip_end > range.end || compare_end > range.end {
            return Err(DriverError::InvalidRange);
        }
    }
    if matches!(kind, Kind::SiFive)
        && range
            .start
            .checked_add(MTIME_OFFSET + 8)
            .is_none_or(|end| end > range.end)
    {
        return Err(DriverError::InvalidRange);
    }
    Ok(())
}

struct Clint {
    base: usize,
    kind: Kind,
    harts: Vec<usize>,
}

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

impl TimerDevice for Clint {
    fn read_time(&self) -> u64 {
        match self.kind {
            Kind::SiFive => self.read_time_register(),
            Kind::THead => read_time_csr(),
        }
    }

    fn set_compare(&self, hart_id: usize, deadline: u64) {
        if !self.contains_hart(hart_id) {
            return;
        }
        let register = self.mtimecmp(hart_id);
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

    fn handle_interrupt(&self) -> bool {
        manifest_supervisor_timer();
        true
    }
}

impl IpiDevice for Clint {
    fn prepare_current_hart(&self) -> Result<(), IpiError> {
        let hart_id = current_hart_id();
        if !self.contains_hart(hart_id) {
            return Err(IpiError::InvalidHart);
        }
        self.claim(hart_id);
        enable_machine_software_interrupt();
        Ok(())
    }

    fn notify(&self, hart_id: usize) {
        if !self.contains_hart(hart_id) {
            return;
        }
        device_fence();
        // SAFETY: construction validates this hart's MSIP word and claims the
        // complete CLINT range for this one driver.
        unsafe { self.msip(hart_id).write_volatile(1) };
        device_fence();
    }

    fn claim(&self, hart_id: usize) {
        if !self.contains_hart(hart_id) {
            return;
        }
        // SAFETY: construction validates this hart's MSIP word and claims the
        // complete CLINT range for this one driver.
        unsafe { self.msip(hart_id).write_volatile(0) };
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
        assert!(validate_registers(&(0x200_0000..0x201_0000), Kind::SiFive, &[0, 8]).is_ok());
        assert_eq!(
            validate_registers(&(0x200_0000..0x200_4040), Kind::SiFive, &[8]),
            Err(DriverError::InvalidRange)
        );
    }

    #[test]
    fn thead_time_does_not_require_an_mtime_mmio_register() {
        assert!(validate_registers(&(0x1000..0x6000), Kind::THead, &[0]).is_ok());
        assert_eq!(
            validate_registers(&(0x1000..0x6000), Kind::SiFive, &[0]),
            Err(DriverError::InvalidRange)
        );
    }

    #[test]
    fn binding_uses_exact_node_identity_and_claims_the_range_once() {
        let mut boot = BootInfo::from_test_dtb(qemu_dtb("timer@2000000"));
        assert!(build(&mut boot, "/timer@2000000").is_ok());
        assert_eq!(
            build(&mut boot, "/timer@2000000").err(),
            Some(DriverError::AlreadyOwned)
        );
        assert_eq!(
            build(&mut boot, "/clint@2000000").err(),
            Some(DriverError::DeviceTree)
        );
    }
}
