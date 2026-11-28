/// The mode of a CSR snapshot read from hardware or decoded from raw bits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadMode {}

/// The mode of a local CSR value prepared for a hardware write.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriteMode {}

pub(crate) mod sealed {
    pub trait Sealed {}
    pub trait WriteAllowed<V>: Sealed {}
}

pub(crate) trait ScalarCodec: Copy {
    fn from_raw_u64(raw: u64) -> Self;
    fn into_raw_u64(self) -> u64;
}

impl ScalarCodec for usize {
    #[inline(always)]
    fn from_raw_u64(raw: u64) -> Self {
        raw as usize
    }

    #[inline(always)]
    fn into_raw_u64(self) -> u64 {
        self as u64
    }
}

impl ScalarCodec for u32 {
    #[inline(always)]
    fn from_raw_u64(raw: u64) -> Self {
        raw as u32
    }

    #[inline(always)]
    fn into_raw_u64(self) -> u64 {
        self as u64
    }
}

impl ScalarCodec for u64 {
    #[inline(always)]
    fn from_raw_u64(raw: u64) -> Self {
        raw
    }

    #[inline(always)]
    fn into_raw_u64(self) -> u64 {
        self
    }
}

/// A mask of bits to set with an atomic `CSRRS`.
///
/// The register's write rules apply even to unselected bits whose read value
/// differs from their stored value.
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Set<T>(pub T);

/// A mask of bits to clear with an atomic `CSRRC`.
///
/// The same read-modify-write rules apply as for [`Set`].
#[repr(transparent)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Clear<T>(pub T);

/// Width and RV32 high-word access protocol of an architectural CSR.
///
/// Paired variants store the RV32 high-half CSR number in `high`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Width {
    /// One CSR word on either RV32 or RV64. Its defined fields can be narrower than XLEN.
    Xlen,
    /// One CSR word defined only on RV32, such as `mstatush` or `pmpcfg1`.
    Rv32Only,
    /// Logical 64-bit configuration/state CSR backed by a single CSR on RV64 and `(low, high)` on RV32.
    Split64 {
        /// The high-half CSR number.
        high: u16,
    },
    /// 64-bit counter whose low word may roll over during an RV32 read.
    Counter64 {
        /// The high-half CSR number used to detect a rollover during a read.
        high: u16,
    },
    /// 64-bit Sstc timer comparator (`stimecmp`/`vstimecmp`).
    Timer64 {
        /// The high-half CSR number written between the temporary and final low words.
        high: u16,
    },
}

impl Width {
    /// Returns `true` if the addressed CSR exists only on RV32.
    #[inline(always)]
    pub const fn is_rv32_only(self) -> bool {
        matches!(self, Self::Rv32Only)
    }
}

/// A CSR write operation and its encoded operand.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WriteOp {
    /// A whole-register write, using the register's high-half protocol on RV32.
    Replace(u64),
    /// Atomic read-and-set `csrs` (`csrrs x0, csr, rs1`), discarding the read value.
    Set(usize),
    /// Atomic bit-clear that discards the previous value (`csrc`).
    Clear(usize),
    /// Atomic bit-clear returning previous value `csrrc rd, csr, rs1`.
    ReadAndClear(usize),
}

/// The architectural number and access width of a standard or vendor register.
pub trait Csr: sealed::Sealed {
    /// Primary architectural CSR number (`0x000..=0xfff`).
    const NUMBER: u16;
    /// Width and RV32 paired high-half access protocol.
    const WIDTH: Width = Width::Xlen;
}

/// CSR read capability and raw-value decoding.
///
/// A `csrr` reads the register with `rs1 = x0`, suppressing the write.
/// Vendor CSRs can define side effects for such reads.
/// The Zkr entropy source requires a write and implements [`ClaimSpec`].
pub trait ReadSpec: Csr {
    /// Value type decoded from raw CSR bits.
    type Value;
    /// Decodes raw CSR bits according to the associated value's width and encoding.
    fn from_raw(raw: u64) -> Self::Value;
}

/// A standard CSR that permits a pure read without read side effects.
pub trait StandardRead: ReadSpec {}

/// CSR write capability parameterized by the permitted operand type `V`.
///
/// Each implementation specifies one permitted register and operand combination.
/// A hardware write also requires the privilege, extensions, legal field values,
/// and ownership described for the register.
pub trait WriteSpec<V>: Csr + sealed::WriteAllowed<V> {
    /// Return type produced by executing this write operation.
    type Output;
    /// The CSR number targeted by an atomic bit operation.
    ///
    /// On RV32, fields in bits 63:32 of a paired register target its high-half CSR.
    const ATOMIC_NUMBER: u16 = Self::NUMBER;
    /// Encodes the operation and operand for a hardware write.
    fn encode_write(value: V) -> WriteOp;
    /// Decodes the readback register value when executing [`WriteOp::ReadAndClear`].
    fn decode_output(raw: usize) -> Self::Output;
}

/// CSR read/write-zero capability using `csrrw rd, csr, x0`.
///
/// An IMSIC `*topei` claim clears the returned identity's pending bit, if any.
/// Polling Zkr `seed` consumes an entropy word only for an ES16 result.
pub trait ClaimSpec: Csr {
    /// Decoded claimed register value.
    type Value;
    /// Wraps the raw `csrrw rd, csr, x0` result into [`Self::Value`].
    fn from_claim_raw(raw: usize) -> Self::Value;
}
