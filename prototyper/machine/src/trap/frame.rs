//! Single source of truth for the machine trap-frame layout.

use super::Cause;

/// Explicit architectural integer-register layout used by entry and restore.
#[repr(C)]
struct Registers {
    x0: usize,
    x1: usize,
    x2: usize,
    x3: usize,
    x4: usize,
    x5: usize,
    x6: usize,
    x7: usize,
    x8: usize,
    x9: usize,
    x10: usize,
    x11: usize,
    x12: usize,
    x13: usize,
    x14: usize,
    x15: usize,
    x16: usize,
    x17: usize,
    x18: usize,
    x19: usize,
    x20: usize,
    x21: usize,
    x22: usize,
    x23: usize,
    x24: usize,
    x25: usize,
    x26: usize,
    x27: usize,
    x28: usize,
    x29: usize,
    x30: usize,
    x31: usize,
}

/// Complete integer and machine-CSR state initialized on every trap entry.
///
/// `mstatus_high`, `tval2`, `tinst`, and `gva` remain XLEN-sized on every
/// target. Entry writes zero when a field is architecturally inapplicable.
/// `previous` is an opaque stack-relative link, never a Rust pointer.
#[repr(C, align(16))]
pub(super) struct Frame {
    registers: Registers,
    mepc: usize,
    mstatus: usize,
    mstatus_high: usize,
    cause: usize,
    tval: usize,
    tval2: usize,
    tinst: usize,
    gva: usize,
    previous: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct HypervisorTrap {
    pub(super) virtualized: bool,
    pub(super) previous_supervisor: bool,
    pub(super) guest_address: bool,
    pub(super) value2: usize,
    pub(super) instruction: usize,
}

impl Frame {
    pub(super) fn cause(&self) -> Cause {
        let interrupt_bit = 1usize << (usize::BITS - 1);
        let number = self.cause & !interrupt_bit;
        if self.cause & interrupt_bit != 0 {
            return match number {
                3 => Cause::MachineSoftwareInterrupt,
                7 => Cause::MachineTimerInterrupt,
                11 => Cause::MachineExternalInterrupt,
                _ => Cause::Other,
            };
        }

        match number {
            2 => Cause::IllegalInstruction,
            4 => Cause::LoadMisaligned,
            6 => Cause::StoreMisaligned,
            9 if self.previous_mode() == 1 => Cause::SbiCall {
                extension_id: self.register(17).expect("a7 is a valid register"),
                function_id: self.register(16).expect("a6 is a valid register"),
                arguments: [
                    self.register(10).expect("a0 is a valid register"),
                    self.register(11).expect("a1 is a valid register"),
                    self.register(12).expect("a2 is a valid register"),
                    self.register(13).expect("a3 is a valid register"),
                    self.register(14).expect("a4 is a valid register"),
                    self.register(15).expect("a5 is a valid register"),
                ],
            },
            _ => Cause::Other,
        }
    }

    pub(super) fn register(&self, number: usize) -> Option<usize> {
        let base = &self.registers as *const Registers as *const usize;
        if number >= 32 {
            return None;
        }
        // SAFETY: `Registers` is C-layout, consists of exactly 32 consecutive
        // `usize` fields, and `number` was checked against that bound.
        Some(unsafe { base.add(number).read() })
    }

    pub(super) fn set_register(&mut self, number: usize, value: usize) -> bool {
        let base = &mut self.registers as *mut Registers as *mut usize;
        if number == 0 {
            return true;
        }
        if number >= 32 {
            return false;
        }
        // SAFETY: the same checked C-layout argument as `register` applies,
        // and the unique frame borrow permits mutation of this one field.
        unsafe { base.add(number).write(value) };
        true
    }

    pub(super) fn advance_pc(&mut self, bytes: usize) -> bool {
        let Some(next) = self.mepc.checked_add(bytes) else {
            return false;
        };
        self.mepc = next;
        true
    }

    pub(super) fn previous_mode(&self) -> usize {
        (self.mstatus >> 11) & 0b11
    }

    fn previous_virtual(&self) -> bool {
        #[cfg(target_pointer_width = "64")]
        {
            self.mstatus & (1usize << 39) != 0
        }
        #[cfg(target_pointer_width = "32")]
        {
            self.mstatus_high & (1usize << 7) != 0
        }
    }

    pub(super) fn hypervisor_trap(&self) -> Option<HypervisorTrap> {
        if self.gva > 1 || self.previous_mode() > 1 {
            return None;
        }
        Some(HypervisorTrap {
            virtualized: self.previous_virtual(),
            previous_supervisor: self.previous_mode() == 1,
            guest_address: self.gva != 0,
            value2: self.tval2,
            instruction: self.tinst,
        })
    }

    pub(super) const fn pc(&self) -> usize {
        self.mepc
    }

    pub(super) const fn encoded_cause(&self) -> usize {
        self.cause
    }

    pub(super) const fn trap_value(&self) -> usize {
        self.tval
    }

    pub(super) fn redirect_to_supervisor(&mut self, entry: usize) -> bool {
        const MSTATUS_SIE: usize = 1 << 1;
        const MSTATUS_SPIE: usize = 1 << 5;
        const MSTATUS_SPP: usize = 1 << 8;
        const MSTATUS_MPP: usize = 0b11 << 11;
        const SUPERVISOR: usize = 1;

        let previous = self.previous_mode();
        if previous > SUPERVISOR || entry == 0 || !entry.is_multiple_of(4) {
            return false;
        }

        // Architecturally reproduce the supervisor trap-state stack before
        // the private restore performs the one machine return.
        if self.mstatus & MSTATUS_SIE != 0 {
            self.mstatus |= MSTATUS_SPIE;
        } else {
            self.mstatus &= !MSTATUS_SPIE;
        }
        self.mstatus &= !MSTATUS_SIE;
        if previous == SUPERVISOR {
            self.mstatus |= MSTATUS_SPP;
        } else {
            self.mstatus &= !MSTATUS_SPP;
        }
        self.mstatus = (self.mstatus & !MSTATUS_MPP) | (SUPERVISOR << 11);
        // A software redirection always enters HS/S mode first. Retaining MPV
        // would make the final MRET skip the supervisor trap handler and enter
        // the interrupted virtual context directly.
        #[cfg(target_pointer_width = "64")]
        {
            self.mstatus &= !(1usize << 39);
        }
        #[cfg(target_pointer_width = "32")]
        {
            self.mstatus_high &= !(1usize << 7);
        }
        self.mepc = entry;
        true
    }

    pub(super) fn commit_instruction(&mut self, register: usize, value: usize) -> bool {
        if register >= 32 {
            return false;
        }
        let Some(next_pc) = self.mepc.checked_add(4) else {
            return false;
        };
        if !self.set_register(register, value) {
            return false;
        }
        self.mepc = next_pc;
        true
    }

    #[cfg(test)]
    pub(super) fn test_frame(mepc: usize, cause: usize, tval: usize) -> Self {
        Self {
            registers: Registers {
                x0: 0,
                x1: 0,
                x2: 0,
                x3: 0,
                x4: 0,
                x5: 0,
                x6: 0,
                x7: 0,
                x8: 0,
                x9: 0,
                x10: 0,
                x11: 0,
                x12: 0,
                x13: 0,
                x14: 0,
                x15: 0,
                x16: 0,
                x17: 0,
                x18: 0,
                x19: 0,
                x20: 0,
                x21: 0,
                x22: 0,
                x23: 0,
                x24: 0,
                x25: 0,
                x26: 0,
                x27: 0,
                x28: 0,
                x29: 0,
                x30: 0,
                x31: 0,
            },
            mepc,
            mstatus: 0,
            mstatus_high: 0,
            cause,
            tval,
            tval2: 0,
            tinst: 0,
            gva: 0,
            previous: 0,
        }
    }

    #[cfg(test)]
    pub(super) fn test_set_previous_mode(&mut self, mode: usize) {
        self.mstatus = (self.mstatus & !(0b11 << 11)) | ((mode & 0b11) << 11);
    }

    #[cfg(test)]
    pub(super) fn test_set_hypervisor_trap(
        &mut self,
        virtualized: bool,
        guest_address: usize,
        value2: usize,
        instruction: usize,
    ) {
        #[cfg(target_pointer_width = "64")]
        {
            self.mstatus = (self.mstatus & !(1usize << 39)) | (usize::from(virtualized) << 39);
        }
        #[cfg(target_pointer_width = "32")]
        {
            self.mstatus_high =
                (self.mstatus_high & !(1usize << 7)) | (usize::from(virtualized) << 7);
        }
        self.gva = guest_address;
        self.tval2 = value2;
        self.tinst = instruction;
    }
}

pub(super) const FRAME_SIZE: usize = core::mem::size_of::<Frame>();
pub(super) const FRAME_ALIGN: usize = core::mem::align_of::<Frame>();
pub(super) const REGISTERS_OFFSET: usize = core::mem::offset_of!(Frame, registers);
pub(super) const REGISTER_OFFSETS: [usize; 32] = [
    REGISTERS_OFFSET + core::mem::offset_of!(Registers, x0),
    REGISTERS_OFFSET + core::mem::offset_of!(Registers, x1),
    REGISTERS_OFFSET + core::mem::offset_of!(Registers, x2),
    REGISTERS_OFFSET + core::mem::offset_of!(Registers, x3),
    REGISTERS_OFFSET + core::mem::offset_of!(Registers, x4),
    REGISTERS_OFFSET + core::mem::offset_of!(Registers, x5),
    REGISTERS_OFFSET + core::mem::offset_of!(Registers, x6),
    REGISTERS_OFFSET + core::mem::offset_of!(Registers, x7),
    REGISTERS_OFFSET + core::mem::offset_of!(Registers, x8),
    REGISTERS_OFFSET + core::mem::offset_of!(Registers, x9),
    REGISTERS_OFFSET + core::mem::offset_of!(Registers, x10),
    REGISTERS_OFFSET + core::mem::offset_of!(Registers, x11),
    REGISTERS_OFFSET + core::mem::offset_of!(Registers, x12),
    REGISTERS_OFFSET + core::mem::offset_of!(Registers, x13),
    REGISTERS_OFFSET + core::mem::offset_of!(Registers, x14),
    REGISTERS_OFFSET + core::mem::offset_of!(Registers, x15),
    REGISTERS_OFFSET + core::mem::offset_of!(Registers, x16),
    REGISTERS_OFFSET + core::mem::offset_of!(Registers, x17),
    REGISTERS_OFFSET + core::mem::offset_of!(Registers, x18),
    REGISTERS_OFFSET + core::mem::offset_of!(Registers, x19),
    REGISTERS_OFFSET + core::mem::offset_of!(Registers, x20),
    REGISTERS_OFFSET + core::mem::offset_of!(Registers, x21),
    REGISTERS_OFFSET + core::mem::offset_of!(Registers, x22),
    REGISTERS_OFFSET + core::mem::offset_of!(Registers, x23),
    REGISTERS_OFFSET + core::mem::offset_of!(Registers, x24),
    REGISTERS_OFFSET + core::mem::offset_of!(Registers, x25),
    REGISTERS_OFFSET + core::mem::offset_of!(Registers, x26),
    REGISTERS_OFFSET + core::mem::offset_of!(Registers, x27),
    REGISTERS_OFFSET + core::mem::offset_of!(Registers, x28),
    REGISTERS_OFFSET + core::mem::offset_of!(Registers, x29),
    REGISTERS_OFFSET + core::mem::offset_of!(Registers, x30),
    REGISTERS_OFFSET + core::mem::offset_of!(Registers, x31),
];
#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
mod assembly_register_offsets {
    use super::REGISTER_OFFSETS;

    macro_rules! offsets {
        ($($name:ident = $index:literal),+ $(,)?) => {
            $(pub(crate) const $name: usize = REGISTER_OFFSETS[$index];)+
        };
    }

    offsets!(
        X0_OFFSET = 0,
        X1_OFFSET = 1,
        X2_OFFSET = 2,
        X3_OFFSET = 3,
        X4_OFFSET = 4,
        X5_OFFSET = 5,
        X6_OFFSET = 6,
        X7_OFFSET = 7,
        X8_OFFSET = 8,
        X9_OFFSET = 9,
        X10_OFFSET = 10,
        X11_OFFSET = 11,
        X12_OFFSET = 12,
        X13_OFFSET = 13,
        X14_OFFSET = 14,
        X15_OFFSET = 15,
        X16_OFFSET = 16,
        X17_OFFSET = 17,
        X18_OFFSET = 18,
        X19_OFFSET = 19,
        X20_OFFSET = 20,
        X21_OFFSET = 21,
        X22_OFFSET = 22,
        X23_OFFSET = 23,
        X24_OFFSET = 24,
        X25_OFFSET = 25,
        X26_OFFSET = 26,
        X27_OFFSET = 27,
        X28_OFFSET = 28,
        X29_OFFSET = 29,
        X30_OFFSET = 30,
        X31_OFFSET = 31,
    );
}

#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
pub(super) use assembly_register_offsets::*;
pub(super) const MEPC_OFFSET: usize = core::mem::offset_of!(Frame, mepc);
pub(super) const MSTATUS_OFFSET: usize = core::mem::offset_of!(Frame, mstatus);
pub(super) const MSTATUS_HIGH_OFFSET: usize = core::mem::offset_of!(Frame, mstatus_high);
pub(super) const CAUSE_OFFSET: usize = core::mem::offset_of!(Frame, cause);
pub(super) const TVAL_OFFSET: usize = core::mem::offset_of!(Frame, tval);
pub(super) const TVAL2_OFFSET: usize = core::mem::offset_of!(Frame, tval2);
pub(super) const TINST_OFFSET: usize = core::mem::offset_of!(Frame, tinst);
pub(super) const GVA_OFFSET: usize = core::mem::offset_of!(Frame, gva);
pub(super) const PREVIOUS_OFFSET: usize = core::mem::offset_of!(Frame, previous);

const _: () = assert!(REGISTERS_OFFSET == 0);
const _: () = assert!(core::mem::size_of::<Registers>() == 32 * core::mem::size_of::<usize>());
const _: () = {
    let mut register = 0;
    while register < REGISTER_OFFSETS.len() {
        assert!(REGISTER_OFFSETS[register] == register * core::mem::size_of::<usize>());
        register += 1;
    }
};
const _: () = assert!(FRAME_ALIGN == 16);
const _: () = assert!(MEPC_OFFSET == 32 * core::mem::size_of::<usize>());
const _: () = assert!(MSTATUS_OFFSET == MEPC_OFFSET + core::mem::size_of::<usize>());
const _: () = assert!(MSTATUS_HIGH_OFFSET == MSTATUS_OFFSET + core::mem::size_of::<usize>());
const _: () = assert!(CAUSE_OFFSET == MSTATUS_HIGH_OFFSET + core::mem::size_of::<usize>());
const _: () = assert!(TVAL_OFFSET == CAUSE_OFFSET + core::mem::size_of::<usize>());
const _: () = assert!(TVAL2_OFFSET == TVAL_OFFSET + core::mem::size_of::<usize>());
const _: () = assert!(TINST_OFFSET == TVAL2_OFFSET + core::mem::size_of::<usize>());
const _: () = assert!(GVA_OFFSET == TINST_OFFSET + core::mem::size_of::<usize>());
const _: () = assert!(PREVIOUS_OFFSET == GVA_OFFSET + core::mem::size_of::<usize>());
const _: () = assert!(FRAME_SIZE >= PREVIOUS_OFFSET + core::mem::size_of::<usize>());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_has_the_selected_word_offsets() {
        let word = core::mem::size_of::<usize>();
        assert_eq!(FRAME_ALIGN, 16);
        for (register, offset) in REGISTER_OFFSETS.into_iter().enumerate() {
            assert_eq!(offset, register * word);
        }
        assert_eq!(MEPC_OFFSET, 32 * word);
        assert_eq!(MSTATUS_OFFSET, 33 * word);
        assert_eq!(MSTATUS_HIGH_OFFSET, 34 * word);
        assert_eq!(CAUSE_OFFSET, 35 * word);
        assert_eq!(TVAL_OFFSET, 36 * word);
        assert_eq!(TVAL2_OFFSET, 37 * word);
        assert_eq!(TINST_OFFSET, 38 * word);
        assert_eq!(GVA_OFFSET, 39 * word);
        assert_eq!(PREVIOUS_OFFSET, 40 * word);
        assert_eq!(FRAME_SIZE, (41 * word).next_multiple_of(FRAME_ALIGN));
    }

    #[test]
    fn redirect_builds_the_supervisor_trap_status_stack() {
        const SIE: usize = 1 << 1;
        const SPIE: usize = 1 << 5;
        const SPP: usize = 1 << 8;
        let mut frame = Frame::test_frame(0x8020_0000, 5, 0x1234);
        frame.mstatus = SIE;
        assert!(frame.redirect_to_supervisor(0x8040_0000));
        assert_eq!(frame.mepc, 0x8040_0000);
        assert_eq!(frame.previous_mode(), 1);
        assert_eq!(frame.mstatus & SIE, 0);
        assert_ne!(frame.mstatus & SPIE, 0);
        assert_eq!(frame.mstatus & SPP, 0);

        let mut machine = Frame::test_frame(0x8020_0000, 5, 0);
        machine.test_set_previous_mode(3);
        assert!(!machine.redirect_to_supervisor(0x8040_0000));
        assert_eq!(machine.mepc, 0x8020_0000);
    }

    #[test]
    fn virtual_trap_metadata_is_captured_before_hs_redirection() {
        let mut frame = Frame::test_frame(0x8020_0000, 21, 0x1234);
        frame.test_set_previous_mode(1);
        frame.test_set_hypervisor_trap(true, 1, 0x4000, 0x2081);
        assert_eq!(
            frame.hypervisor_trap(),
            Some(HypervisorTrap {
                virtualized: true,
                previous_supervisor: true,
                guest_address: true,
                value2: 0x4000,
                instruction: 0x2081,
            })
        );

        assert!(frame.redirect_to_supervisor(0x8040_0000));
        assert!(!frame.previous_virtual());
        assert_eq!(frame.previous_mode(), 1);
    }

    #[test]
    fn invalid_gva_encoding_cannot_reach_hypervisor_csrs() {
        let mut frame = Frame::test_frame(0x8020_0000, 21, 0x1234);
        frame.test_set_previous_mode(1);
        frame.test_set_hypervisor_trap(true, 2, 0, 0);
        assert_eq!(frame.hypervisor_trap(), None);
    }
}
