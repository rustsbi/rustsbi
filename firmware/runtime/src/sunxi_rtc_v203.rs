//! Sunxi RTC V203 vendor register descriptions.

use fdt::node::FdtNode;

use crate::memory::{DeviceRegisterRange, PhysAddr, PhysAddrRange};
use crate::{Error, Result, node_is_enabled};

/// Decodes the absolute GPRCM address and size independently of parent cells.
pub(crate) fn gprcm_registers(
    node: FdtNode<'_, '_>,
    fdt_storage: PhysAddrRange,
) -> Result<Option<DeviceRegisterRange>> {
    if !node_is_enabled(node)
        || !node
            .compatible()
            .is_some_and(|values| values.all().any(|value| value == "allwinner,rtc-v203"))
    {
        return Ok(None);
    }
    let Some(property) = node.property("gprcm_reg") else {
        return Ok(None);
    };
    let bytes = property.value;
    let storage =
        PhysAddrRange::from_start_len(PhysAddr::new(bytes.as_ptr() as usize), bytes.len())?;
    if !fdt_storage.contains(storage) {
        return Err(Error::AccessDenied);
    }
    let (start, len) = decode_gprcm(bytes)?;
    PhysAddrRange::from_start_len(PhysAddr::new(start), len)
        .map(DeviceRegisterRange::from_description)
        .map(Some)
}

// The vendor RTC binding fixes this encoding independently of parent cell counts.
fn decode_gprcm(bytes: &[u8]) -> Result<(usize, usize)> {
    if bytes.len() != 16 {
        return Err(Error::InvalidArgs);
    }
    let start = u64::from_be_bytes(bytes[..8].try_into().map_err(|_| Error::InvalidArgs)?);
    let len = u64::from_be_bytes(bytes[8..].try_into().map_err(|_| Error::InvalidArgs)?);
    Ok((
        usize::try_from(start).map_err(|_| Error::InvalidArgs)?,
        usize::try_from(len).map_err(|_| Error::InvalidArgs)?,
    ))
}

#[cfg(test)]
mod gprcm_tests {
    use super::*;
    #[test]
    fn vendor_property_uses_fixed_big_endian_address_and_size() {
        let bytes = [0, 0, 0, 0, 0x4a, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0x20];
        assert_eq!(decode_gprcm(&bytes).unwrap(), (0x4a00_0000, 0x20));
        assert!(decode_gprcm(&bytes[..12]).is_err());
        assert!(decode_gprcm(&[0; 20]).is_err());
    }
}
