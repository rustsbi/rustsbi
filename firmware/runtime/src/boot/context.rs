//! Trap installation and next-stage architectural context.

#[cfg(target_pointer_width = "32")]
use csr::standard::isa::privileged::machine::{Medelegh, Mstatush};
use csr::{
    Read, Set, Write, WriteMode,
    standard::isa::{
        privileged::{
            hypervisor::{Hstateen0, Hstateen1, Hstateen2, Hstateen3},
            machine::{
                self, Mcounteren, Menvcfg, Mepc, Mideleg, Misa, Mseccfg, Mstateen0, Mstateen1,
                Mstateen2, Mstateen3, Mstatus, Mtvec,
            },
            supervisor::{Satp, Scounteren, Sie, Sstateen0, Sstateen1, Sstateen2, Sstateen3},
        },
        values::{
            address::{Address, AddressSpace},
            cache_block::CacheBlockInvalidate,
            counter::CounterSet,
            status::Privilege,
            trap::{Exception, ExceptionSet, InterruptSet},
            vector::{VectorBase, VectorMode},
        },
    },
};

use crate::{Result, ipi, irq::LocalIrqDisabled, trap::entry};

/// Machine software, timer, and external interrupt delegation bits in `mideleg`.
const MACHINE_INTERRUPT_DELEGATION_MASK: usize = (1 << 3) | (1 << 7) | (1 << 11);

/// Configures lower-privilege state access for a fresh supervisor.
///
/// Leaves the state-enable CSRs untouched when Runtime recovery or Smstateen is unavailable.
/// Requests access to AIA and IMSIC state when `smaia` is set and the registered
/// IPI service uses an IMSIC. The hart determines which state-enable bits are writable.
///
/// # Safety
///
/// Run during M-mode startup with exclusive ownership of lower-mode state.
/// The caller must be authorized to grant the supervisor access to the selected state.
pub unsafe fn configure_state_access(smaia: bool) {
    if !crate::recovery::is_ready() {
        return;
    }
    let enable_aia = smaia && ipi::uses_imsic();
    crate::irq::with_local_disabled(|_| {
        // SAFETY: The readiness check established the trap vector and clear MPRV
        // and mscratch. Interrupts stay masked throughout the probes below.
        if unsafe { Mstateen0::try_read() }.is_err() {
            return;
        }

        let mut m0 = Mstateen0::zero();
        m0.set(Mstateen0::CONTEXT, true);
        m0.set(Mstateen0::ENVCFG, true);
        m0.set(Mstateen0::SE0, true);
        if enable_aia {
            m0.set(Mstateen0::IMSIC, true);
            m0.set(Mstateen0::AIA, true);
            m0.set(Mstateen0::CSRIND, true);
        }

        let mut m1 = Mstateen1::zero();
        m1.set(Mstateen1::SE0, true);
        let mut m2 = Mstateen2::zero();
        m2.set(Mstateen2::SE0, true);
        let mut m3 = Mstateen3::zero();
        m3.set(Mstateen3::SE0, true);

        // SAFETY: The probe established Smstateen. Startup owns lower-mode
        // access, with local interrupts excluded before any lower-mode entry.
        unsafe {
            Mstateen0::write(m0);
            Mstateen1::write(m1);
            Mstateen2::write(m2);
            Mstateen3::write(m3);
        }

        // SAFETY: Runtime recovery and local interrupt exclusion remain active;
        // each probe establishes the lower-level CSR family before clearing it.
        unsafe {
            if Sstateen0::try_read().is_ok() {
                Sstateen0::write(0);
                Sstateen1::write(0);
                Sstateen2::write(0);
                Sstateen3::write(0);
            }
            if Hstateen0::try_read().is_ok() {
                Hstateen0::write(Hstateen0::zero());
                Hstateen1::write(Hstateen1::zero());
                Hstateen2::write(Hstateen2::zero());
                Hstateen3::write(Hstateen3::zero());
            }
        }
    });
}

/// Requests supervisor access to `seed` through `mseccfg.SSEED`.
///
/// Missing recovery, an unavailable CSR, or a read-only zero leaves access unchanged.
pub fn enable_supervisor_seed() {
    if !crate::recovery::is_ready() {
        return;
    }
    // SAFETY: Recovery is ready. CSRRS selects only SSEED, preserving the PMP policy bits.
    let _ = unsafe { Mseccfg::try_write(Set(Mseccfg::SSEED)) };
}

/// Installs machine exception handling before platform feature probes.
///
/// # Safety
///
/// Run in M-mode with a valid Runtime stack, local machine interrupts disabled,
/// and `mscratch` set to zero.
#[doc(hidden)]
pub unsafe extern "C" fn prepare_traps() {
    // SAFETY: Startup owns this hart's trap state before entering policy.
    unsafe {
        install(entry::trap_entry);
        let mut status = Mstatus::read().into_write();
        status.set(Mstatus::MPRV, false);
        #[cfg(target_pointer_width = "64")]
        status.set(Mstatus::MDT, false);
        Mstatus::write(status);
    }
    // Install the basic vector before probing so an absent `mstatush` can recover.
    #[cfg(target_pointer_width = "32")]
    // SAFETY: The basic Runtime trap entry is installed with mscratch/MPRV clear.
    if let Ok(high) = unsafe { Mstatush::try_read() } {
        // SAFETY:
        // 1. The probe confirmed that `mstatush` is available.
        // 2. Startup owns this hart's trap setup.
        unsafe {
            let mut high = high.into_write();
            high.set(Mstatush::MDT, false);
            Mstatush::write(high);
            install(entry::trap_entry_high);
        }
    }
    if Misa::read().has_extension('H') {
        // SAFETY:
        // 1. The hypervisor extension is present.
        // 2. Startup owns this hart's trap setup.
        unsafe {
            install(entry::trap_entry_hypervisor);
        }
    }
}

/// Installs a machine trap vector for the current hart.
///
/// # Safety
///
/// Run in M-mode with machine interrupts disabled.
/// `entry` must implement Runtime's stack and register-preservation protocol.
/// Every optional CSR used by that entry must exist on this hart.
unsafe fn install(entry: unsafe extern "C" fn()) {
    let base = VectorBase::new(Address::from_bits(entry as *const () as usize))
        .expect("aligned trap vector");
    let mut vector = Mtvec::zero();
    vector.set(Mtvec::BASE, base);
    vector.set(Mtvec::MODE, VectorMode::Direct);
    // SAFETY: The caller supplies a Runtime trap entry.
    unsafe { Mtvec::write(vector) }
}

/// ISA extensions advertised for this hart that affect supervisor environment setup.
#[derive(Clone, Copy, Default)]
pub struct SupervisorEnvConfig {
    /// Cache-block management instructions.
    pub zicbom: bool,
    /// Cache-block zero instruction.
    pub zicboz: bool,
    /// Page-based memory types.
    pub svpbmt: bool,
    /// Page faults for software-managed accessed and dirty bits.
    pub svade: bool,
    /// Hardware updates of accessed and dirty bits.
    pub svadu: bool,
}

/// Initializes the supervisor environment for a fresh hart.
///
/// Requests the [SBI FWFT reset values] for landing pads, shadow stacks,
/// supervisor double traps and, on RV64, pointer masking.
/// Enables the advertised cache-block operations and requests PBMT for `svpbmt`.
/// Following the [CPU binding], requests software A/D updates for `svade`
/// and hardware updates for `svadu` alone.
/// With neither extension advertised, it preserves the incoming `ADUE` value.
///
/// An absent `menvcfg` is ignored.
/// On RV32, an inaccessible `menvcfgh` returns an error if `menvcfg` is present.
/// A fault writing the high word can leave the low word updated.
///
/// [SBI FWFT reset values]: https://docs.riscv.org/reference/sbi/v3.0/ext-firmware-features.html#table_fw_features_attribute_values
/// [CPU binding]: https://github.com/torvalds/linux/blob/v6.18/Documentation/devicetree/bindings/riscv/extensions.yaml#L174-L193
///
/// # Safety
///
/// Execute in M-mode during hart startup with Runtime trap entry installed,
/// local interrupts disabled, `mscratch` zero, and `MPRV` clear. Own this hart's
/// environment before lower-mode entry. A resume must preserve the suspended
/// supervisor's FWFT settings.
pub unsafe fn configure_supervisor_envcfg(extensions: SupervisorEnvConfig) -> Result<()> {
    // SAFETY: The caller owns this hart's startup context and establishes recovery.
    let current = match unsafe { Menvcfg::try_read() } {
        Ok(value) => value,
        Err(error) => {
            // An RV32 pair can fault on either word. Probe a primary-word bit
            // to distinguish an absent register from a failed upper-word read.
            if cfg!(target_pointer_width = "32")
                // SAFETY: The startup caller keeps the recovery context active.
                && unsafe { machine::try_read_menvcfg_bit(Menvcfg::LPE) }.is_ok()
            {
                return Err(error.into());
            }
            return Ok(());
        }
    };
    let enable_pbmt = extensions.svpbmt && !current.get(Menvcfg::PBMTE);
    let previous_adue = current.get(Menvcfg::ADUE);
    let adue = match (extensions.svade, extensions.svadu) {
        (true, _) => false,
        (false, true) => true,
        (false, false) => previous_adue,
    };
    let mut cfg = current.into_write();
    cfg.set(Menvcfg::LPE, false);
    cfg.set(Menvcfg::SSE, false);
    cfg.set(Menvcfg::DTE, false);
    cfg.set(Menvcfg::ADUE, adue);
    #[cfg(target_pointer_width = "64")]
    cfg.set(
        Menvcfg::PMM,
        csr::standard::isa::values::pointer_masking::PointerMasking::Disabled,
    );
    cfg.set(
        Menvcfg::CBIE,
        if extensions.zicbom {
            CacheBlockInvalidate::Invalidate
        } else {
            CacheBlockInvalidate::Disabled
        },
    );
    cfg.set(Menvcfg::CBCFE, extensions.zicbom);
    cfg.set(Menvcfg::CBZE, extensions.zicboz);
    if enable_pbmt {
        cfg.set(Menvcfg::PBMTE, true);
    }
    // SAFETY: The caller owns environment configuration before lower-mode entry.
    unsafe { Menvcfg::try_write(cfg)? };
    if enable_pbmt || adue != previous_adue {
        // SAFETY: The startup caller establishes recovery with local interrupts disabled.
        unsafe { crate::fence::synchronize_translations() };
    }
    Ok(())
}

/// Initializes hart-local services, delegation, and counter access for the next stage.
///
/// # Panics
///
/// Panics if the registered IPI device fails to initialize or acknowledge its source,
/// or if delegation readback leaves a machine software, timer or external interrupt delegated.
///
/// # Safety
///
/// 1. Run on a hart with S-mode and counter-enable CSRs, in M-mode with local
///    interrupts disabled, Runtime recovery ready, and no active lower-mode execution.
/// 2. For IMSIC delivery, exclusively own this hart's machine interrupt file
///    and supply its implemented identity count at registration. Pending
///    interrupts may be discarded when the file is initialized.
pub unsafe fn configure_next_stage() {
    // SAFETY: The startup caller keeps MIE clear for this configuration.
    let irq = unsafe { LocalIrqDisabled::assume_disabled() };
    if let Some(ipi) = ipi::get() {
        ipi.init_hart(&irq)
            .expect("could not initialize hart IPI state");
    }
    // SAFETY: The caller owns this hart's next-stage configuration.
    unsafe {
        let delegation = usize::MAX & !MACHINE_INTERRUPT_DELEGATION_MASK;
        Mideleg::write(InterruptSet::from_bits(delegation));
        let delegated = Mideleg::read().bits();
        assert_eq!(
            delegated & MACHINE_INTERRUPT_DELEGATION_MASK,
            0,
            "machine interrupts were delegated to S-mode: mideleg={delegated:#x}"
        );
        let mut exceptions = ExceptionSet::from_bits(u64::MAX);
        for exception in [
            Exception::SupervisorEcall,
            Exception::LoadMisaligned,
            Exception::StoreMisaligned,
            Exception::IllegalInstruction,
            Exception::LoadAccess,
            Exception::StoreAccess,
        ] {
            exceptions.set(exception, false);
        }
        machine::write_medeleg_word(exceptions);
        #[cfg(target_pointer_width = "32")]
        {
            // Older privileged implementations may omit the upper delegation CSR.
            let _ = Medelegh::try_write((exceptions.bits() >> 32) as u32);
        }
        Mcounteren::write(CounterSet::all());
        Scounteren::write(CounterSet::all());
    }
}

pub(crate) fn set_return_mode(status: &mut Mstatus<WriteMode>, mode: Privilege) {
    status.set(Mstatus::MPP, mode);
    status.set(Mstatus::MPIE, true);
    status.set(Mstatus::SIE, false);
}

/// Prepares translation and interrupt state for a new supervisor stage.
///
/// # Safety
///
/// Run in M-mode with interrupts disabled, `MPRV` clear, and Runtime trap entry
/// installed.
/// The outgoing supervisor address space must no longer be needed.
pub(crate) unsafe fn prepare_supervisor() {
    // SAFETY: The caller is replacing the outgoing supervisor context;
    // clearing its translation and interrupt enables cannot invalidate M-mode state.
    unsafe {
        Sie::write(Sie::zero());
        Satp::write(AddressSpace::bare());
    }
    // SAFETY: This function's caller keeps MIE clear throughout the handoff.
    let irq = unsafe { LocalIrqDisabled::assume_disabled() };
    if let Some(ipi) = ipi::get() {
        ipi.enable_interrupt(&irq);
    }
}

/// Programs the initial boot handoff in the current hart's CSRs.
///
/// Trap handoffs update the saved frame restored by trap entry instead.
///
/// # Safety
///
/// Run in M-mode with interrupts disabled, Runtime recovery ready, and no live
/// supervisor context. The next stage must be able to execute at `start_addr`
/// in `next_mode`.
pub(super) unsafe fn stage_next_mode(start_addr: usize, next_mode: Privilege) {
    // SAFETY: The boot caller owns the outgoing trap state.
    unsafe {
        prepare_supervisor();
    }
    // A recovered probe changes live trap state. Finish probes before
    // staging the status and PC consumed by the final mret.
    #[cfg(target_pointer_width = "32")]
    // SAFETY: The startup caller establishes Runtime trap recovery before mret staging.
    if let Ok(high) = unsafe { Mstatush::try_read() } {
        // SAFETY: The probe succeeded; boot owns the outgoing trap state.
        unsafe {
            let mut high = high.into_write();
            high.set(Mstatush::MPV, false);
            Mstatush::write(high);
        }
    }
    // SAFETY: Boot owns the outgoing trap state after all recovered probes.
    unsafe {
        let mut status = Mstatus::read().into_write();
        set_return_mode(&mut status, next_mode);
        #[cfg(target_pointer_width = "64")]
        status.set(Mstatus::MPV, false);
        Mstatus::write(status);
        Mepc::write(Address::from_bits(start_addr));
    }
}
