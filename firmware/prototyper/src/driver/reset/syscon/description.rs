//! Devicetree description of one syscon reset action.

use core::mem::{align_of, size_of};

use runtime::memory::DeviceRegisterRange;
use runtime::{Error, Result};
use serde_device_tree::buildin::{Node, StrSeq};

use super::super::registry;

/// One validated syscon action that has not acquired its MMIO word yet.
#[derive(Clone, Copy, Debug)]
pub(super) struct ActionDescription {
    registers: DeviceRegisterRange,
    value: u32,
    mask: u32,
    priority: u32,
}

impl ActionDescription {
    pub(super) fn from_node<'tree>(
        platform: &runtime::PlatformView<'tree>,
        node: &Node<'tree>,
        parent: Option<&Node<'tree>>,
    ) -> Result<Self> {
        let provider = Self::resolve_provider(platform.root(), node, parent)?;
        Self::validate_provider(&provider)?;

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
            registry::primary_registers(platform, &provider)?.subrange(offset, size_of::<u32>())?;
        if !registers.start().is_aligned_to(align_of::<u32>()) {
            return Err(Error::InvalidArgs);
        }
        Ok(Self {
            registers,
            value,
            mask,
            priority: Self::read_u32(node, "priority")?.unwrap_or(192),
        })
    }

    pub(super) const fn priority(self) -> u32 {
        self.priority
    }

    pub(super) const fn into_parts(self) -> (DeviceRegisterRange, u32, u32) {
        (self.registers, self.value, self.mask)
    }

    fn value_and_mask(value: Option<u32>, mask: Option<u32>) -> Result<(u32, u32)> {
        match (value, mask) {
            (Some(value), Some(mask)) => Ok((value, mask)),
            (Some(value), None) | (None, Some(value)) => Ok((value, u32::MAX)),
            (None, None) => Err(Error::InvalidArgs),
        }
    }

    fn read_u32(node: &Node<'_>, name: &str) -> Result<Option<u32>> {
        node.get_prop(name)
            .map(|property| {
                let bytes = property.deserialize::<&[u8]>();
                let bytes = bytes.try_into().map_err(|_| Error::InvalidArgs)?;
                Ok(u32::from_be_bytes(bytes))
            })
            .transpose()
    }

    fn resolve_provider<'tree>(
        root: &Node<'tree>,
        node: &Node<'tree>,
        parent: Option<&Node<'tree>>,
    ) -> Result<Node<'tree>> {
        if let Some(phandle) = Self::read_u32(node, "regmap")? {
            if phandle == 0 || phandle == u32::MAX {
                return Err(Error::InvalidArgs);
            }
            Self::find_phandle(root, phandle)?.ok_or(Error::InvalidArgs)
        } else {
            parent.cloned().ok_or(Error::InvalidArgs)
        }
    }

    fn find_phandle<'tree>(node: &Node<'tree>, phandle: u32) -> Result<Option<Node<'tree>>> {
        // A disabled ancestor also makes a referenced provider unavailable.
        if !runtime::node_is_enabled(node) {
            return Ok(None);
        }
        let handle = Self::read_u32(node, "phandle")?.or(Self::read_u32(node, "linux,phandle")?);
        if handle == Some(phandle) {
            return Ok(Some(node.clone()));
        }
        for child in node.nodes() {
            let child = child.deserialize::<Node<'tree>>();
            if let Some(provider) = Self::find_phandle(&child, phandle)? {
                return Ok(Some(provider));
            }
        }
        Ok(None)
    }

    fn validate_provider(node: &Node<'_>) -> Result<()> {
        let is_syscon = node.get_prop("compatible").is_some_and(|property| {
            property
                .deserialize::<StrSeq>()
                .iter()
                .any(|value| value == "syscon")
        });
        if !runtime::node_is_enabled(node)
            || !is_syscon
            || Self::read_u32(node, "reg-io-width")?.is_some_and(|width| width != 4)
            || node.get_prop("big-endian").is_some()
        {
            return Err(Error::InvalidArgs);
        }
        Ok(())
    }
}
