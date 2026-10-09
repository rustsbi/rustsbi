//! Small, typed wrappers for the machine CSRs used by Runtime.
//!
//! The wrappers keep the unavoidable privileged instructions in Runtime. A
//! platform adapter therefore only has to acknowledge its own device; it
//! does not need to know how Runtime enables or signals a RISC-V interrupt.

/// Reads the current hart's architectural identifier.
#[inline]
pub fn mhartid() -> usize {
    riscv::register::mhartid::read()
}

/// The supervisor timer compare CSR introduced by Sstc.
pub(crate) const STIMECMP: u16 = 0x14d;

/// Probes whether the current hart implements Sstc's `stimecmp` CSR.
///
/// The probe is only used during Runtime trap initialization, after the
/// guarded-fault entry is available; an absent CSR is reported as `false`.
#[inline(never)]
pub(crate) fn has_stimecmp() -> bool {
    crate::trap::read_csr_guarded::<STIMECMP>().is_ok()
}

/// State-enable registers used to prepare a supervisor next stage.
pub mod stateen {
    use core::arch::asm;

    const MSTATEEN0: u16 = 0x30c;
    const MSTATEEN1: u16 = 0x30d;
    const MSTATEEN2: u16 = 0x30e;
    const MSTATEEN3: u16 = 0x30f;
    const SSTATEEN0: u16 = 0x10c;
    const SSTATEEN1: u16 = 0x10d;
    const SSTATEEN2: u16 = 0x10e;
    const SSTATEEN3: u16 = 0x10f;
    const HSTATEEN0: u16 = 0x60c;
    const HSTATEEN1: u16 = 0x60d;
    const HSTATEEN2: u16 = 0x60e;
    const HSTATEEN3: u16 = 0x60f;

    const CONTEXT: u64 = 1u64 << 57;
    const IMSIC: u64 = 1u64 << 58;
    const AIA: u64 = 1u64 << 59;
    const SVSLCT: u64 = 1u64 << 60;
    const ENVCFG: u64 = 1u64 << 62;
    const STATEN: u64 = 1u64 << 63;

    /// Configures state access for the supervisor next stage on this hart.
    ///
    /// An implementation without Smstateen needs no configuration. When
    /// Smstateen is present, Runtime also clears every implemented lower-level
    /// state-enable register as required before entering a fresh supervisor.
    /// `aia_enabled` exposes supervisor AIA and IMSIC state.
    pub fn configure_supervisor(aia_enabled: bool) {
        if crate::trap::read_csr_guarded::<MSTATEEN0>().is_err() {
            return;
        }

        let mut stateen0 = STATEN | CONTEXT | ENVCFG;
        if aia_enabled {
            stateen0 |= IMSIC | AIA | SVSLCT;
        }
        // CTR is intentionally omitted until Prototyper independently detects
        // Ssctr; it is not an AIA capability.

        write64::<MSTATEEN0, { MSTATEEN0 + 0x10 }>(stateen0);
        write64::<MSTATEEN1, { MSTATEEN1 + 0x10 }>(STATEN);
        write64::<MSTATEEN2, { MSTATEEN2 + 0x10 }>(STATEN);
        write64::<MSTATEEN3, { MSTATEEN3 + 0x10 }>(STATEN);

        if crate::trap::read_csr_guarded::<SSTATEEN0>().is_ok() {
            write::<SSTATEEN0>(0);
            write::<SSTATEEN1>(0);
            write::<SSTATEEN2>(0);
            write::<SSTATEEN3>(0);
        }
        if crate::trap::read_csr_guarded::<HSTATEEN0>().is_ok() {
            write64::<HSTATEEN0, { HSTATEEN0 + 0x10 }>(0);
            write64::<HSTATEEN1, { HSTATEEN1 + 0x10 }>(0);
            write64::<HSTATEEN2, { HSTATEEN2 + 0x10 }>(0);
            write64::<HSTATEEN3, { HSTATEEN3 + 0x10 }>(0);
        }
    }

    #[inline(always)]
    fn write<const CSR: u16>(value: usize) {
        // SAFETY: configure_supervisor probes the relevant state-enable bank
        // before use, and Smstateen defines each implemented bank completely.
        unsafe {
            asm!("csrw {csr}, {value}", csr = const CSR, value = in(reg) value, options(nomem))
        }
    }

    #[inline(always)]
    fn write64<const CSR: u16, const CSR_HIGH: u16>(value: u64) {
        write::<CSR>(value as usize);
        #[cfg(target_pointer_width = "32")]
        write::<CSR_HIGH>((value >> 32) as usize);
    }
}

/// Machine-interrupt enable bits used by the Runtime trap mechanism.
pub mod mie {
    /// Enables machine software interrupts on the current hart.
    #[inline]
    pub fn set_machine_software() {
        // SAFETY: Runtime owns the machine trap state on the current hart.
        unsafe { riscv::register::mie::set_msoft() }
    }

    /// Enables machine timer interrupts on the current hart.
    #[inline]
    pub fn set_machine_timer() {
        // SAFETY: Runtime owns the machine trap state on the current hart.
        unsafe { riscv::register::mie::set_mtimer() }
    }

    /// Disables machine timer interrupts on the current hart.
    #[inline]
    pub fn clear_machine_timer() {
        // SAFETY: Runtime owns the machine trap state on the current hart.
        unsafe { riscv::register::mie::clear_mtimer() }
    }

    /// Enables machine external interrupts on the current hart.
    #[inline]
    pub fn set_machine_external() {
        // SAFETY: Runtime owns the machine trap state on the current hart.
        unsafe { riscv::register::mie::set_mext() }
    }
}

/// Machine-interrupt pending bits used by the Runtime trap mechanism.
pub mod mip {
    /// Signals a supervisor software interrupt to the next-stage supervisor.
    #[inline]
    pub fn set_supervisor_software() {
        // SAFETY: Runtime owns the machine trap state on the current hart.
        unsafe { riscv::register::mip::set_ssoft() }
    }

    /// Signals a supervisor timer interrupt to the next-stage supervisor.
    #[inline]
    pub fn set_supervisor_timer() {
        // SAFETY: Runtime owns the machine trap state on the current hart.
        unsafe { riscv::register::mip::set_stimer() }
    }

    /// Clears the supervisor timer interrupt pending bit on the current hart.
    #[inline]
    pub fn clear_supervisor_timer() {
        // SAFETY: Runtime owns the machine trap state on the current hart.
        // Writes to STIP are ignored when Sstc drives it.
        unsafe { riscv::register::mip::clear_stimer() }
    }
}

use crate::trap::Error;

pub(crate) mod private {
    pub trait Sealed {}
}

pub(crate) trait Value: Copy {
    fn from_bits(bits: usize) -> Self;
    fn bits(self) -> usize;
}

impl Value for usize {
    fn from_bits(bits: usize) -> Self {
        bits
    }

    fn bits(self) -> usize {
        self
    }
}

pub(crate) trait Csr: private::Sealed {
    type Value: Value;
    const NUMBER: u16;
}

pub(crate) trait Readable: Csr {
    /// Reads through this CSR's native or guarded architectural access.
    fn read() -> Result<Self::Value, Error>;
}

pub(crate) trait Writable: Csr {
    fn write(value: Self::Value) -> Result<(), Error>;
}

macro_rules! word_value {
    ($($(#[$attribute:meta])* $name:ident;)*) => {$(
        $(#[$attribute])*
        #[derive(Clone, Copy, Debug, Eq, PartialEq)]
        pub(crate) struct $name(usize);

        $(#[$attribute])*
        impl Value for $name {
            fn from_bits(bits: usize) -> Self { Self(bits) }
            fn bits(self) -> usize { self.0 }
        }
    )*};
}

macro_rules! identity {
    ($name:ty, $number:expr, $value:ty) => {
        impl $crate::csr::private::Sealed for $name {}
        impl $crate::csr::Csr for $name {
            type Value = $value;
            const NUMBER: u16 = $number;
        }
    };
}

macro_rules! readable {
    ($name:ty, $number:expr, $value:ty) => {
        $crate::csr::identity!($name, $number, $value);
        impl $crate::csr::Readable for $name {
            fn read() -> core::result::Result<Self::Value, $crate::trap::Error> {
                $crate::trap::read_csr_guarded::<{ $number }>()
                    .map(<Self::Value as $crate::csr::Value>::from_bits)
            }
        }
    };
}

macro_rules! writable {
    ($name:ty, $number:expr) => {
        impl $crate::csr::Writable for $name {
            fn write(value: Self::Value) -> core::result::Result<(), $crate::trap::Error> {
                $crate::trap::write_csr_guarded::<{ $number }>($crate::csr::Value::bits(value))
            }
        }
    };
}

macro_rules! registers {
    (read { $($(#[$attribute:meta])* $name:ident: $value:ty = $number:expr;)* }
     write { $($(#[$rw_attribute:meta])* $rw_name:ident: $rw_value:ty = $rw_number:expr;)* }) => {
        $(
            $(#[$attribute])*
            pub(crate) enum $name {}
            $(#[$attribute])*
            $crate::csr::readable!($name, $number, $value);
        )*
        $(
            $(#[$rw_attribute])*
            pub(crate) enum $rw_name {}
            $(#[$rw_attribute])*
            $crate::csr::readable!($rw_name, $rw_number, $rw_value);
            $(#[$rw_attribute])*
            $crate::csr::writable!($rw_name, $rw_number);
        )*
    };
}

macro_rules! native_read {
    ($number:expr) => { $crate::csr::native_read!($number, nomem, nostack) };
    ($number:expr, [$($option:ident),*]) => {
        $crate::csr::native_read!($number, $($option),*)
    };
    ($number:expr, $($option:ident),*) => {{
        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
        {
            let bits: usize;
            // SAFETY:
            // 1. The owning mechanism fixes this CSR identity at compile time.
            // 2. Its callers establish the execution mode and extension prerequisites.
            unsafe {
                core::arch::asm!(
                    "csrr {value}, {csr}",
                    csr = const $number,
                    value = out(reg) bits,
                    options($($option),*),
                );
            }
            bits
        }
        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
        unimplemented!("native CSR access requires a RISC-V target")
    }};
}

macro_rules! native_registers {
    (read { $($read:tt)* } write { $($write:tt)* }) => {
        $crate::csr::native_registers!(@impl [nomem, nostack]
            read { $($read)* } write { $($write)* } write_only {});
    };
    (ordered; read { $($read:tt)* } write { $($write:tt)* }
     write_only { $($write_only:tt)* }) => {
        $crate::csr::native_registers!(@impl [nostack]
            read { $($read)* } write { $($write)* } write_only { $($write_only)* });
    };
    (@impl $options:tt
     read { $($(#[$attribute:meta])* $name:ident: $value:ty = $number:expr;)* }
     write { $($rw_name:ident: $rw_value:ty = $rw_number:expr;)* }
     write_only { $($wo_name:ident: $wo_value:ty = $wo_number:expr;)* }) => {
        $(
            $(#[$attribute])*
            $crate::csr::native_registers!(@identity $name: $value = $number);
            $(#[$attribute])*
            impl $crate::csr::Readable for $name {
                #[inline]
                fn read() -> core::result::Result<Self::Value, $crate::trap::Error> {
                    match () {
                        #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
                        () => Ok(<Self::Value as $crate::csr::Value>::from_bits(
                            $crate::csr::native_read!(
                                <Self as $crate::csr::Csr>::NUMBER, $options
                            ),
                        )),
                        #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
                        () => unimplemented!("native CSR access requires a RISC-V target"),
                    }
                }
            }
        )*
        $(
            $crate::csr::native_registers!(@impl $options
                read { $rw_name: $rw_value = $rw_number; } write {} write_only {});
            $crate::csr::native_registers!(@write $options $rw_name);
        )*
        $(
            $crate::csr::native_registers!(@identity $wo_name: $wo_value = $wo_number);
            $crate::csr::native_registers!(@write $options $wo_name);
        )*
    };
    (@identity $name:ident: $value:ty = $number:expr) => {
        pub(crate) enum $name {}
        $crate::csr::identity!($name, $number, $value);
    };
    (@write $options:tt $name:ident) => {
        impl $crate::csr::Writable for $name {
            #[inline]
            fn write(value: Self::Value) -> core::result::Result<(), $crate::trap::Error> {
                match () {
                    #[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
                    () => {
                        $crate::csr::native_write!("csrw", value, $options);
                        Ok(())
                    }
                    #[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64")))]
                    () => {
                        let _ = value;
                        unimplemented!("native CSR access requires a RISC-V target")
                    }
                }
            }
        }
    };
}

pub(crate) use {identity, native_read, native_registers, readable, writable};

word_value! { TriggerData; }

impl TriggerData {
    pub(crate) fn trigger_type(self) -> usize {
        self.0 >> (usize::BITS - 4)
    }
}

native_registers! { read { Mhartid: usize = 0xf14; } write {} }

registers! { read { Tdata1: TriggerData = 0x7a1; } write { Tselect: usize = 0x7a0; } }
