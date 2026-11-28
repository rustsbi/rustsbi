//! Sunxi RTC V203 vendor register descriptions.

use fdt::node::FdtNode;

use super::{PlatformView, node_is_enabled};
use crate::memory::{PhysAddr, RegRange};
use crate::{Error, Result};

impl<'tree> PlatformView<'tree> {
    /// Returns the GPRCM register window of an enabled RTC V203 node.
    ///
    /// The vendor's `gprcm_reg` property contains a big-endian 64-bit address
    /// followed by a big-endian 64-bit size, independent of the parent's cell
    /// counts. A missing property or a nonmatching node yields `Ok(None)`.
    pub fn sunxi_rtc_v203_gprcm(&self, node: FdtNode<'_, 'tree>) -> Result<Option<RegRange>> {
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
        self.check_property_storage(property.value)?;
        let (start, len) = decode_gprcm(property.value)?;
        RegRange::from_start_len(PhysAddr::new(start), len).map(Some)
    }
}

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
