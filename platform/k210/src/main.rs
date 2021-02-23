#![no_std]
#![no_main]
#![feature(alloc_error_handler)]
#![feature(global_asm)]
#![feature(llvm_asm)]

#[cfg(not(test))]
use core::alloc::Layout;
#[cfg(not(test))]
use core::panic::PanicInfo;
use k210_hal::{clock::Clocks, fpioa, pac, prelude::*};
use linked_list_allocator::LockedHeap;
use rustsbi::{enter_privileged, print, println};
use riscv::register::{
    mcause::{self, Exception, Interrupt, Trap},
    medeleg, mepc, mhartid, mideleg, mie, mip, misa::{self, MXL},
    mstatus::{self, MPP},
    mtval,
    mtvec::{self, TrapMode},
    satp,
};

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

static mut DEVINTRENTRY: usize = 0;

#[cfg(not(test))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("[rustsbi] {}", info);
    loop {}
}

#[cfg(not(test))]
#[alloc_error_handler]
fn oom(layout: Layout) -> ! {
    println!("[rustsbi] out of memory for layout {:?}", layout);
    loop {}
}

fn mp_hook() -> bool {
    use riscv::asm::wfi;
    use k210_hal::clint::msip;

    let hartid = mhartid::read();
    if hartid == 0 {
        true
    } else {
        unsafe {
            // Clear IPI
            msip::clear_ipi(hartid);
            // Start listening for software interrupts
            mie::set_msoft();

            loop {
                wfi();
                if mip::read().msoft() {
                    break;
                }
            }

            // Stop listening for software interrupts
            mie::clear_msoft();
            // Clear IPI
            msip::clear_ipi(hartid);
        }
        false
    }
}

#[export_name = "_start"]
#[link_section = ".text.entry"] // this is stable
fn main() -> ! {
    unsafe {
        llvm_asm!(
            "
        csrr    a2, mhartid
        lui     t0, %hi(_max_hart_id)
        add     t0, t0, %lo(_max_hart_id)
        bgtu    a2, t0, _start_abort
        la      sp, _stack_start
        lui     t0, %hi(_hart_stack_size)
        add     t0, t0, %lo(_hart_stack_size)
    .ifdef __riscv_mul
        mul     t0, a2, t0
    .else
        beqz    a2, 2f  // Jump if single-hart
        mv      t1, a2
        mv      t2, t0
    1:
        add     t0, t0, t2
        addi    t1, t1, -1
        bnez    t1, 1b
    2:
    .endif
        sub     sp, sp, t0
        csrw    mscratch, zero
        j _start_success
        
    _start_abort:
        wfi
        j _start_abort
    _start_success:
        
    "
        )
    };
    if mp_hook() {
        extern "C" {
            static mut _ebss: u32;
            static mut _sbss: u32;
            static mut _edata: u32;
            static mut _sdata: u32;
            static _sidata: u32;
        }
        unsafe {
            r0::zero_bss(&mut _sbss, &mut _ebss);
            r0::init_data(&mut _sdata, &mut _edata, &_sidata);
        } 
    }

    extern "C" {
        fn _start_trap();
    }
    unsafe {
        mtvec::write(_start_trap as usize, TrapMode::Direct);
    }
    if mhartid::read() == 0 {
        extern "C" {
            fn _sheap();
            fn _heap_size();
        }
        let sheap = &mut _sheap as *mut _ as usize;
        let heap_size = &_heap_size as *const _ as usize;
        unsafe {
            ALLOCATOR.lock().init(sheap, heap_size);
        }

        let p = pac::Peripherals::take().unwrap();

        let mut sysctl = p.SYSCTL.constrain();
        let fpioa = p.FPIOA.split(&mut sysctl.apb0);
        let clocks = Clocks::new();
        let _uarths_tx = fpioa.io5.into_function(fpioa::UARTHS_TX);
        let _uarths_rx = fpioa.io4.into_function(fpioa::UARTHS_RX);
        // Configure UART
        let serial = p.UARTHS.configure(115_200.bps(), &clocks);
        let (tx, rx) = serial.split();
        use rustsbi::legacy_stdio::init_legacy_stdio_embedded_hal_fuse;
        init_legacy_stdio_embedded_hal_fuse(tx, rx);

        struct Ipi;
        impl rustsbi::Ipi for Ipi {
            fn max_hart_id(&self) -> usize {
                1
            }
            fn send_ipi_many(&mut self, hart_mask: rustsbi::HartMask) -> rustsbi::SbiRet {
                use k210_hal::clint::msip;
                for i in 0..=1 {
                    if hart_mask.has_bit(i) {
                        msip::set_ipi(i);
                        msip::clear_ipi(i);
                    }
                }
                rustsbi::SbiRet::ok(0)
            }
        }
        use rustsbi::init_ipi;
        init_ipi(Ipi);
        struct Timer;
        impl rustsbi::Timer for Timer {
            fn set_timer(&mut self, stime_value: u64) {
                // This function must clear the pending timer interrupt bit as well.
                use k210_hal::clint::mtimecmp;
                mtimecmp::write(mhartid::read(), stime_value);
                unsafe { mip::clear_mtimer() };
            }
        }
        use rustsbi::init_timer;
        init_timer(Timer);

        struct Reset;
        impl rustsbi::Reset for Reset {
            fn system_reset(&self, reset_type: usize, reset_reason: usize) -> rustsbi::SbiRet {
                println!("[rustsbi] reset triggered! todo: shutdown all harts on k210; program halt. Type: {}, reason: {}", reset_type, reset_reason);
                loop {}
            }
        }
        use rustsbi::init_reset;
        init_reset(Reset);

        use k210_hal::plic::Priority;
        use k210_hal::pac::Interrupt;
        use k210_hal::gpiohs::Edge;
        unsafe { 
            pac::PLIC::set_threshold(mhartid::read(), Priority::P0);
        }
        let gpiohs = p.GPIOHS.split();
        fpioa.io16.into_function(fpioa::GPIOHS0);
        let mut boot = gpiohs.gpiohs0.into_pull_up_input();
        boot.trigger_on_edge(Edge::RISING | Edge::FALLING);
        unsafe {
            pac::PLIC::set_priority(Interrupt::GPIOHS0, Priority::P1);
            pac::PLIC::unmask(mhartid::read(), Interrupt::GPIOHS0);
        }
        boot.clear_interrupt_pending_bits();
    }

    unsafe {
        //mideleg::set_sext();
        mideleg::set_stimer();
        mideleg::set_ssoft();
        medeleg::set_instruction_misaligned();
        medeleg::set_breakpoint();
        medeleg::set_user_env_call();
        /* MMU Exception Delegation
        /* Page Faults are *Reserved* in 1.9.1 version */
        - medeleg::set_instruction_page_fault();
        - medeleg::set_load_page_fault();
        - medeleg::set_store_page_fault();
        /* Actually, in 1.9.1 they are merged into more general exceptions */
        + medeleg::set_instruction_fault();
        + medeleg::set_load_fault();
        + medeleg::set_store_fault(); */
        medeleg::set_instruction_fault();
        medeleg::set_load_fault();
        medeleg::set_store_fault();
        // 默认不打开mie::set_mext
        // 不打开mie::set_mtimer
        mie::set_msoft();
    }

    if mhartid::read() == 0 {
        println!("[rustsbi] RustSBI version {}", rustsbi::VERSION);
        println!("{}", rustsbi::LOGO);
        println!("[rustsbi] Platform: K210 (Version {})", env!("CARGO_PKG_VERSION"));
        let isa = misa::read();
        if let Some(isa) = isa {
            let mxl_str = match isa.mxl() {
                MXL::XLEN32 => "RV32",
                MXL::XLEN64 => "RV64",
                MXL::XLEN128 => "RV128",
            };
            print!("[rustsbi] misa: {}", mxl_str);
            for ext in 'A'..='Z' {
                if isa.has_extension(ext) {
                    print!("{}", ext);
                }
            }
            println!("");
        }
        println!("[rustsbi] mideleg: {:#x}", mideleg::read().bits());
        println!("[rustsbi] medeleg: {:#x}", medeleg::read().bits());
        println!("[rustsbi] Kernel entry: 0x80020000");
    }
    extern "C" {
        fn _s_mode_start();
    }
    unsafe {
        mepc::write(_s_mode_start as usize);
        mstatus::set_mpp(MPP::Supervisor);
        enter_privileged(mhartid::read(), 0x2333333366666666);
    }
}

global_asm!(
    "
    .section .text
    .globl _s_mode_start
_s_mode_start:
1:  auipc ra, %pcrel_hi(1f)
    ld ra, %pcrel_lo(1b)(ra)
    jr ra
.align  3
1:  .dword 0x80020000
"
);

// todo: configurable target address

global_asm!(
    "
    .equ REGBYTES, 8
    .macro STORE reg, offset
        sd  \\reg, \\offset*REGBYTES(sp)
    .endm
    .macro LOAD reg, offset
        ld  \\reg, \\offset*REGBYTES(sp)
    .endm
    .section .text
    .global _start_trap
    .p2align 2
_start_trap:
    csrrw   sp, mscratch, sp
    bnez    sp, 1f
    /* from M level, load sp */
    csrrw   sp, mscratch, zero
1:
    addi    sp, sp, -16 * REGBYTES
    STORE   ra, 0
    STORE   t0, 1
    STORE   t1, 2
    STORE   t2, 3
    STORE   t3, 4
    STORE   t4, 5
    STORE   t5, 6
    STORE   t6, 7
    STORE   a0, 8
    STORE   a1, 9
    STORE   a2, 10
    STORE   a3, 11
    STORE   a4, 12
    STORE   a5, 13
    STORE   a6, 14
    STORE   a7, 15
    mv      a0, sp
    call    _start_trap_rust
    LOAD    ra, 0
    LOAD    t0, 1
    LOAD    t1, 2
    LOAD    t2, 3
    LOAD    t3, 4
    LOAD    t4, 5
    LOAD    t5, 6
    LOAD    t6, 7
    LOAD    a0, 8
    LOAD    a1, 9
    LOAD    a2, 10
    LOAD    a3, 11
    LOAD    a4, 12
    LOAD    a5, 13
    LOAD    a6, 14
    LOAD    a7, 15
    addi    sp, sp, 16 * REGBYTES
    csrrw   sp, mscratch, sp
    mret
"
);

#[allow(unused)]
struct TrapFrame {
    ra: usize,
    t0: usize,
    t1: usize,
    t2: usize,
    t3: usize,
    t4: usize,
    t5: usize,
    t6: usize,
    a0: usize,
    a1: usize,
    a2: usize,
    a3: usize,
    a4: usize,
    a5: usize,
    a6: usize,
    a7: usize,
}

#[export_name = "_start_trap_rust"]
extern "C" fn start_trap_rust(trap_frame: &mut TrapFrame) {
    let cause = mcause::read().cause();
    match cause {
        Trap::Exception(Exception::SupervisorEnvCall) => {
            if trap_frame.a7 == 0x0A000004 && trap_frame.a6 == 0x210 {
                // We use implementation specific sbi_rustsbi_k210_sext function (extension 
                // id: 0x0A000004, function id: 0x210) to register S-level interrupt handler
                // for K210 chip only. This chip uses 1.9.1 version of privileged spec,
                // which did not declare any S-level external interrupts. 
                unsafe { DEVINTRENTRY = trap_frame.a0; }
                // enable mext
                unsafe { mie::set_mext(); }
                // return values
                trap_frame.a0 = 0; // SbiRet::error = SBI_SUCCESS
                trap_frame.a1 = 0; // SbiRet::value = 0
            } else {
                // Due to legacy 1.9.1 version of privileged spec, if we are in S-level
                // timer handler (delegated from M mode), and we call SBI's `set_timer`,
                // a M-level external interrupt may be triggered. This may try to obtain
                // data structures locked previously by S-level interrupt handler, which
                // results in a deadlock. 
                // Ref: https://github.com/luojia65/rustsbi/pull/5 
                if trap_frame.a7 == 0x0 {
                    unsafe {
                        let mtip = mip::read().mtimer();
                        if mtip {
                            if DEVINTRENTRY != 0 {
                                mie::set_mext();
                            }
                        }
                    }
                }
                // Actual ecall handler which is common for all RustSBI platforms
                let params = [trap_frame.a0, trap_frame.a1, trap_frame.a2, trap_frame.a3, trap_frame.a4];
                let ans = rustsbi::ecall(trap_frame.a7, trap_frame.a6, params);
                trap_frame.a0 = ans.error;
                trap_frame.a1 = ans.value;
            }
            mepc::write(mepc::read().wrapping_add(4));
        }
        Trap::Interrupt(Interrupt::MachineSoft) => {
            // Forward to S-level software interrupt
            unsafe {
                mip::set_ssoft(); // set S-soft interrupt flag
                mie::clear_msoft(); // mask M-soft interrupt
            }
        }
        Trap::Interrupt(Interrupt::MachineTimer) => {
            // Forward to S-level timer interrupt
            unsafe {
                mip::set_stimer(); // set S-timer interrupt flag
                mie::clear_mext(); // Ref: Pull request #5
                mie::clear_mtimer(); // mask M-timer interrupt
            }
        }
        Trap::Interrupt(Interrupt::MachineExternal) => {
            /* legacy software delegation
            // to make UARTHS interrupt soft delegation work; ref: pull request #1
            // PLIC target0(Always Hart0-M-Interrupt) acquire
            let irq_id = unsafe { (0x0c20_0004 as *const u32).read_volatile() };
            // read from UARTHS RXFIFO
            let ch: u8 = unsafe { (0x3800_0004 as *const u32).read_volatile() & 0xFF } as u8;
            // black magic @_@, soft delegation won't success without it!
            print!("{}", 0 as char);
            // PLIC complete
            unsafe { (0x0c20_0004 as *mut u32).write_volatile(irq_id); }
            // communicate with delegated interrupt with stval CSR 
            unsafe { llvm_asm!("csrw stval, $0" :: "r"(ch as usize) :: "volatile"); }
            // soft delegate to S Mode soft interrupt
            unsafe { mip::set_ssoft(); }
             */
            unsafe {
                let mut mstatus: usize;
                llvm_asm!("csrr $0, mstatus" : "=r"(mstatus) ::: "volatile");
                // set mstatus.mprv
                mstatus |= 1 << 17;
                // it may trap from U/S Mode
                // save mpp and set mstatus.mpp to S Mode
                let mpp = (mstatus >> 11) & 3;
                mstatus = mstatus & !(3 << 11);
                mstatus |= 1 << 11;
                // drop mstatus.mprv protection
                llvm_asm!("csrw mstatus, $0" :: "r"(mstatus) :: "volatile");
                fn devintr() {
                    unsafe {
                        // call devintr defined in application
                        // we have to ask compiler save ra explicitly
                        llvm_asm!("jalr 0($0)" :: "r"(DEVINTRENTRY) : "ra" : "volatile");
                    }
                }
                // compiler helps us save/restore caller-saved registers
                devintr();
                // restore mstatus
                mstatus = mstatus &!(3 << 11);
                mstatus |= mpp << 11;
                mstatus -= 1 << 17;
                llvm_asm!("csrw mstatus, $0" :: "r"(mstatus) :: "volatile");
            }
        }
        Trap::Exception(Exception::IllegalInstruction) => {
            let vaddr = mepc::read();
            let ins = unsafe { get_vaddr_u32(vaddr) };
            if ins & 0xFFFFF07F == 0xC0102073 { // rdtime instruction
                // rdtime is actually a csrrw instruction
                let rd = ((ins >> 7) & 0b1_1111) as u8;
                let mtime = k210_hal::clint::mtime::read();
                let time_usize = mtime as usize;
                set_rd(trap_frame, rd, time_usize);
                mepc::write(mepc::read().wrapping_add(4)); // skip current instruction 
            } else if ins & 0xFE007FFF == 0x12000073 { // sfence.vma instruction
                // There is no `sfence.vma` in 1.9.1 privileged spec; however there is a `sfence.vm`.
                // For backward compability, here we emulate the first instruction using the second one.
                // sfence.vma: | 31..25 funct7=SFENCE.VMA(0001001) | 24..20 rs2/asid | 19..15 rs1/vaddr | 
                //               14..12 funct3=PRIV(000) | 11..7 rd, =0 | 6..0 opcode=SYSTEM(1110011) |
                // sfence.vm(1.9):  | 31..=20 SFENCE.VM(000100000100) | 19..15 rs1/vaddr |
                //               14..12 funct3=PRIV(000) | 11..7 rd, =0 | 6..0 opcode=SYSTEM(1110011) |
                // discard rs2 // let _rs2_asid = ((ins >> 20) & 0b1_1111) as u8;
                // let rs1_vaddr = ((ins >> 15) & 0b1_1111) as u8;
                // read paging mode from satp (sptbr)
                let satp_bits = satp::read().bits();
                // bit 63..20 is not readable and writeable on K210, so we cannot
                // decide paging type from the 'satp' register.
                // that also means that the asid function is not usable on this chip.
                // we have to fix it to be Sv39.
                let ppn = satp_bits & 0xFFF_FFFF_FFFF; // 43..0 PPN WARL
                // write to sptbr
                let sptbr_bits = ppn & 0x3F_FFFF_FFFF;
                unsafe { llvm_asm!("csrw 0x180, $0"::"r"(sptbr_bits)) }; // write to sptbr
                // enable paging (in v1.9.1, mstatus: | 28..24 VM[4:0] WARL | ... )
                let mut mstatus_bits: usize; 
                unsafe { llvm_asm!("csrr $0, mstatus":"=r"(mstatus_bits)) };
                mstatus_bits &= !0x1F00_0000;
                mstatus_bits |= 9 << 24; 
                unsafe { llvm_asm!("csrw mstatus, $0"::"r"(mstatus_bits)) };
                // emulate with sfence.vm (declared in privileged spec v1.9)
                unsafe { llvm_asm!(".word 0x10400073") }; // sfence.vm x0
                // ::"r"(rs1_vaddr)
                mepc::write(mepc::read().wrapping_add(4)); // skip current instruction
            } else {
                panic!("invalid instruction! mepc: {:016x?}, instruction: {:08x?}", mepc::read(), ins);
            }
        }
        cause => panic!(
            "unhandled trap! mcause: {:?}, mepc: {:016x?}, mtval: {:016x?}",
            cause,
            mepc::read(),
            mtval::read(),
        ),
    }
}

#[inline]
unsafe fn get_vaddr_u32(vaddr: usize) -> u32 {
    // todo: comment
    get_vaddr_u16(vaddr) as u32 | 
    ((get_vaddr_u16(vaddr.wrapping_add(2)) as u32) << 16)
}

#[inline]
unsafe fn get_vaddr_u16(vaddr: usize) -> u16 {
    let mut ans: u16;
    llvm_asm!("
        li      t0, (1 << 17)
        csrrs   t0, mstatus, t0
        lhu     $0, 0($1)
        csrw    mstatus, t0
    "
        :"=r"(ans) 
        :"r"(vaddr)
        :"t0", "t1");
    ans
}

#[inline]
fn set_rd(trap_frame: &mut TrapFrame, rd: u8, value: usize) {
    match rd {
        10 => trap_frame.a0 = value,
        11 => trap_frame.a1 = value,
        12 => trap_frame.a2 = value,
        13 => trap_frame.a3 = value,
        14 => trap_frame.a4 = value,
        15 => trap_frame.a5 = value,
        16 => trap_frame.a6 = value,
        17 => trap_frame.a7 = value,
        5  => trap_frame.t0 = value,
        6  => trap_frame.t1 = value,
        7  => trap_frame.t2 = value,
        28 => trap_frame.t3 = value,
        29 => trap_frame.t4 = value,
        30 => trap_frame.t5 = value,
        31 => trap_frame.t6 = value,
        _ => panic!("invalid target `rd`"),
    }
}

