//! Devicetree description of one syscon reset action.

use core::mem::{align_of, size_of};

use runtime::{Error, FdtNode, PlatformView, Result, memory::RegRange};

use super::super::primary_registers;
use crate::devicetree::is_compatible;

/// Selected reset actions before their MMIO registers are acquired.
pub(crate) struct Desc {
    pub(super) poweroff: Option<ActionDesc>,
    pub(super) reboot: Option<ActionDesc>,
}

/// One validated syscon action whose MMIO word has not been acquired yet.
#[derive(Clone, Copy, Debug)]
pub(super) struct ActionDesc {
    pub(super) registers: RegRange,
    pub(super) value: u32,
    pub(super) mask: u32,
    pub(super) priority: u32,
}

impl ActionDesc {
    pub(super) fn from_node<'view, 'tree>(
        platform: &'view PlatformView<'tree>,
        node: FdtNode<'view, 'tree>,
        parent: Option<FdtNode<'view, 'tree>>,
    ) -> Result<Self> {
        let provider = Self::resolve_provider(platform, node, parent)?;
        Self::validate_provider(provider)?;

        let offset = Self::read_u32(node, "offset")?.ok_or(Error::InvalidArgs)?;
        let offset = usize::try_from(offset).map_err(|_| Error::Overflow)?;
        if !offset.is_multiple_of(size_of::<u32>()) {
            return Err(Error::InvalidArgs);
        }
        let (value, mask) = Self::value_and_mask(
            Self::read_u32(node, "value")?,
            Self::read_u32(node, "mask")?,
        )?;
        let registers =
            primary_registers(platform, provider)?.subrange(offset, size_of::<u32>())?;
        if !registers.start().is_aligned_to(align_of::<u32>()) {
            return Err(Error::InvalidArgs);
        }
        Ok(Self {
            registers,
            value,
            mask,
            // The syscon reboot binding defaults `priority` to 192.
            priority: Self::read_u32(node, "priority")?.unwrap_or(192),
        })
    }

    fn value_and_mask(value: Option<u32>, mask: Option<u32>) -> Result<(u32, u32)> {
        // If only one of `value` and `mask` is present,
        // use it as the value for a full-word write.
        match (value, mask) {
            (Some(value), Some(mask)) => Ok((value, mask)),
            (Some(value), None) | (None, Some(value)) => Ok((value, u32::MAX)),
            (None, None) => Err(Error::InvalidArgs),
        }
    }

    fn read_u32(node: FdtNode<'_, '_>, name: &str) -> Result<Option<u32>> {
        node.property(name)
            .map(|property| {
                let bytes = property.value.try_into().map_err(|_| Error::InvalidArgs)?;
                Ok(u32::from_be_bytes(bytes))
            })
            .transpose()
    }

    fn resolve_provider<'view, 'tree>(
        platform: &'view PlatformView<'tree>,
        node: FdtNode<'view, 'tree>,
        parent: Option<FdtNode<'view, 'tree>>,
    ) -> Result<FdtNode<'view, 'tree>> {
        // Without `regmap`, the binding uses the parent node
        // as the syscon provider.
        match Self::read_u32(node, "regmap")? {
            Some(0 | u32::MAX) => Err(Error::InvalidArgs),
            Some(phandle) => {
                let mut provider = None;
                platform.try_for_each_enabled_node(|candidate, _, _| {
                    if provider.is_none() {
                        let handle = Self::read_u32(candidate, "phandle")?
                            .or(Self::read_u32(candidate, "linux,phandle")?);
                        if handle == Some(phandle) {
                            provider = Some(candidate);
                        }
                    }
                    Ok(())
                })?;
                provider.ok_or(Error::InvalidArgs)
            }
            None => parent.ok_or(Error::InvalidArgs),
        }
    }

    fn validate_provider(node: FdtNode<'_, '_>) -> Result<()> {
        // The driver supports only 32-bit little-endian syscon registers.
        if !is_compatible(node, "syscon")
            || Self::read_u32(node, "reg-io-width")?.is_some_and(|width| width != 4)
            || node.property("big-endian").is_some()
        {
            return Err(Error::InvalidArgs);
        }
        Ok(())
    }
}
