//! Hart and model discovery.

use alloc::{format, string::ToString, vec::Vec};

use riscv::register::mstatus::MPP;
use runtime::node_is_enabled;

use crate::devicetree::{is_cpu_node, u32_property};
use crate::platform::info::BoardInfo;
use crate::sbi::features::detect_extensions;

use super::imsic::{self, CpuInterruptController};

/// Reads hart topology and features, retaining CPU interrupt wiring for IMSIC discovery.
pub(super) fn discover(
    board: &mut BoardInfo,
    platform: &runtime::PlatformView<'_>,
    next_mode: Option<MPP>,
) -> runtime::Result<Vec<CpuInterruptController>> {
    let root = platform.root();
    let cpus = platform
        .find_enabled_node("/cpus")
        .ok_or(runtime::Error::InvalidArgs)?;
    board.harts.timebase_frequency_hz =
        u32_property(cpus, "timebase-frequency").filter(|frequency| *frequency != 0);
    board.model = root
        .property("model")
        .and_then(|property| property.as_str())
        .unwrap_or("<unspecified>")
        .to_string();

    let mut controllers = Vec::new();
    for node in cpus.children().filter(|node| is_cpu_node(*node)) {
        if !node_is_enabled(node) {
            continue;
        }
        let hart_id = node
            .reg()
            .and_then(|mut registers| registers.next())
            .map(|register| register.starting_address as usize)
            .ok_or(runtime::Error::InvalidArgs)?;
        if board.harts.enabled.get(hart_id).is_none() {
            return Err(runtime::Error::InvalidArgs);
        }
        if !cpu_is_usable_by_next_stage(node, next_mode) {
            board
                .harts
                .disabled_cpu_paths
                .push(format!("/cpus/{}", node.name));
            continue;
        }
        let enabled = board
            .harts
            .enabled
            .get_mut(hart_id)
            .expect("hart ID was checked against Runtime capacity");
        *enabled = true;
        board.harts.count += 1;
        detect_extensions(hart_id, node);
        controllers.extend(
            node.children()
                .filter_map(|child| imsic::cpu_interrupt_controller(child, hart_id)),
        );
    }

    Ok(controllers)
}

/// Returns whether a CPU node can host the requested next stage.
///
/// An S-mode operating system needs a usable MMU description; `riscv,none`
/// explicitly describes a hart without one.
fn cpu_is_usable_by_next_stage(node: runtime::FdtNode<'_, '_>, next_mode: Option<MPP>) -> bool {
    next_mode != Some(MPP::Supervisor)
        || node
            .property("mmu-type")
            .and_then(|property| property.as_str())
            .is_some_and(|mmu_type| !mmu_type.is_empty() && mmu_type != "riscv,none")
}
