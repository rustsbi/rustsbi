use XReg::*;
/// This example illustrates how we use non-usize SBI register values on emulators.
use core::ops::ControlFlow;
use sbi_spec::binary::SbiRet;

/// We take RISC-V RV128I as an example, as by now (2025 CE) there are nearly no common host
/// platforms that supports 128-bit pointer width. It will illustrate how we write an emulator
/// with pointer width different than the host platform.
///
/// This emulator starts at S-mode, allowing the emulated supervisor software to call via
/// the Supervisor Binary Interface (SBI) by using the `ecall` instruction.
pub struct SimpleRv128IHart {
    xregs: [u128; 32],
    pc: u128,
    inst_memory: InstMemory<0x2000_0000, { 0x4000 / 4 }>,
}

// Run the emulator.
fn main() {
    let mut memory = InstMemory::new_unimp();
    // Call SBI probe_extension to probe the BASE extension which always exists
    memory.li(0x0, A0, 0x10);
    memory.li(0x4, A6, 3);
    memory.li(0x8, A7, 0x10);
    memory.ecall(0xC);
    // Call SBI system_reset to shutdown emulation
    memory.li(0x10, A0, 0x0);
    memory.li(0x14, A1, 0x0);
    memory.li(0x18, A6, 0x0);
    memory.li(0x1C, A7, 0x53525354);
    memory.ecall(0x24);

    let mut hart = SimpleRv128IHart::new(memory);

    let emulation_result = loop {
        match hart.stepi() {
            Ok(()) => {}
            Err(Exception::SupervisorEcall) => match machine_firmware_handle_ecall(&mut hart) {
                ControlFlow::Break(value) => break value,
                ControlFlow::Continue(()) => continue,
            },
            Err(e) => {
                println!("Emulation failed for unexpected exception: {:?}", e);
                return;
            }
        }
    };

    println!("Emulation success! Result: {}", emulation_result);
}

/* -- Implementations of SimpleRv128Platform -- */

pub struct InstMemory<const BASE: usize, const N_INSNS: usize> {
    inner: [u32; N_INSNS],
}

const OPCODE_OP_IMM: u32 = 0b001_0011;
const OPCODE_LUI: u32 = 0b011_0111;
const FUNCT3_OP_ADD_SUB: u32 = 0b000;

impl<const BASE: usize, const N_INSNS: usize> InstMemory<BASE, N_INSNS> {
    pub fn new_unimp() -> Self {
        Self {
            inner: [0; N_INSNS],
        }
    }

    pub fn addi(&mut self, offset: usize, rd: XReg, rs: XReg, simm12: impl Into<Simm12>) {
        let funct3 = FUNCT3_OP_ADD_SUB;
        let opcode = OPCODE_OP_IMM;
        let word = (u32::from(simm12.into().0) << 20)
            | ((rs as u32) << 15)
            | (funct3 << 12)
            | ((rd as u32) << 7)
            | opcode;
        self.inner[offset / 4] = word;
    }

    pub fn lui(&mut self, offset: usize, rd: XReg, simm20: impl Into<Simm20>) {
        let opcode = OPCODE_LUI;
        let word = (u32::from(simm20.into().0) << 12) | ((rd as u32) << 7) | opcode;
        self.inner[offset / 4] = word;
    }

    pub fn li(&mut self, offset: usize, rd: XReg, imm: u32) {
        let simm20 = (imm >> 12) & 0xFFFFF;
        let simm12 = imm & 0xFFF;
        if simm20 != 0 {
            self.lui(offset, rd, simm20);
            self.addi(offset + 0x4, rd, rd, simm12);
        } else {
            self.addi(offset, rd, XReg::Zero, simm12);
        }
    }

    pub fn ecall(&mut self, offset: usize) {
        let word = 0b000000000000_00000_000_00000_1110011;
        self.inner[offset / 4] = word;
    }

    pub fn get(&mut self, ptr: u128) -> Option<u32> {
        if ptr % 4 != 0 || ptr >= (BASE + 4 * N_INSNS) as u128 {
            return None;
        }
        Some(self.inner[(ptr as usize - BASE) / 4])
    }
}

impl SimpleRv128IHart {
    pub fn new(inst_memory: InstMemory<0x2000_0000, { 0x4000 / 4 }>) -> Self {
        Self {
            xregs: [0; 32],
            pc: 0x2000_0000,
            inst_memory,
        }
    }
    pub fn stepi(&mut self) -> Result<(), Exception> {
        let raw_insn = self
            .inst_memory
            .get(self.pc)
            .ok_or(Exception::InstructionAccessFault)?;

        println!("Raw insn at 0x{:x?} is 0x{:x?}", self.pc, raw_insn);

        let parsed_insn =
            Instruction::try_from(raw_insn).map_err(|_| Exception::IllegalInstruction)?;

        match parsed_insn {
            Instruction::Addi(rd, rs, simm12) => {
                self.xregs[rd as usize] = self.xregs[rs as usize] + simm12.0 as u128;
                self.pc = self.pc.wrapping_add(4);
            }
            Instruction::Lui(rd, simm20) => {
                self.xregs[rd as usize] = (simm20.0 as u128) << 12;
                self.pc = self.pc.wrapping_add(4);
            }
            Instruction::Ecall => return Err(Exception::SupervisorEcall),
        }

        Ok(())
    }
}

#[derive(Debug)]
pub enum Exception {
    IllegalInstruction,
    InstructionAccessFault,
    SupervisorEcall,
}

#[derive(Debug)]
pub enum Instruction {
    Addi(XReg, XReg, Simm12),
    Lui(XReg, Simm20),
    Ecall,
}

impl TryFrom<u32> for Instruction {
    type Error = ();

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        let opcode = value & 0x7F;
        let rd = ((value >> 7) & 0x1F).try_into().unwrap();
        let rs1 = ((value >> 15) & 0x1F).try_into().unwrap();
        let funct3 = (value >> 12) & 0b111;
        let simm12 = (value >> 20).into();
        let simm20 = (value >> 12).into();
        if opcode == OPCODE_OP_IMM && funct3 == FUNCT3_OP_ADD_SUB {
            Ok(Self::Addi(rd, rs1, simm12))
        } else if opcode == OPCODE_LUI {
            Ok(Self::Lui(rd, simm20))
        } else if value == 0b000000000000_00000_000_00000_1110011 {
            Ok(Self::Ecall)
        } else {
            Err(())
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub enum XReg {
    Zero = 0,
    Ra = 1,
    Sp = 2,
    Gp = 3,
    Tp = 4,
    T0 = 5,
    T1 = 6,
    T2 = 7,
    S0 = 8,
    S1 = 9,
    A0 = 10,
    A1 = 11,
    A2 = 12,
    A3 = 13,
    A4 = 14,
    A5 = 15,
    A6 = 16,
    A7 = 17,
    S2 = 18,
    S3 = 19,
    S4 = 20,
    S5 = 21,
    S6 = 22,
    S7 = 23,
    S8 = 24,
    S9 = 25,
    S10 = 26,
    S11 = 27,
    T3 = 28,
    T4 = 29,
    T5 = 30,
    T6 = 31,
}

impl TryFrom<u32> for XReg {
    type Error = ();

    fn try_from(value: u32) -> Result<Self, Self::Error> {
        Ok(match value {
            0 => Zero,
            1 => Ra,
            2 => Sp,
            3 => Gp,
            4 => Tp,
            5 => T0,
            6 => T1,
            7 => T2,
            8 => S0,
            9 => S1,
            10 => A0,
            11 => A1,
            12 => A2,
            13 => A3,
            14 => A4,
            15 => A5,
            16 => A6,
            17 => A7,
            18 => S2,
            19 => S3,
            20 => S4,
            21 => S5,
            22 => S6,
            23 => S7,
            24 => S8,
            25 => S9,
            26 => S10,
            27 => S11,
            28 => T3,
            29 => T4,
            30 => T5,
            31 => T6,
            _ => return Err(()),
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Simm12(u16);

impl From<u32> for Simm12 {
    fn from(value: u32) -> Self {
        Self((value & 0x0FFF) as u16)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Simm20(u32);

impl From<u32> for Simm20 {
    fn from(value: u32) -> Self {
        Self(value & 0xFFFFF)
    }
}

// A simple SBI implementation without using RustSBI.

fn machine_firmware_handle_ecall(hart: &mut SimpleRv128IHart) -> ControlFlow<u128> {
    println!("Handle ecall, registers: {:x?}", hart.xregs);
    let param = [
        hart.xregs[A0 as usize],
        hart.xregs[A1 as usize],
        hart.xregs[A2 as usize],
        hart.xregs[A3 as usize],
        hart.xregs[A4 as usize],
        hart.xregs[A5 as usize],
    ];
    let ret = handle_sbi_call(hart.xregs[A7 as usize], hart.xregs[A6 as usize], param);
    println!("SbiRet: {:?}", ret);
    if ret.error == 0x114514 {
        return ControlFlow::Break(ret.value);
    }
    hart.xregs[A0 as usize] = ret.error;
    hart.xregs[A1 as usize] = ret.value;
    hart.pc = hart.pc.wrapping_add(4);
    ControlFlow::Continue(())
}

fn handle_sbi_call(extension: u128, function: u128, param: [u128; 6]) -> SbiRet<u128> {
    match (extension, function) {
        // BASE probe_extension
        (0x10, 3) => {
            if param[0] == 0x10 {
                SbiRet::success(1)
            } else {
                SbiRet::success(0)
            }
        }
        // SRST system_reset
        (0x53525354, 0) => {
            let (reset_type, reset_reason) = (param[0], param[1]);
            if reset_type == sbi_spec::srst::RESET_TYPE_SHUTDOWN as u128 {
                // special SBI error value for platform shutdown
                SbiRet {
                    value: reset_reason,
                    error: 0x114514,
                }
            } else {
                SbiRet::not_supported()
            }
        }
        _ => SbiRet::not_supported(),
    }
}
