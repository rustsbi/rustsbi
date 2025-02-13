/// This example illustrates how to use non-usize SBI register values on emulators.
///
/// We take RISC-V RV128I as an example, since as of now (2025 CE) there are almost no common host
/// platforms that support 128-bit pointer width. This example shows how to write an emulator whose
/// pointer width differs from that of the host platform.
///
/// The emulator starts in S-mode, allowing the emulated supervisor software to make SBI calls via
/// the `ecall` instruction.
use XReg::*;
use core::ops::ControlFlow;
use sbi_spec::binary::SbiRet;

/// Represents a simple RV128I hardware thread (hart) emulator.
///
/// The hart contains:
/// - A set of 32 general-purpose registers (each 128-bit wide)
/// - A program counter (PC) also 128-bit wide
/// - An instruction memory that holds the binary instructions to execute
pub struct SimpleRv128IHart {
    xregs: [u128; 32],
    pc: u128,
    inst_memory: InstMemory<0x2000_0000, { 0x4000 / 4 }>,
}

/// The main function that sets up the instruction memory, runs the emulator, and prints the result.
fn main() {
    // Create a new instruction memory with all instructions unimplemented (zeroed out)
    let mut memory = InstMemory::new_unimp();

    // --- Build a simple program in instruction memory ---
    // Call SBI probe_extension to probe the BASE extension which always exists
    memory.li(0x0, A0, 0x10);
    memory.li(0x4, A6, 3);
    memory.li(0x8, A7, 0x10);
    memory.ecall(0xC);
    // Judge if the SBI call result is zero; if non-zero, jump to emulation failed.
    memory.beqz(0x10, A1, 0x1C);
    // Emulation success, call SBI system_reset to shutdown emulation
    memory.li(0x14, A0, 0x0);
    memory.li(0x18, A1, 0x0);
    memory.li(0x1C, A6, 0x0);
    memory.li(0x20, A7, 0x53525354);
    memory.ecall(0x28);
    // Emulation failed, call SBI system_reset with SYSTEM_FAILURE to shutdown emulation
    memory.li(0x2C, A0, 0x0);
    memory.li(0x30, A1, 0x1);
    memory.li(0x34, A6, 0x0);
    memory.li(0x38, A7, 0x53525354);
    memory.ecall(0x44);

    // --- Initialize the emulator hart ---
    let mut hart = SimpleRv128IHart::new(memory);
    println!("Starting SimpleRv128IHart...");

    // Run the emulation loop, executing one instruction at a time.
    // The loop breaks when an SBI call requests to shutdown the emulator (returns a special error value).
    let emulation_result = loop {
        match hart.stepi() {
            Ok(()) => {} // Instruction executed normally; continue to the next one.
            Err(Exception::SupervisorEcall) => match handle_ecall(&mut hart) {
                // If the SBI call indicates a shutdown (with a special error value), break out of the loop.
                ControlFlow::Break(value) => break value,
                // Otherwise, continue execution.
                ControlFlow::Continue(()) => continue,
            },
            // Any other exception is considered unexpected, so we print an error and terminate.
            Err(e) => {
                println!("Emulation failed for unexpected exception: {:?}", e);
                return;
            }
        }
    };

    // Print the final result of the emulation.
    println!("Emulation finished. Result: {}", emulation_result);
    if emulation_result == 0 {
        println!("✓ Test success!");
    } else {
        println!("✗ Test failed, emulator returns {}", emulation_result);
    }
}

/// Handle an SBI call given the extension and function numbers, along with its parameters.
///
/// This is a simple SBI implementation (without using RustSBI) that supports a few functions:
/// - BASE probe_extension: checks if an SBI extension exists.
/// - SRST system_reset: performs a system reset (shutdown).
///
/// Note that the returned `SbiRet<u128>` represents an `SbiRet` with `u128` as the SBI register
/// type.
///
/// # Parameters
/// - `extension`: The SBI extension identifier (from register A7).
/// - `function`: The SBI function number (from register A6).
/// - `param`: An array containing SBI call parameters (from registers A0-A5).
///
/// # Returns
/// An `SbiRet` structure containing the error and return values.
fn handle_sbi_call(extension: u128, function: u128, param: [u128; 6]) -> SbiRet<u128> {
    match (extension, function) {
        // BASE probe_extension: if the parameter matches the BASE extension identifier, return 1.
        (0x10, 3) => {
            if param[0] == 0x10 {
                SbiRet::success(1)
            } else {
                SbiRet::success(0)
            }
        }
        // SRST system_reset: perform a system reset if the reset type is shutdown.
        (0x53525354, 0) => {
            let (reset_type, reset_reason) = (param[0], param[1]);
            if reset_type == sbi_spec::srst::RESET_TYPE_SHUTDOWN as u128 {
                // Use a special SBI error value (0x114514) to signal platform shutdown.
                SbiRet {
                    value: reset_reason,
                    error: 0x114514,
                }
            } else {
                SbiRet::not_supported()
            }
        }
        // All other SBI calls are not supported.
        _ => SbiRet::not_supported(),
    }
}

/* -- Implementations of SimpleRv128Platform -- */

/// Handle the supervisor call (ecall) exception by performing an SBI call.
///
/// This function extracts the parameters from the hart's registers, performs the SBI call, and
/// then updates the hart's registers and program counter with the results.
///
/// # Parameters
/// - `hart`: A mutable reference to the RV128I hart emulator.
///
/// # Returns
/// - `ControlFlow::Break(value)` if the SBI call indicates that the platform should shutdown.
/// - `ControlFlow::Continue(())` if the emulation should continue.
fn handle_ecall(hart: &mut SimpleRv128IHart) -> ControlFlow<u128> {
    println!("Handle ecall, registers: {:x?}", hart.xregs);
    // Extract SBI call parameters from registers A0-A5.
    let param = [
        hart.xregs[A0 as usize],
        hart.xregs[A1 as usize],
        hart.xregs[A2 as usize],
        hart.xregs[A3 as usize],
        hart.xregs[A4 as usize],
        hart.xregs[A5 as usize],
    ];
    // Call the SBI handler with the extension and function numbers from registers A7 and A6.
    let ret = handle_sbi_call(hart.xregs[A7 as usize], hart.xregs[A6 as usize], param);
    println!("SbiRet: {:?}", ret);
    // If the SBI call returns the special error value (0x114514), signal shutdown.
    if ret.error == 0x114514 {
        return ControlFlow::Break(ret.value);
    }
    // Otherwise, store the error and return values into registers A0 and A1, respectively.
    hart.xregs[A0 as usize] = ret.error;
    hart.xregs[A1 as usize] = ret.value;
    // Advance the program counter past the ecall instruction.
    hart.pc = hart.pc.wrapping_add(4);
    ControlFlow::Continue(())
}

/// An instruction memory implementation that holds a fixed number of instructions.
///
/// `BASE` defines the starting memory address, and `N_INSNS` is the number of 32-bit words.
pub struct InstMemory<const BASE: usize, const N_INSNS: usize> {
    inner: [u32; N_INSNS],
}

/// Opcode and function constant definitions for a simplified RISC-V subset.
const OPCODE_OP_IMM: u32 = 0b001_0011;
const OPCODE_LUI: u32 = 0b011_0111;
const OPCODE_BRANCH: u32 = 0b110_0011;
const FUNCT3_OP_ADD_SUB: u32 = 0b000;
const FUNCT3_BRANCH_BEQ: u32 = 0b000;

impl<const BASE: usize, const N_INSNS: usize> InstMemory<BASE, N_INSNS> {
    /// Creates a new instance of instruction memory with all instructions set to unimplemented (zero).
    pub fn new_unimp() -> Self {
        Self {
            inner: [0; N_INSNS],
        }
    }

    /// Assemble an ADDI instruction and store it at the given memory index.
    ///
    /// # Parameters
    /// - `idx`: The byte offset at which to place the instruction.
    /// - `rd`: The destination register.
    /// - `rs`: The source register.
    /// - `simm12`: The 12-bit signed immediate.
    pub fn addi(&mut self, idx: usize, rd: XReg, rs: XReg, simm12: impl Into<Simm12>) {
        let funct3 = FUNCT3_OP_ADD_SUB;
        let opcode = OPCODE_OP_IMM;
        let word = (u32::from(simm12.into().0) << 20)
            | ((rs as u32) << 15)
            | (funct3 << 12)
            | ((rd as u32) << 7)
            | opcode;
        self.inner[idx / 4] = word;
    }

    /// Assemble a LUI (Load Upper Immediate) instruction and store it at the given memory index.
    ///
    /// # Parameters
    /// - `idx`: The byte offset at which to place the instruction.
    /// - `rd`: The destination register.
    /// - `simm20`: The 20-bit immediate value.
    pub fn lui(&mut self, idx: usize, rd: XReg, simm20: impl Into<Simm20>) {
        let opcode = OPCODE_LUI;
        let word = (u32::from(simm20.into().0) << 12) | ((rd as u32) << 7) | opcode;
        self.inner[idx / 4] = word;
    }

    /// Load an immediate value into a register.
    ///
    /// This function will generate either a single ADDI instruction (if the upper 20 bits are zero)
    /// or a LUI followed by an ADDI instruction.
    ///
    /// # Parameters
    /// - `idx`: The byte offset at which to place the instructions.
    /// - `rd`: The destination register.
    /// - `imm`: The immediate value (128-bit).
    pub fn li(&mut self, idx: usize, rd: XReg, imm: u128) {
        assert!(
            imm <= 0xFFFFFFFF,
            "in this example `li` only supports immediate values less than 0xFFFFFFFF"
        );
        let imm = imm as u32;
        let (simm20, simm12) = (imm >> 12, imm & 0xFFF);
        if simm20 != 0 {
            self.lui(idx, rd, simm20);
            self.addi(idx + 4, rd, rd, simm12);
        } else {
            self.addi(idx, rd, XReg::Zero, simm12);
        }
    }

    /// Assemble a BEQ (branch if equal) instruction and store it at the given memory index.
    ///
    /// # Parameters
    /// - `idx`: The byte offset at which to place the instruction.
    /// - `rs1`: The first source register.
    /// - `rs2`: The second source register.
    /// - `offset`: The branch offset.
    pub fn beq(&mut self, idx: usize, rs1: XReg, rs2: XReg, offset: impl Into<Offset>) {
        let opcode = OPCODE_BRANCH;
        let funct3 = FUNCT3_BRANCH_BEQ;
        // Convert offset into the proper bit segments for the instruction encoding.
        let offset_u32 = u32::from_ne_bytes(i32::to_ne_bytes(offset.into().0));
        let simm12_12 = (offset_u32 & 0b1_0000_0000_0000) >> 12;
        let simm12_11 = (offset_u32 & 0b1000_0000_0000) >> 11;
        let simm12_10_5 = (offset_u32 & 0b111_1110_0000) >> 5;
        let simm12_4_1 = (offset_u32 & 0b1_1110) >> 1;
        let word = simm12_12 << 31
            | simm12_10_5 << 25
            | ((rs2 as u32) << 20)
            | ((rs1 as u32) << 15)
            | (funct3 << 12)
            | simm12_4_1 << 8
            | simm12_11 << 7
            | opcode;
        self.inner[idx / 4] = word;
    }

    /// Assemble a BEQZ (branch if equal to zero) instruction.
    ///
    /// This is a special case of BEQ where the second register is hardwired to zero.
    ///
    /// # Parameters
    /// - `idx`: The byte offset at which to place the instruction.
    /// - `rs`: The register to test for zero.
    /// - `offset`: The branch offset.
    pub fn beqz(&mut self, idx: usize, rs: XReg, offset: impl Into<Offset>) {
        self.beq(idx, rs, Zero, offset);
    }

    /// Assemble an ECALL instruction at the given offset.
    ///
    /// This instruction triggers a supervisor call exception.
    ///
    /// # Parameters
    /// - `offset`: The byte offset at which to place the ecall instruction.
    pub fn ecall(&mut self, idx: usize) {
        let word = 0b000000000000_00000_000_00000_1110011;
        self.inner[idx / 4] = word;
    }

    /// Retrieve an instruction word from instruction memory based on the given pointer.
    ///
    /// Returns `None` if the pointer is not aligned or outside the allocated memory range.
    ///
    /// # Parameters
    /// - `ptr`: The 128-bit address from which to fetch the instruction.
    pub fn get(&mut self, ptr: u128) -> Option<u32> {
        if ptr % 4 != 0 || ptr >= (BASE + 4 * N_INSNS) as u128 {
            return None;
        }
        Some(self.inner[(ptr as usize - BASE) / 4])
    }
}

impl SimpleRv128IHart {
    /// Creates a new RV128I hart emulator with the given instruction memory.
    ///
    /// The hart is initialized with all registers set to zero and the program counter
    /// set to the base address of the instruction memory.
    pub fn new(inst_memory: InstMemory<0x2000_0000, { 0x4000 / 4 }>) -> Self {
        Self {
            xregs: [0; 32],
            pc: 0x2000_0000,
            inst_memory,
        }
    }

    /// Execute one instruction step.
    ///
    /// Fetches, decodes, and executes the instruction at the current program counter (PC),
    /// updating the PC accordingly.
    ///
    /// # Returns
    /// - `Ok(())` if executed normally.
    /// - `Err(Exception::SupervisorEcall)` if an ecall instruction was encountered.
    /// - `Err(e)` for other exceptions.
    pub fn stepi(&mut self) -> Result<(), Exception> {
        let raw_insn = self
            .inst_memory
            .get(self.pc)
            .ok_or(Exception::InstructionAccessFault)?;

        println!("Insn at 0x{:x}: 0x{:x}", self.pc, raw_insn);

        // Attempt to decode the raw instruction into one of the supported instruction variants.
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
            Instruction::Beq(rs1, rs2, offset) => {
                if self.xregs[rs1 as usize] == self.xregs[rs2 as usize] {
                    self.pc = self.pc.wrapping_add_signed(offset.0 as i128);
                } else {
                    self.pc = self.pc.wrapping_add(4);
                }
            }
            Instruction::Ecall => return Err(Exception::SupervisorEcall),
        }

        Ok(())
    }
}

/* -- RISC-V ISA enumerations and structures -- */

/// RISC-V exceptions that may occur during emulation.
#[derive(Debug)]
pub enum Exception {
    /// The instruction is illegal or not supported.
    IllegalInstruction,
    /// The instruction memory access failed (e.g., due to an out-of-bound address).
    InstructionAccessFault,
    /// An ecall was executed in supervisor mode.
    SupervisorEcall,
}

/// Enum representing the supported instructions in our simplified RV128I emulator.
#[derive(Debug)]
pub enum Instruction {
    /// ADDI instruction: rd = rs + immediate.
    Addi(XReg, XReg, Simm12),
    /// LUI instruction: rd = immediate << 12.
    Lui(XReg, Simm20),
    /// BEQ instruction: if (rs1 == rs2) branch to PC + offset.
    Beq(XReg, XReg, Offset),
    /// ECALL instruction to trigger a supervisor call.
    Ecall,
}

impl TryFrom<u32> for Instruction {
    type Error = ();

    /// Attempts to decode a 32-bit word into a supported Instruction.
    ///
    /// Returns an error if the instruction encoding does not match any known pattern.
    fn try_from(value: u32) -> Result<Self, Self::Error> {
        let opcode = value & 0x7F;
        let rd = ((value >> 7) & 0x1F).try_into().unwrap();
        let rs1 = ((value >> 15) & 0x1F).try_into().unwrap();
        let rs2 = ((value >> 20) & 0x1F).try_into().unwrap();
        let funct3 = (value >> 12) & 0b111;
        let simm12 = (value >> 20).into();
        let simm20 = (value >> 12).into();
        // Decode the branch offset from its scattered bit fields.
        let offset = {
            let offset12 = value >> 31;
            let offset10_5 = (value >> 25) & 0x3F;
            let offset4_1 = (value >> 8) & 0xF;
            let offset11 = (value >> 7) & 0x1;
            let value = (offset4_1 << 1) | (offset10_5 << 5) | (offset11 << 11) | (offset12 << 12);
            value.into()
        };
        if opcode == OPCODE_OP_IMM && funct3 == FUNCT3_OP_ADD_SUB {
            Ok(Self::Addi(rd, rs1, simm12))
        } else if opcode == OPCODE_LUI {
            Ok(Self::Lui(rd, simm20))
        } else if opcode == OPCODE_BRANCH && funct3 == FUNCT3_BRANCH_BEQ {
            Ok(Self::Beq(rs1, rs2, offset))
        } else if value == 0b000000000000_00000_000_00000_1110011 {
            Ok(Self::Ecall)
        } else {
            Err(())
        }
    }
}

/// Enumeration of RISC-V registers.
///
/// Each variant corresponds to a register name and its associated register number.
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

    /// Convert a u32 into an XReg.
    /// Returns an error if the value does not correspond to a valid register number.
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

/// A 12-bit signed immediate value used in instructions such as ADDI.
#[derive(Clone, Copy, Debug)]
pub struct Simm12(u16);

impl From<u32> for Simm12 {
    fn from(value: u32) -> Self {
        Self((value & 0x0FFF) as u16)
    }
}

/// A 20-bit immediate value used in instructions such as LUI.
#[derive(Clone, Copy, Debug)]
pub struct Simm20(u32);

impl From<u32> for Simm20 {
    fn from(value: u32) -> Self {
        Self(value & 0xFFFFF)
    }
}

/// A branch offset used in branch instructions.
#[derive(Clone, Copy, Debug)]
pub struct Offset(i32);

impl From<i32> for Offset {
    fn from(value: i32) -> Self {
        Self(value & 0x1FFE)
    }
}

impl From<u32> for Offset {
    fn from(mut value: u32) -> Self {
        value = value & 0x1FFE;
        if value & 0x1000 != 0 {
            value |= 0xFFFFE000;
        }
        let ans = i32::from_ne_bytes(u32::to_ne_bytes(value));
        Self(ans)
    }
}
