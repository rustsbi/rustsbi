//! Strict H-enabled RFENCE tests, including two-stage translation visibility.
//!
//! Build the kernel and firmware with `hypervisor` and run on an H CPU.
//! No SUCCESS-or-NOT_SUPPORTED fallback is allowed in this suite.

use core::{
    arch::asm,
    ptr::write_volatile,
    sync::atomic::{AtomicUsize, Ordering},
};
use sbi_spec::binary::{HartMask, SbiRet};
use sbi_testing::sbi;

const ENTRIES: usize = 4096 / size_of::<usize>();
const ROOT_SHIFT: usize = if usize::BITS == 64 { 30 } else { 22 };
const ID_SHIFT: usize = if usize::BITS == 64 { 44 } else { 22 };
const MODE: usize = if usize::BITS == 64 {
    8 << (usize::BITS - 4) // Sv39 / Sv39x4
} else {
    1 << (usize::BITS - 1) // Sv32 / Sv32x4
};
const VA: usize = 0x4000;
const GPA: usize = 0x4000_0000;
const VALUE_A: usize = 0x1234;
const VALUE_B: usize = 0x5678;
const ASID: usize = 1;
const VMID: usize = 1;
const LEAF: usize = 0xdf; // V/R/W/X/U/A/D; both stages permit a guest-user load.

#[repr(align(4096))]
struct Table([usize; ENTRIES]);
#[repr(align(16384))]
struct GuestRoot([usize; 4 * ENTRIES]);
#[repr(align(4096))]
struct Page([u32; 1024]);

static mut G_ROOT: GuestRoot = GuestRoot([0; 4 * ENTRIES]);
static mut G_MID: Table = Table([0; ENTRIES]);
static mut G_LEAF: Table = Table([0; ENTRIES]);
static mut V_ROOT: Table = Table([0; ENTRIES]);
static mut V_MID: Table = Table([0; ENTRIES]);
static mut V_LEAF: Table = Table([0; ENTRIES]);
static mut PAGE_A: Page = Page([0; 1024]);
static mut PAGE_B: Page = Page([0; 1024]);

// Only the boot hart writes page tables, while the secondary is waiting for
// a command. Release/acquire publishes commands/results and the table writes.
static COMMAND: AtomicUsize = AtomicUsize::new(0);
static OBSERVED: AtomicUsize = AtomicUsize::new(0);
const READ: usize = 1;
const SELECT_CALLER: usize = 2;
const CHECK_CONTEXT: usize = 3;
const SELECT_BARE: usize = 4;

fn pte(address: usize) -> usize {
    (address >> 12) << 10
}

pub(super) fn initialize() {
    // SAFETY: This runs once, before the secondary starts or any guest load.
    // All tables/pages are permanent aligned objects in supervisor-owned RAM.
    unsafe {
        write_volatile((&raw mut PAGE_A.0).cast::<u32>(), VALUE_A as u32);
        write_volatile((&raw mut PAGE_B.0).cast::<u32>(), VALUE_B as u32);
        let g_root = (&raw mut G_ROOT.0).cast::<usize>();
        let v_root = (&raw mut V_ROOT.0).cast::<usize>();
        let g_leaf = (&raw mut G_LEAF.0).cast::<usize>();
        let v_leaf = (&raw mut V_LEAF.0).cast::<usize>();
        // Guest page-table memory uses identity-mapped QEMU RAM. Data at GPA
        // and GPA+4K is separately mapped through G_LEAF to PAGE_A / PAGE_B.
        for address in (0x8000_0000..0x9000_0000).step_by(1 << ROOT_SHIFT) {
            write_volatile(g_root.add(address >> ROOT_SHIFT), pte(address) | LEAF);
        }
        if usize::BITS == 64 {
            write_volatile(
                g_root.add(GPA >> ROOT_SHIFT),
                pte((&raw const G_MID) as usize) | 1,
            );
            write_volatile((&raw mut G_MID.0).cast::<usize>(), pte(g_leaf as usize) | 1);
            write_volatile(v_root, pte((&raw const V_MID) as usize) | 1);
            write_volatile((&raw mut V_MID.0).cast::<usize>(), pte(v_leaf as usize) | 1);
        } else {
            write_volatile(g_root.add(GPA >> ROOT_SHIFT), pte(g_leaf as usize) | 1);
            write_volatile(v_root, pte(v_leaf as usize) | 1);
        }
        write_volatile(g_leaf, pte((&raw const PAGE_A) as usize) | LEAF);
        write_volatile(g_leaf.add(1), pte((&raw const PAGE_B) as usize) | LEAF);
        write_volatile(v_leaf.add(VA >> 12), pte(GPA) | LEAF);
    }
}

/// Original and installed guest CSR state on one hart.
pub(super) struct GuestState {
    old: (usize, usize),
    active: (usize, usize),
}

fn read_context() -> (usize, usize) {
    let (hgatp, vsatp);
    // SAFETY: This suite explicitly requires H; reading does not change state.
    unsafe { asm!("csrr {h}, hgatp", "csrr {v}, vsatp", h = out(reg) hgatp, v = out(reg) vsatp) };
    (hgatp, vsatp)
}

fn context(vmid: usize) -> (usize, usize) {
    (
        MODE | (vmid << ID_SHIFT) | ((&raw const G_ROOT) as usize >> 12),
        // The secondary initially has ASID 2 as well as VMID 2, so restoring
        // either CSR to the caller's state is observable.
        MODE | (vmid << ID_SHIFT) | ((&raw const V_ROOT) as usize >> 12),
    )
}

fn select_context((hgatp, vsatp): (usize, usize)) {
    // SAFETY: No guest is running. These are saved CSR values or the permanent
    // test tables. Clear vsatp while switching VMIDs to prevent speculative
    // cross-guest translations. A reused VMID retains its root and MODE.
    unsafe {
        asm!("csrw vsatp, zero", "csrw hgatp, {h}", "csrw vsatp, {v}",
            h = in(reg) hgatp, v = in(reg) vsatp);
    }
}

fn flush_local() {
    // SAFETY: H is required and no guest is executing; broad invalidation also
    // orders MODE changes when installing/restoring the test environment.
    unsafe {
        asm!(
            ".option push",
            ".option arch, +h",
            "hfence.gvma",
            "hfence.vvma",
            ".option pop"
        )
    };
}

impl GuestState {
    fn install(vmid: usize) -> Self {
        let state = Self {
            old: read_context(),
            active: context(vmid),
        };
        select_context(state.active);
        flush_local();
        state.check();
        state
    }

    fn check(&self) {
        assert_eq!(
            read_context(),
            self.active,
            "RFENCE must preserve the target guest context"
        );
    }

    pub(super) fn restore(self) {
        self.check();
        select_context(self.old);
        flush_local();
        assert_eq!(read_context(), self.old);
    }
}

pub(super) fn install_secondary() -> GuestState {
    GuestState::install(2)
}

fn guest_load() -> usize {
    let (old_status, value): (usize, usize);
    // SAFETY: The test tables map VA to a readable guest-user page. SPVP=0
    // selects VU permissions, and the original hstatus is restored afterwards.
    unsafe {
        asm!(
            ".option push", ".option arch, +h",
            "csrr {old}, hstatus", "li {tmp}, 1 << 8", "csrc hstatus, {tmp}",
            "hlv.w {value}, ({address})", "csrw hstatus, {old}", ".option pop",
            old = out(reg) old_status, tmp = out(reg) _,
            value = out(reg) value, address = in(reg) VA,
        );
    }
    let _ = old_status;
    value
}

pub(super) fn secondary_poll(state: &mut GuestState) {
    let command = COMMAND.load(Ordering::Acquire);
    if command == 0 {
        return;
    }
    state.check();
    match command {
        READ => OBSERVED.store(guest_load(), Ordering::Relaxed),
        SELECT_CALLER => {
            state.active = context(VMID);
            select_context(state.active);
            flush_local();
        }
        SELECT_BARE => {
            state.active = (0, 0);
            select_context(state.active);
            flush_local();
        }
        CHECK_CONTEXT => {}
        _ => panic!("unknown HFENCE test command"),
    }
    state.check();
    COMMAND.store(0, Ordering::Release);
}

fn remote_command(command: usize) -> usize {
    COMMAND.store(command, Ordering::Release);
    while COMMAND.load(Ordering::Acquire) != 0 {
        core::hint::spin_loop();
    }
    OBSERVED.load(Ordering::Relaxed)
}

fn ranges(mask: HartMask) {
    for (start, size) in [
        (0, 0),
        (0x1000, 0),
        (0x1000, 1),
        (0x1000, 0x1001),
        (1, usize::MAX),
    ] {
        assert_eq!(
            sbi::remote_hfence_gvma(mask, start, size),
            SbiRet::success(0)
        );
        assert_eq!(
            sbi::remote_hfence_vvma(mask, start, size),
            SbiRet::success(0)
        );
        for id in [0, 1, usize::MAX] {
            assert_eq!(
                sbi::remote_hfence_gvma_vmid(mask, start, size, id),
                SbiRet::success(0)
            );
            assert_eq!(
                sbi::remote_hfence_vvma_asid(mask, start, size, id),
                SbiRet::success(0)
            );
        }
    }
    for (start, size) in [(1, 4096), (usize::MAX & !0xfff, 4096)] {
        assert_eq!(
            sbi::remote_hfence_gvma(mask, start, size),
            SbiRet::invalid_address()
        );
        assert_eq!(
            sbi::remote_hfence_gvma_vmid(mask, start, size, VMID),
            SbiRet::invalid_address()
        );
        assert_eq!(
            sbi::remote_hfence_vvma(mask, start, size),
            SbiRet::invalid_address()
        );
        assert_eq!(
            sbi::remote_hfence_vvma_asid(mask, start, size, ASID),
            SbiRet::invalid_address()
        );
    }
}

#[derive(Clone, Copy)]
enum GuestLoadTarget {
    CurrentHart,
    Secondary,
}

fn load_guest_target(target: GuestLoadTarget) -> usize {
    match target {
        GuestLoadTarget::CurrentHart => guest_load(),
        GuestLoadTarget::Secondary => remote_command(READ),
    }
}

fn translation_visibility(mask: HartMask, target: GuestLoadTarget) {
    for with_id in [false, true] {
        // SAFETY: The only load targets are `guest_load` and the secondary
        // READ command, which acknowledges after that load. Both pointers
        // originate from permanent tables; indices 4 and 0 are in bounds,
        // aligned and initialized. The secondary performs no guest loads
        // between commands.
        unsafe {
            write_volatile(
                (&raw mut V_LEAF.0).cast::<usize>().add(VA >> 12),
                pte(GPA) | LEAF,
            );
            write_volatile(
                (&raw mut G_LEAF.0).cast::<usize>(),
                pte((&raw const PAGE_A) as usize) | LEAF,
            );
        }
        assert_eq!(sbi::remote_hfence_gvma(mask, 0, 0), SbiRet::success(0));
        assert_eq!(sbi::remote_hfence_vvma(mask, 0, 0), SbiRet::success(0));
        assert_eq!(load_guest_target(target), VALUE_A);
        // SAFETY: The selected guest-load path completed its read. No further
        // read is issued before VVMA. V_LEAF is permanent and VA selects
        // aligned, in-bounds entry 4.
        unsafe {
            write_volatile(
                (&raw mut V_LEAF.0).cast::<usize>().add(VA >> 12),
                pte(GPA + 4096) | LEAF,
            )
        };
        let ret = if with_id {
            sbi::remote_hfence_vvma_asid(mask, VA, 4096, ASID)
        } else {
            sbi::remote_hfence_vvma(mask, VA, 4096)
        };
        assert_eq!(ret, SbiRet::success(0));
        assert_eq!(
            load_guest_target(target),
            VALUE_B,
            "VS-stage mapping must be visible after VVMA"
        );

        // SAFETY: The selected guest-load path completed its read. No further
        // read is issued before VVMA. V_LEAF entry 4 is aligned and in bounds.
        unsafe {
            write_volatile(
                (&raw mut V_LEAF.0).cast::<usize>().add(VA >> 12),
                pte(GPA) | LEAF,
            )
        };
        assert_eq!(sbi::remote_hfence_vvma(mask, 0, 0), SbiRet::success(0));
        assert_eq!(load_guest_target(target), VALUE_A);
        // SAFETY: The selected guest-load path completed its read. No further
        // read is issued before GVMA. G_LEAF entry 0 is aligned and in bounds.
        unsafe {
            write_volatile(
                (&raw mut G_LEAF.0).cast::<usize>(),
                pte((&raw const PAGE_B) as usize) | LEAF,
            )
        };
        let ret = if with_id {
            sbi::remote_hfence_gvma_vmid(mask, GPA, 4096, VMID)
        } else {
            sbi::remote_hfence_gvma(mask, GPA, 4096)
        };
        assert_eq!(ret, SbiRet::success(0));
        assert_eq!(
            load_guest_target(target),
            VALUE_B,
            "G-stage mapping must be visible after GVMA"
        );
    }
}

pub(super) fn test_remote(remote: HartMask, mixed: HartMask) {
    let state = GuestState::install(VMID);
    ranges(remote);
    ranges(mixed);
    state.check();
    // First require restoration of a target with a different VMID and ASID.
    remote_command(CHECK_CONTEXT);
    // Selecting the caller's nonzero VMID must also work when the target has
    // no active G-stage translation. Every request must restore Bare exactly.
    remote_command(SELECT_BARE);
    ranges(remote);
    remote_command(CHECK_CONTEXT);
    println!("[rfence] H remote Bare VMID selection pass");
    // Then keep the target's CSRs fixed throughout prime -> PTE update ->
    // RFENCE -> observation. A CSR write must not hide a missing invalidation.
    remote_command(SELECT_CALLER);
    translation_visibility(remote, GuestLoadTarget::Secondary);
    state.restore();
    println!("[rfence] H remote delivery, guest restoration and translation visibility pass");
}

pub(super) fn test_local(mask: HartMask) {
    let state = GuestState::install(VMID);
    ranges(mask);
    ranges(HartMask::from_mask_base(0, 0));
    translation_visibility(mask, GuestLoadTarget::CurrentHart);
    state.restore();
    println!("[rfence] H local ranges and translation visibility pass");
}
