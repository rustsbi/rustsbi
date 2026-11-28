//! PMP protection of NAPOT regions.

use crate::SecMemProtector;
use pmpm::{
    PmpConfig, bitmap::PMPSlotAllocator, check_pmp_area_available, decode_pmp_addr, get_pmp_cfg,
    get_pmp_entry,
};
use riscv::register::{Permission, Range};

/// Platform callback for changing one PMP entry.
/// Return `true` only after the requested protection and its translation fences
/// have taken effect on every selected hart. Before returning `false`, restore
/// any changed entries and synchronize the old protection on those harts.
pub type UpdatePmp = unsafe fn(u32, usize, usize, &PmpConfig) -> bool;

pub(crate) struct SecMemProtectorByPMP {
    slots: PMPSlotAllocator,
    update_local: UpdatePmp,
    update_all: UpdatePmp,
}

impl SecMemProtectorByPMP {
    /// # Safety
    ///
    /// The caller owns the managed entries on every affected hart. They must
    /// be implemented, unlocked and use standard PMP permissions. Run local
    /// updates in M-mode with exclusive access to the packed configuration
    /// registers. The platform must serialize entry changes with affected
    /// contexts, including while applying or rolling back a broadcast.
    /// Both callbacks must implement the [`UpdatePmp`] contract.
    pub(crate) unsafe fn new(
        alloc_mask: u64,
        manage_mask: u64,
        update_local: UpdatePmp,
        update_all: UpdatePmp,
    ) -> Self {
        Self {
            slots: PMPSlotAllocator::new(alloc_mask, manage_mask),
            update_local,
            update_all,
        }
    }

    fn apply(
        &self,
        hwid: u32,
        addr: usize,
        len: usize,
        range: Range,
        perm: Permission,
        update: UpdatePmp,
    ) -> bool {
        self.slots.is_managed(hwid)
            // SAFETY: Construction establishes PMP ownership and synchronization.
            && unsafe { update(hwid, addr, len, &PmpConfig::new(range, perm, false)) }
    }
}

impl SecMemProtector for SecMemProtectorByPMP {
    fn is_protectable(&self, addr: usize, len: usize) -> bool {
        len != usize::MAX && check_pmp_area_available(addr, len, Range::NAPOT)
    }

    fn alloc(&mut self) -> Option<u32> {
        self.slots.alloc().ok()
    }

    fn free(&mut self, hwid: u32) -> bool {
        self.slots.free(hwid).is_ok()
    }

    fn disable(&self, hwid: u32) -> bool {
        self.apply(hwid, 0, 0, Range::OFF, Permission::NONE, self.update_all)
    }

    fn grant_access(&self, addr: usize, len: usize, hwid: u32) -> bool {
        if !self.slots.is_managed(hwid) || get_pmp_cfg(hwid).range() != Range::OFF {
            return false;
        }
        self.apply(
            hwid,
            addr,
            len,
            Range::NAPOT,
            Permission::RWX,
            self.update_local,
        )
    }

    fn retrive_access(&self, addr: usize, len: usize, hwid: u32) -> bool {
        if !self.slots.is_managed(hwid) {
            return false;
        }
        let (address, config) = get_pmp_entry(hwid);
        if config.range() != Range::NAPOT || config.perm() != Permission::RWX {
            return false;
        }
        let grant = decode_pmp_addr(address, config.range());
        grant.start() == addr as u128
            && grant.len() == len as u128
            && self.apply(hwid, 0, 0, Range::OFF, Permission::NONE, self.update_local)
    }

    fn grant_access_all(&self, addr: usize, len: usize, hwid: u32) -> bool {
        self.apply(
            hwid,
            addr,
            len,
            Range::NAPOT,
            Permission::RWX,
            self.update_all,
        )
    }

    fn retrive_access_all(&self, addr: usize, len: usize, hwid: u32) -> bool {
        self.apply(
            hwid,
            addr,
            len,
            Range::NAPOT,
            Permission::NONE,
            self.update_all,
        )
    }
}
