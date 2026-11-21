//! RFENCE currently supports native-width supervisor and guest contexts only.
//!
//! `hgatp` is laid out by HSXLEN, and `vsatp`/guest ASIDs by VSXLEN, not
//! firmware pointer width (Privileged ISA sections 3.1.6.3, 5.2.1 and 5.2.10).
//! Reject mixed widths before interpreting those fields or issuing a fence.

use super::FenceOperation;
use crate::{Error, Result};
use csr::values::xlen::Xlen;

pub(super) fn check(operation: FenceOperation) -> Result<()> {
    #[cfg(target_pointer_width = "64")]
    {
        check_with(
            Xlen::X64,
            operation,
            || {
                let status = crate::csr::csrr!(csr::machine::Status)?;
                Ok(csr::machine::Status::from_bits(status).get::<csr::machine::SXL>())
            },
            || {
                #[cfg(feature = "hypervisor")]
                {
                    let status = crate::csr::csrr!(csr::hypervisor::Status)
                        .map_err(|_| Error::NotSupported)?;
                    Ok(csr::hypervisor::Status::from_bits(status).get::<csr::hypervisor::VSXL>())
                }
                #[cfg(not(feature = "hypervisor"))]
                Err(Error::NotSupported)
            },
        )
    }
    #[cfg(target_pointer_width = "32")]
    {
        // SXL and VSXL do not exist; SXLEN and VSXLEN are necessarily 32.
        check_with(
            Xlen::X32,
            operation,
            || Err(Error::NotSupported),
            || Err(Error::NotSupported),
        )
    }
}

fn check_with(
    native: Xlen,
    operation: FenceOperation,
    supervisor: impl FnOnce() -> Result<Option<Xlen>>,
    guest: impl FnOnce() -> Result<Option<Xlen>>,
) -> Result<()> {
    if operation == FenceOperation::FenceI || native == Xlen::X32 {
        return Ok(());
    }
    if supervisor()? != Some(native) {
        return Err(Error::NotSupported);
    }
    #[cfg(feature = "hypervisor")]
    if matches!(
        operation,
        FenceOperation::HFenceVvma | FenceOperation::HFenceVvmaAsid { .. }
    ) && guest()? != Some(native)
    {
        return Err(Error::NotSupported);
    }
    #[cfg(not(feature = "hypervisor"))]
    let _ = guest;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_read() -> Result<Option<Xlen>> {
        panic!("unrelated or absent width field must not be read")
    }

    fn operations() -> &'static [FenceOperation] {
        &[
            FenceOperation::SFenceVma,
            FenceOperation::SFenceVmaAsid { asid: 0 },
            #[cfg(feature = "hypervisor")]
            FenceOperation::HFenceGvma,
            #[cfg(feature = "hypervisor")]
            FenceOperation::HFenceGvmaVmid { vmid: 0 },
            #[cfg(feature = "hypervisor")]
            FenceOperation::HFenceVvma,
            #[cfg(feature = "hypervisor")]
            FenceOperation::HFenceVvmaAsid { asid: 0 },
        ]
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn status_fields_decode_supervisor_and_guest_widths_separately() {
        #[cfg(feature = "hypervisor")]
        use csr::hypervisor;
        use csr::machine;
        for (encoding, width) in [
            (0, None),
            (1, Some(Xlen::X32)),
            (2, Some(Xlen::X64)),
            (3, Some(Xlen::X128)),
        ] {
            assert_eq!(
                machine::Status::from_bits(encoding << 34).get::<machine::SXL>(),
                width
            );
            #[cfg(feature = "hypervisor")]
            assert_eq!(
                hypervisor::Status::from_bits(encoding << 32).get::<hypervisor::VSXL>(),
                width
            );
        }
    }

    #[test]
    fn rv32_has_no_width_fields_and_fence_i_does_not_need_them() {
        for &operation in operations() {
            assert_eq!(check_with(Xlen::X32, operation, no_read, no_read), Ok(()));
        }
        assert_eq!(
            check_with(Xlen::X64, FenceOperation::FenceI, no_read, no_read),
            Ok(())
        );
    }

    #[test]
    fn rv64_rejects_non_native_supervisor_before_reading_guest_state() {
        for &operation in operations() {
            for width in [None, Some(Xlen::X32), Some(Xlen::X128)] {
                assert_eq!(
                    check_with(Xlen::X64, operation, || Ok(width), no_read),
                    Err(Error::NotSupported)
                );
            }
            assert_eq!(
                check_with(Xlen::X64, operation, || Err(Error::NotSupported), no_read),
                Err(Error::NotSupported)
            );
        }
    }

    #[test]
    fn supervisor_fences_do_not_require_native_guest_width() {
        for operation in [
            FenceOperation::SFenceVma,
            FenceOperation::SFenceVmaAsid { asid: usize::MAX },
            #[cfg(feature = "hypervisor")]
            FenceOperation::HFenceGvma,
            #[cfg(feature = "hypervisor")]
            FenceOperation::HFenceGvmaVmid { vmid: usize::MAX },
        ] {
            assert_eq!(
                check_with(Xlen::X64, operation, || Ok(Some(Xlen::X64)), no_read),
                Ok(())
            );
        }
    }

    #[cfg(feature = "hypervisor")]
    #[test]
    fn vvma_requires_native_vsxl_and_propagates_read_failures() {
        for operation in [
            FenceOperation::HFenceVvma,
            FenceOperation::HFenceVvmaAsid { asid: 0 },
        ] {
            for width in [None, Some(Xlen::X32), Some(Xlen::X64), Some(Xlen::X128)] {
                assert_eq!(
                    check_with(Xlen::X64, operation, || Ok(Some(Xlen::X64)), || Ok(width)),
                    if width == Some(Xlen::X64) {
                        Ok(())
                    } else {
                        Err(Error::NotSupported)
                    }
                );
            }
            assert_eq!(
                check_with(
                    Xlen::X64,
                    operation,
                    || Ok(Some(Xlen::X64)),
                    || Err(Error::NotSupported)
                ),
                Err(Error::NotSupported)
            );
        }
    }
}
