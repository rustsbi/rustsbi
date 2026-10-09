//! Runtime-owned cold entry and its safe firmware-policy connection.

use crate::csr::Readable;
use core::fmt::Debug;
use core::sync::atomic::AtomicU32;

use spin::Mutex;

use crate::{DeviceTreeHandoff, PlatformDescription};

use super::{DynamicInfo, DynamicReadError};

static BOOT_LOCK: AtomicU32 = AtomicU32::new(0);
static BOOT_READY: AtomicU32 = AtomicU32::new(0);

/// MPRV/MIE mask cleared before cold-entry memory accesses.
const ENTRY_MSTATUS_CLEAR: usize = (1 << 17) | (1 << 3);

/// The previous-stage handoff captured for one hart at cold entry.
pub struct BootInput {
    /// Hardware hart ID, read before the enabled topology is published.
    pub hart_id: usize,
    /// The previous stage's device-tree handoff.
    pub device_tree: DeviceTreeHandoff,
    /// A copied dynamic handoff, or `None` for builds using another protocol.
    pub dynamic_info: Option<Result<DynamicInfo, DynamicReadError>>,
}

/// Owned discovery results passed from the bootstrap stack to the final boot stack.
pub struct PreparedBoot<C> {
    /// The selected boot hart's validated platform description.
    pub platform: PlatformDescription,
    /// Raw next-stage entry excluded from secondary stack storage.
    /// Keep this address even when later policy validation rejects the handoff.
    pub stack_exclusion_entry: usize,
    /// Policy data consumed on the boot hart's final stack.
    pub context: C,
}

enum BootState<C> {
    Empty,
    Prepared {
        hart_id: usize,
        boot: PreparedBoot<C>,
    },
    Consumed,
}

/// Runtime's single owned slot between bootstrap discovery and final-stack boot.
///
/// Policy declares one static slot. Runtime alone publishes and consumes it;
/// neither operation permits a borrowed bootstrap-stack value to escape.
pub struct BootStorage<C: Send + 'static> {
    state: Mutex<BootState<C>>,
}

impl<C: Send + 'static> BootStorage<C> {
    /// Creates an empty slot for one firmware entry.
    pub const fn new() -> Self {
        Self {
            state: Mutex::new(BootState::Empty),
        }
    }

    fn publish(&self, hart_id: usize, boot: PreparedBoot<C>) {
        let mut state = self.state.lock();
        assert!(
            matches!(*state, BootState::Empty),
            "boot discovery published more than once"
        );
        *state = BootState::Prepared { hart_id, boot };
    }

    fn take_for(&self, current_hart: usize) -> Option<PreparedBoot<C>> {
        let mut state = self.state.lock();
        match &*state {
            BootState::Empty => panic!("boot discovery has not been published"),
            BootState::Prepared { hart_id, .. } if *hart_id == current_hart => {
                let BootState::Prepared { boot, .. } =
                    core::mem::replace(&mut *state, BootState::Consumed)
                else {
                    unreachable!()
                };
                Some(boot)
            }
            _ => None,
        }
    }
}

impl<C: Send + 'static> Default for BootStorage<C> {
    fn default() -> Self {
        Self::new()
    }
}

/// Firmware policy at the three safe stages of Runtime's cold-entry sequence.
///
/// Runtime serializes candidates, validates stack storage and publishes topology.
/// It calls the initializer only after the discovery frames have returned.
pub trait BootPolicy: 'static {
    /// Owned policy state retained between the bootstrap and final stack phases.
    type Context: Send + 'static;
    /// The original failure returned by candidate preparation.
    type PrepareError: Debug;
    /// The permanent slot used by this firmware entry.
    const STORAGE: &'static BootStorage<Self::Context>;
    /// Whether `a2` contains a dynamic handoff. Other protocols never read it.
    const USE_DYNAMIC_HANDOFF: bool;
    /// Maximum enabled hart count, independent of hardware ID values.
    const HART_CAPACITY: usize;
    /// Stack size matching the linker-owned bootstrap storage.
    const STACK_SIZE_PER_HART: usize;

    /// Selects a candidate. `Ok(None)` means this hart does not lead boot.
    /// The selected candidate must initialize the heap before returning.
    fn prepare(input: BootInput)
    -> Result<Option<PreparedBoot<Self::Context>>, Self::PrepareError>;
    /// Consumes discovery results on the selected boot hart's final stack.
    fn initialize_boot(boot: PreparedBoot<Self::Context>);
    /// Initializes a loader-started secondary hart on its published stack.
    fn initialize_secondary(input: BootInput);
}

/// A compile-time connection between Runtime's cold entry and safe policy.
///
/// Store this value in a `#[used]` static.
/// The firmware linker retains Runtime's entry in `.text.entry` and names its
/// beginning `_start`.
pub struct FirmwareEntry {
    _entry: unsafe extern "C" fn() -> !,
}

impl FirmwareEntry {
    /// Connects the Runtime cold entry to `P` without running startup code.
    ///
    /// # Panics
    ///
    /// Panics if the stack size is not a multiple of 128 bytes or does not
    /// exceed the trap-frame size.
    pub const fn new<P: BootPolicy>() -> Self {
        assert!(
            P::STACK_SIZE_PER_HART.is_multiple_of(128),
            "stack size must be a multiple of the stack alignment"
        );
        assert!(
            P::STACK_SIZE_PER_HART > crate::trap::entry::FRAME_BYTES,
            "stack size must exceed the trap frame"
        );
        Self { _entry: start::<P> }
    }
}

/// The sole architectural entry in the firmware linker's retained entry section.
///
/// # Safety
///
/// 1. The previous stage enters in M-mode once per hart, before firmware Rust
///    references exist. `a1` points to an exclusively owned, writable device
///    tree whose descriptions match the hardware.
/// 2. For dynamic firmware, nonzero `a2` designates immutable foreign storage,
///    reserved until all receiving harts consume it. It must not alias MMIO or
///    a live firmware Rust allocation.
/// 3. The linker owns the image layout and relocation records. RAM immediately
///    after the image is exclusively available for secondary stacks, excluding
///    the complete next-stage image and every live loader object. The platform
///    description matches this envelope.
///
/// Entry clears MPRV/MIE before accessing memory, relocates once, clears BSS,
/// and installs disjoint stacks.
#[cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), unsafe(naked))]
#[unsafe(link_section = ".text.entry")]
unsafe extern "C" fn start<P: BootPolicy>() -> ! {
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    core::arch::naked_asm!(
        include_str!("start.S"),
        early_vector = sym super::fail_stop,
        relocation_update = sym relocate,
        locate_stack = sym super::locate_stack,
        boot_lock = sym BOOT_LOCK,
        boot_ready = sym BOOT_READY,
        initialize = sym prepare::<P>,
        main = sym initialize::<P>,
        finish_boot = sym super::finish_boot,
        entry_mstatus_clear = const ENTRY_MSTATUS_CLEAR,
        XLEN = const usize::BITS,
    );
    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    unimplemented!("Firmware cold entry requires a RISC-V target");
}

/// Applies only the linker-owned relative relocations before Rust memory exists.
///
/// # Safety
///
/// Only the elected cold-entry hart may call this once, before any Rust
/// references exist or another hart uses relocated state.
/// The complete linker-owned relocation table must describe writable targets
/// in the loaded image, and no external writer may modify it or those targets.
#[cfg_attr(any(target_arch = "riscv32", target_arch = "riscv64"), unsafe(naked))]
unsafe extern "C" fn relocate() {
    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
    core::arch::naked_asm!(
        include_str!("relocation.S"),
        R_RISCV_RELATIVE = const 3,
        XLEN = const usize::BITS,
    );
    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
    unimplemented!("Firmware relocation requires a RISC-V target");
}

fn handoff<P: BootPolicy>(device_tree_address: usize, dynamic_info_address: usize) -> BootInput {
    // SAFETY:
    // 1. The private entry bridge establishes the exclusive foreign-FDT contract.
    // 2. It clears MPRV/MIE and installs a stack and the early recovery vector.
    let device_tree = unsafe { DeviceTreeHandoff::from_entry_register(device_tree_address) };
    let dynamic_info = P::USE_DYNAMIC_HANDOFF.then(|| {
        // SAFETY:
        // 1. This protocol binds a2 to foreign storage reserved for every hart's copy.
        // 2. The entry bridge cleared MPRV/MIE and installed the stack and early vector.
        unsafe { super::handoff::snapshot(dynamic_info_address) }
    });
    BootInput {
        hart_id: crate::csr::Mhartid::read()
            .expect("machine hart ID CSR is always readable in M-mode"),
        device_tree,
        dynamic_info,
    }
}

extern "C" fn prepare<P: BootPolicy>(
    device_tree_address: usize,
    dynamic_info_address: usize,
) -> usize {
    let input = handoff::<P>(device_tree_address, dynamic_info_address);
    let hart_id = input.hart_id;
    // Exclude the exact successfully copied ABI layout. With an invalid
    // header, reserve the largest possible layout when its address fits;
    // policy will still report the original snapshot error and stop boot.
    let handoff_range = if P::USE_DYNAMIC_HANDOFF && dynamic_info_address != 0 {
        let size_bytes = input
            .dynamic_info
            .as_ref()
            .and_then(|info| info.as_ref().ok())
            .and_then(|info| super::handoff::storage_size_bytes(info.version))
            .unwrap_or(super::handoff::MAX_STORAGE_BYTES);
        crate::memory::PhysAddrRange::from_start_len(
            crate::memory::PhysAddr::new(dynamic_info_address),
            size_bytes,
        )
        .ok()
    } else {
        None
    };
    let Some(boot) = P::prepare(input).expect("firmware entry rejected the platform description")
    else {
        return 0;
    };
    // SAFETY:
    // 1. Entry holds BOOT_LOCK and exclusively owns the linker bootstrap stack.
    // 2. The selected candidate initialized the heap.
    // 3. Other harts wait stackless; entry publishes BOOT_READY after Rust returns.
    // 4. The loader contract excludes all live foreign objects from stack storage.
    unsafe {
        super::stack::initialize_stacks(
            P::STACK_SIZE_PER_HART,
            P::HART_CAPACITY,
            &boot.platform,
            boot.stack_exclusion_entry,
            handoff_range,
        )
    }
    .expect("firmware could not initialize its hart topology");
    P::STORAGE.publish(hart_id, boot);
    1
}

extern "C" fn initialize<P: BootPolicy>(device_tree_address: usize, dynamic_info_address: usize) {
    let hart = crate::hart::HartId::current().expect("hart belongs to published topology");
    if let Some(boot) = P::STORAGE.take_for(hart.as_usize()) {
        P::initialize_boot(boot);
    } else {
        P::initialize_secondary(handoff::<P>(device_tree_address, dynamic_info_address));
    }
    if !crate::trap::init::current_is_ready() || riscv::register::mstatus::read().mie() {
        // SAFETY: entry established M-mode; fail_stop needs no caller stack.
        unsafe { super::fail_stop() };
    }
}
