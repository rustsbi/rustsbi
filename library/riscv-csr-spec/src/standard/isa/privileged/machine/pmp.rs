//! Physical memory protection and security configuration.

/// Physical Memory Protection (PMP) address-matching mode (`pmpcfg*.A`).
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PmpRange {
    /// Disabled entry.
    Off = 0,
    /// Top-of-range addressing. The lower bound is zero for entry 0 and comes
    /// from the preceding address register for every other entry.
    Tor = 1,
    /// Naturally aligned four-byte region.
    Na4 = 2,
    /// Naturally aligned power-of-two region of at least eight bytes.
    Napot = 3,
}

/// Physical Memory Protection (PMP) permission bits (`pmpcfg*.XWR`).
///
/// The names describe the base-PMP encoding. With `mseccfg.MML = 1`,
/// effective permissions depend on both `XWR` and `L`. For `XWR = 010` or `110`,
/// `L = 0` selects a shared data region and `L = 1` selects a shared code region.
/// [Smepmp permission table](https://docs.riscv.org/reference/isa/v20260120/priv/smepmp.html#5-1-2-smepmp-physical-memory-protection-rules).
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PmpPermission {
    /// No permissions in base PMP.
    None = 0,
    /// Read permission.
    R = 1,
    /// Reserved write-only encoding in base PMP.
    W = 2,
    /// Read and write permission.
    Rw = 3,
    /// Execute permission.
    X = 4,
    /// Read and execute permission.
    Rx = 5,
    /// Reserved write-and-execute encoding in base PMP.
    Wx = 6,
    /// Read, write, and execute permission.
    Rwx = 7,
}

/// One eight-bit Physical Memory Protection configuration field (`pmpcfg*` byte).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PmpEntry {
    /// Address-matching mode (`A`).
    pub range: PmpRange,
    /// Permission bits (`XWR`).
    pub permission: PmpPermission,
    /// Lock and M-mode enforcement bit (`L`).
    pub locked: bool,
}

impl PmpEntry {
    /// Decodes a PMP configuration byte, discarding reserved bits 6:5.
    #[inline(always)]
    pub const fn from_bits_truncate(bits: u8) -> Self {
        let range = match (bits >> 3) & 3 {
            0 => PmpRange::Off,
            1 => PmpRange::Tor,
            2 => PmpRange::Na4,
            _ => PmpRange::Napot,
        };
        let permission = match bits & 7 {
            0 => PmpPermission::None,
            1 => PmpPermission::R,
            2 => PmpPermission::W,
            3 => PmpPermission::Rw,
            4 => PmpPermission::X,
            5 => PmpPermission::Rx,
            6 => PmpPermission::Wx,
            _ => PmpPermission::Rwx,
        };
        Self {
            range,
            permission,
            locked: (bits & 0x80) != 0,
        }
    }

    /// Encodes into an 8-bit PMP configuration byte (reserved bits 6:5 zeroed).
    #[inline(always)]
    pub const fn bits(self) -> u8 {
        ((self.range as u8) << 3) | (self.permission as u8) | ((self.locked as u8) << 7)
    }
}

define_csr! {
    /// Machine security configuration (`mseccfg` / RV32 `mseccfgh`, `Smepmp`/`Zkr`).
    ///
    /// This API exposes an atomic set of `SSEED`. The [sticky `MML` and
    /// `MMWP` bits](https://docs.riscv.org/reference/isa/v20260120/priv/machine.html#sec:mseccfg)
    /// can be cleared only by a PMP reset once set.
    ///
    /// # Access requirements
    ///
    /// Setting [`SSEED`](https://docs.riscv.org/reference/isa/v20260120/unpriv/scalar-crypto.html#crypto_scalar_es_access)
    /// requires M-mode on a hart implementing Zkr and grants S/HS-mode access to the entropy source.
    Mseccfg { (0x747, 0x757), r, w(Set) } {
        /// Machine-mode PMP lockdown.
        MML[0] { r: bool }
        /// Machine-mode PMP whitelist policy.
        MMWP[1] { r: bool }
        /// Rule-locking bypass.
        RLB[2] { r: bool }
        /// U-mode permission to poll `seed`.
        USEED[8] { r: bool }
        /// S/HS-mode permission to poll `seed`.
        SSEED[9] { r: bool, bit }
    }
}

define_indexed_csr! {
    /// The configuration word for a group of PMP entries.
    ///
    /// # Access requirements
    ///
    /// Each word covers four entries on RV32 or eight on RV64.
    /// RV64 implements only even register indices.
    /// Preserve the other entries when changing one. Setting `L` can lock an entry until reset.
    Pmpcfg<INDEX> {
        index: 0..=15, base: 0x3a0,
        width: if INDEX.is_multiple_of(2) { crate::Width::Xlen } else { crate::Width::Rv32Only },
        r: usize, w: usize
    }
}

define_indexed_csr! {
    /// PMP address word (`pmpaddr0..=63`).
    ///
    /// # Access requirements
    ///
    /// Changing an address can affect its own entry and the following TOR
    /// entry. Keep both regions valid for accesses throughout the update.
    Pmpaddr<INDEX> {
        index: 0..=63, base: 0x3b0, width: crate::Width::Xlen,
        r: usize, w: usize
    }
}
