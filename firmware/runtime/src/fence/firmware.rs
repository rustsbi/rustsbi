//! Current-hart fence execution.

use csr::{
    Read, Write,
    standard::isa::{
        privileged::{
            hypervisor::{GuestAddressSpace, GuestPagingMode, Hgatp},
            machine::Mstatus,
            virtual_supervisor::Vsatp,
        },
        values::address::AddressSpace,
    },
};

use super::{LocalFence, Xlen, translation};
use crate::{Error, Result, hart, instruction, irq};

/// Returns the VMID of the current hart's active guest address space.
///
/// Call from M-mode. Returns [`Error::NotSupported`] if H is unavailable or
/// the supervisor register width cannot be determined.
pub fn current_guest_vmid() -> Result<usize> {
    require_hypervisor()?;
    let hsxlen = hart::supervisor_xlen().ok_or(Error::NotSupported)?;
    Hgatp::read().vmid_for(hsxlen).ok_or(Error::NotSupported)
}

/// Executes the requested fence on the current hart.
///
/// Run in M-mode with `MPRV` clear.
/// Supply ASIDs and VMIDs within the target context's architectural width.
/// A Bare-only hart may reject `SFENCE.VMA`. Recovery completes the request
/// and overwrites live machine trap metadata.
/// A guest virtual fence uses the implemented VMID bits on the target hart.
/// During a temporary VMID switch, it keeps local interrupts masked until
/// `hgatp` and `vsatp` have been restored.
///
/// Returns [`Error::InvalidArgs`] for a nonempty page range with zero stride,
/// [`Error::NotReady`] for a nonempty supervisor fence before trap recovery is
/// ready or for a guest virtual fence with `MPRV` set, and [`Error::NotSupported`]
/// when H is unavailable or the requested guest context cannot be selected.
pub fn execute_local(fence: LocalFence) -> Result<()> {
    match fence {
        LocalFence::Instruction => instruction::fence_i(),
        LocalFence::Supervisor { range, asid } => fence_supervisor(range.pages()?, asid)?,
        LocalFence::GuestPhysical { range, vmid } => {
            let addresses = range.pages()?;
            require_hypervisor()?;
            for_each_address(addresses, |address| {
                translation::hfence_gvma(address, vmid);
            });
        }
        LocalFence::GuestVirtual { range, asid, vmid } => {
            let addresses = range.pages()?;
            require_hypervisor()?;
            let hsxlen = hart::supervisor_xlen().ok_or(Error::NotSupported)?;
            irq::with_local_disabled(|irq| {
                if Mstatus::read().get(Mstatus::MPRV) {
                    return Err(Error::NotReady);
                }
                // SAFETY: H is available and MPRV is clear under local IRQ exclusion.
                unsafe {
                    with_guest_vmid(irq, vmid, hsxlen, || {
                        for_each_address(addresses, |address| {
                            translation::hfence_vvma(address, asid);
                        });
                    })
                }
            })?;
        }
    }
    Ok(())
}

fn fence_supervisor(
    mut pages: Option<impl Iterator<Item = usize>>,
    asid: Option<usize>,
) -> Result<()> {
    let first = match &mut pages {
        None => None,
        Some(pages) => match pages.next() {
            Some(page) => Some(page),
            None => return Ok(()),
        },
    };
    let supported = irq::with_local_disabled(|_| {
        if !crate::recovery::is_ready() {
            return Err(Error::NotReady);
        }
        // SAFETY: The Runtime trap entry, MPRV and mscratch were checked with IRQs masked.
        Ok(unsafe { translation::try_sfence_vma(first, asid) })
    })?;
    if supported && let Some(pages) = pages {
        for page in pages {
            translation::sfence_vma_page(page, asid);
        }
    }
    Ok(())
}

fn for_each_address(
    addresses: Option<impl Iterator<Item = usize>>,
    mut for_each_fn: impl FnMut(Option<usize>),
) {
    match addresses {
        Some(addresses) => addresses.map(Some).for_each(for_each_fn),
        None => for_each_fn(None),
    }
}

fn require_hypervisor() -> Result<()> {
    if hart::has_hypervisor() {
        Ok(())
    } else {
        Err(Error::NotSupported)
    }
}

/// Executes `fence_fn` with `vmid` selected in `hgatp` under `hsxlen`.
///
/// # Safety
///
/// Call in M-mode with `MPRV` clear on a hart with H, with local interrupts
/// disabled and `hsxlen` matching the hart's supervisor register width.
/// The callback must only fence translations and must return normally so the
/// original address spaces can be restored.
unsafe fn with_guest_vmid(
    _irq: &irq::LocalIrqDisabled,
    vmid: usize,
    hsxlen: Xlen,
    fence_fn: impl FnOnce(),
) -> Result<()> {
    let saved_hgatp = Hgatp::read();
    let saved_mode = saved_hgatp.mode_for(hsxlen).ok_or(Error::NotSupported)?;
    let max_vmid = GuestAddressSpace::max_vmid_for(hsxlen);
    if vmid > max_vmid {
        return Err(Error::NotSupported);
    }
    if saved_hgatp.vmid_for(hsxlen) == Some(vmid) {
        fence_fn();
        return Ok(());
    }
    let saved_vsatp = Vsatp::read();
    // SAFETY: Local interrupts are excluded and MPRV is clear. Bare vsatp
    // prevents speculative G-stage walks using the old guest's page table.
    unsafe {
        Vsatp::write(AddressSpace::bare());
    }
    // SAFETY: VS translation stays inactive throughout selection and restoration.
    let selected = unsafe { select_fence_vmid(saved_hgatp, saved_mode, hsxlen, vmid) };
    if selected {
        fence_fn();
    }
    // SAFETY: No guest memory access can occur until the original G-stage
    // context is restored and synchronized. Only then is VS translation restored.
    unsafe {
        select_guest_context(saved_hgatp);
        Vsatp::write(saved_vsatp);
    }
    selected.then_some(()).ok_or(Error::NotSupported)
}

/// Selects a paged VMID or the sole zero context of a Bare-only hart.
///
/// # Safety
///
/// Keep VS translation inactive under the exclusion required by `with_guest_vmid`.
/// Restore both address spaces after this operation, including on failure.
unsafe fn select_fence_vmid(
    saved: GuestAddressSpace,
    saved_mode: u8,
    hsxlen: Xlen,
    vmid: usize,
) -> bool {
    let modes: &[GuestPagingMode] = match hsxlen {
        Xlen::X32 => &[GuestPagingMode::Sv32x4],
        Xlen::X64 => &[
            GuestPagingMode::Sv39x4,
            GuestPagingMode::Sv48x4,
            GuestPagingMode::Sv57x4,
        ],
    };
    let mut bare_only = saved_mode == 0 && saved.vmid_for(hsxlen) == Some(0);
    for &mode in modes {
        let base = if saved_mode == mode as u8 {
            saved
        } else if saved_mode == 0 {
            // No guest translations run while probing paging modes with root PPN zero.
            GuestAddressSpace::paged(mode, 0).expect("mode matches HSXLEN")
        } else {
            continue;
        };
        let select = |id| {
            let context = base.with_vmid(id, hsxlen).expect("validated paged VMID");
            // SAFETY: The caller keeps VS translation inactive across these writes.
            unsafe { select_guest_context(context) }
        };
        let read_vmid = |value: GuestAddressSpace| {
            if value.mode_for(hsxlen) == Some(mode as u8) {
                value.vmid_for(hsxlen)
            } else {
                None
            }
        };
        if saved_mode != 0 && read_vmid(select(vmid)) == Some(vmid) {
            return true;
        }
        let selected = select(0);
        bare_only &= selected.mode_for(hsxlen) == Some(0) && selected.vmid_for(hsxlen) == Some(0);
        if read_vmid(selected) != Some(0) {
            continue;
        }
        // Implemented VMID bits form a contiguous low-order mask.
        let Some(mask) = read_vmid(select(GuestAddressSpace::max_vmid_for(hsxlen))) else {
            continue;
        };
        let effective_vmid = vmid & mask;
        if mask & (mask + 1) == 0 && read_vmid(select(effective_vmid)) == Some(effective_vmid) {
            return true;
        }
    }
    // On a Bare-only hart, every VMID aliases the sole context, VMID zero.
    bare_only
}

/// Writes and synchronizes a temporary G-stage context before its readback.
///
/// # Safety
///
/// H is present, local interrupts are excluded, and `vsatp` is Bare with MPRV
/// clear. Keep guest accesses inactive until the original context is restored.
unsafe fn select_guest_context(context: GuestAddressSpace) -> GuestAddressSpace {
    // SAFETY: The caller excludes guest accesses while changing MODE or VMID.
    unsafe { Hgatp::write(context) };
    translation::hfence_gvma(None, None);
    Hgatp::read()
}
