//! Private concrete bindings for machine-owned physical devices.

use alloc::vec::Vec;

use crate::boot::device_tree::BindingError;
use crate::boot::{BootInfo, MachineRangeError};
use crate::hart::HartRuntime;
use crate::{HartControl, Ipi, RemoteFence, Timer};

mod aplic;
pub mod clint;
mod imsic;
pub mod sifive_test;
mod sstc;
pub mod uart;

/// Failure while validating or constructing a concrete machine device.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DriverError {
    /// The owned boot tree is malformed or an exact selected node is absent.
    DeviceTree,
    /// The selected nodes do not describe the retained concrete configuration.
    Unsupported,
    /// A single device register range is malformed or too short.
    InvalidRange,
    /// Cross-device ranges, hart files, identities, or topology fields are invalid.
    InvalidTopology,
    /// The description is not the authorized QEMU virt configuration.
    Unauthorized,
    /// Another live machine resource already owns a required register range.
    AlreadyOwned,
    /// A required machine APLIC configuration register is locked.
    Locked,
    /// A required device or CSR write did not read back as requested.
    Readback,
    /// A required hart-local architectural facility is unavailable.
    Hardware,
}

/// Validates and binds the retained QEMU Sstc/IMSIC/M-APLIC configuration.
///
/// Both node paths are exact identities retained by safe platform discovery.
/// The function publishes neither capability until the complete topology is
/// validated, all machine-only ranges are claimed, and the routing protocol
/// succeeds.
pub fn build_aia(
    boot: &mut BootInfo,
    imsic_path: &str,
    aplic_path: &str,
) -> Result<(Timer, Ipi, RemoteFence, HartControl), DriverError> {
    let imsic = imsic::ImsicLayout::from_dtb(boot, imsic_path).map_err(map_imsic_error)?;
    let aplic = aplic::Binding::from_dtb(boot, aplic_path).map_err(map_aplic_error)?;
    boot.ensure_runtime_unbound()
        .map_err(|_| DriverError::AlreadyOwned)?;

    let mut ranges = Vec::with_capacity(imsic.register_ranges.len() + 1);
    ranges.extend(imsic.register_ranges.iter().cloned());
    ranges.push(aplic.range.clone());
    boot.claim_machine_ranges(&ranges)
        .map_err(|error| match error {
            MachineRangeError::Invalid => DriverError::InvalidTopology,
            MachineRangeError::AlreadyClaimed => DriverError::AlreadyOwned,
        })?;

    let machine_base = imsic
        .register_ranges
        .first()
        .ok_or(DriverError::InvalidTopology)?
        .start;
    aplic
        .configure(machine_base, imsic.hart_index_width)
        .map_err(map_aplic_error)?;
    let timer = sstc::build(imsic.hart_ids()).map_err(|_| DriverError::Hardware)?;
    let (device, harts) = imsic.into_device();
    let wake_by_ipi = alloc::vec![true; harts.len()];
    let runtime = HartRuntime::new(device, &harts, boot.init_hart_id(), &wake_by_ipi)
        .map_err(|_| DriverError::Hardware)?;
    boot.install_runtime(runtime.clone(), timer.trap_device())
        .map_err(|_| DriverError::AlreadyOwned)?;
    Ok((
        timer,
        Ipi::new(runtime.clone()),
        RemoteFence::new(runtime.clone()),
        HartControl::new(runtime),
    ))
}

fn map_imsic_error(error: imsic::ImsicError) -> DriverError {
    match error {
        imsic::ImsicError::Binding(BindingError::DeviceTree) => DriverError::DeviceTree,
        imsic::ImsicError::Binding(BindingError::Unsupported) => DriverError::Unsupported,
        imsic::ImsicError::Binding(BindingError::InvalidRange)
        | imsic::ImsicError::InvalidTopology => DriverError::InvalidTopology,
        imsic::ImsicError::Unauthorized => DriverError::Unauthorized,
        imsic::ImsicError::Hardware => DriverError::Hardware,
    }
}

fn map_aplic_error(error: aplic::AplicError) -> DriverError {
    match error {
        aplic::AplicError::Binding(BindingError::DeviceTree) => DriverError::DeviceTree,
        aplic::AplicError::Binding(BindingError::Unsupported) => DriverError::Unsupported,
        aplic::AplicError::Binding(BindingError::InvalidRange)
        | aplic::AplicError::InvalidConfiguration => DriverError::InvalidTopology,
        aplic::AplicError::Unauthorized => DriverError::Unauthorized,
        aplic::AplicError::Locked => DriverError::Locked,
        aplic::AplicError::Readback => DriverError::Readback,
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec::Vec;

    use dtoolkit::model::{DeviceTree, DeviceTreeNode, DeviceTreeProperty};

    use super::*;

    fn qemu_aia_dtb(aplic_sources: u32) -> Vec<u8> {
        let mut tree = DeviceTree::new();
        tree.root.add_property(
            DeviceTreeProperty::new("model", b"riscv-virtio,qemu\0".to_vec()).unwrap(),
        );
        tree.root
            .add_property(DeviceTreeProperty::new("#address-cells", 2u32.to_be_bytes()).unwrap());
        tree.root
            .add_property(DeviceTreeProperty::new("#size-cells", 2u32.to_be_bytes()).unwrap());

        let intc = DeviceTreeNode::builder("interrupt-controller")
            .unwrap()
            .property(DeviceTreeProperty::new("compatible", b"riscv,cpu-intc\0".to_vec()).unwrap())
            .property(DeviceTreeProperty::new("phandle", 1u32.to_be_bytes()).unwrap())
            .build();
        let cpu = DeviceTreeNode::builder("cpu@0")
            .unwrap()
            .property(DeviceTreeProperty::new("reg", 0u32.to_be_bytes()).unwrap())
            .child(intc)
            .build();
        let cpus = DeviceTreeNode::builder("cpus")
            .unwrap()
            .property(DeviceTreeProperty::new("#address-cells", 1u32.to_be_bytes()).unwrap())
            .property(DeviceTreeProperty::new("#size-cells", 0u32.to_be_bytes()).unwrap())
            .child(cpu)
            .build();
        tree.root.add_child(cpus);

        let imsic = DeviceTreeNode::builder("imsics@24000000")
            .unwrap()
            .property(DeviceTreeProperty::new("compatible", b"riscv,imsics\0".to_vec()).unwrap())
            .property(DeviceTreeProperty::new("reg", reg(0x2400_0000, 0x1000)).unwrap())
            .property(DeviceTreeProperty::new("riscv,num-ids", 255u32.to_be_bytes()).unwrap())
            .property(DeviceTreeProperty::new("interrupts-extended", cells(&[1, 11])).unwrap())
            .build();
        let aplic = DeviceTreeNode::builder("aplic@c000000")
            .unwrap()
            .property(DeviceTreeProperty::new("compatible", b"riscv,aplic\0".to_vec()).unwrap())
            .property(DeviceTreeProperty::new("reg", reg(0x0c00_0000, 0x4000)).unwrap())
            .property(
                DeviceTreeProperty::new("riscv,num-sources", aplic_sources.to_be_bytes()).unwrap(),
            )
            .property(DeviceTreeProperty::new("riscv,children", 2u32.to_be_bytes()).unwrap())
            .build();
        let serial = DeviceTreeNode::builder("serial@10000000")
            .unwrap()
            .property(DeviceTreeProperty::new("compatible", b"ns16550a\0".to_vec()).unwrap())
            .property(DeviceTreeProperty::new("reg", reg(0x1000_0000, 0x100)).unwrap())
            .build();
        let power = DeviceTreeNode::builder("test@100000")
            .unwrap()
            .property(
                DeviceTreeProperty::new(
                    "compatible",
                    b"sifive,test1\0sifive,test0\0syscon\0".to_vec(),
                )
                .unwrap(),
            )
            .property(DeviceTreeProperty::new("reg", reg(0x0010_0000, 0x1000)).unwrap())
            .build();
        let soc = DeviceTreeNode::builder("soc")
            .unwrap()
            .property(DeviceTreeProperty::new("#address-cells", 2u32.to_be_bytes()).unwrap())
            .property(DeviceTreeProperty::new("#size-cells", 2u32.to_be_bytes()).unwrap())
            .property(DeviceTreeProperty::new("ranges", Vec::new()).unwrap())
            .child(imsic)
            .child(aplic)
            .child(serial)
            .child(power)
            .build();
        tree.root.add_child(soc);
        tree.to_dtb()
    }

    fn reg(start: u32, size: u32) -> Vec<u8> {
        cells(&[0, start, 0, size])
    }

    fn cells(values: &[u32]) -> Vec<u8> {
        values
            .iter()
            .flat_map(|value| value.to_be_bytes())
            .collect()
    }

    #[test]
    fn complete_qemu_aia_binding_is_atomic_and_uses_exact_nested_paths() {
        let mut malformed = BootInfo::from_test_dtb(qemu_aia_dtb(95));
        assert_eq!(
            build_aia(&mut malformed, "/soc/imsics@24000000", "/soc/aplic@c000000").err(),
            Some(DriverError::Unauthorized)
        );

        let mut boot = BootInfo::from_test_dtb(qemu_aia_dtb(96));
        assert!(build_aia(&mut boot, "/soc/imsics@24000000", "/soc/aplic@c000000").is_ok());
        assert_eq!(
            build_aia(&mut boot, "/soc/imsics@24000000", "/soc/aplic@c000000").err(),
            Some(DriverError::AlreadyOwned)
        );
        assert_eq!(
            build_aia(
                &mut BootInfo::from_test_dtb(qemu_aia_dtb(96)),
                "/imsics@24000000",
                "/soc/aplic@c000000"
            )
            .err(),
            Some(DriverError::DeviceTree)
        );
    }

    #[test]
    fn console_and_power_bind_the_same_exact_nested_qemu_identity() {
        let mut boot = BootInfo::from_test_dtb(qemu_aia_dtb(96));
        assert!(uart::build(&mut boot, "/soc/serial@10000000").is_ok());
        assert!(sifive_test::build(&mut boot, "/soc/test@100000").is_ok());
        assert_eq!(
            uart::build(&mut boot, "/serial@10000000").err(),
            Some(DriverError::DeviceTree)
        );
        assert_eq!(
            sifive_test::build(&mut boot, "/soc/test@100000").err(),
            Some(DriverError::AlreadyOwned)
        );
    }
}
