//! Machine timer and IPI drivers backed by a CLINT.
//!
//! Compatible strings follow the pinned [SiFive CLINT binding].
//!
//! [SiFive CLINT binding]: https://github.com/torvalds/linux/blob/a500db7819c50db59e55f1b4fa1c3baa5a2616f3/Documentation/devicetree/bindings/timer/sifive%2Cclint.yaml

mod description;
pub(crate) mod ipi;
pub(super) mod sifive;
pub(super) mod thead;

use alloc::boxed::Box;
use runtime::{Error, FdtNode, PlatformView, Result, timer::TimerDevice};

use super::Description;
pub(crate) use description::Desc;

/// A boxed timer description selected for one CLINT node.
type TimerDesc = Box<dyn Description<Device = Box<dyn TimerDevice>>>;

const SIFIVE_COMPATIBLES: [&str; 3] = ["riscv,clint0", "starfive,jh7110-clint", "sifive,clint0"];
const THEAD_COMPATIBLES: [&str; 1] = ["thead,c900-clint"];

/// Returns whether a compatible string names a supported CLINT.
pub(crate) fn supports(compatible: &str) -> bool {
    SIFIVE_COMPATIBLES.contains(&compatible) || THEAD_COMPATIBLES.contains(&compatible)
}

/// Parses a CLINT node, following its compatible-string order.
///
/// The timer backend follows the first supported compatible string.
/// Software IPIs use the shared `msip` window for every layout.
pub(crate) fn describe(platform: &PlatformView<'_>, node: FdtNode<'_, '_>) -> Result<Desc> {
    let registers = platform.device_register(node)?.ok_or(Error::InvalidArgs)?;
    let timer = node
        .compatible()
        .and_then(|compatibles| {
            compatibles.all().find_map(|compatible| match compatible {
                compatible if SIFIVE_COMPATIBLES.contains(&compatible) => {
                    Some(Box::new(sifive::Desc { registers }) as TimerDesc)
                }
                compatible if THEAD_COMPATIBLES.contains(&compatible) => {
                    Some(Box::new(thead::Desc { registers }) as TimerDesc)
                }
                _ => None,
            })
        })
        .ok_or(Error::InvalidArgs)?;
    Ok(Desc {
        ipi: ipi::Desc { registers },
        timer,
    })
}
