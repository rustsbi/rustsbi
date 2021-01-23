#![no_std]
#![no_main]
#![feature(naked_functions)]
#![feature(alloc_error_handler)]
#![feature(llvm_asm)]
#![feature(asm)]
#![feature(global_asm)]

mod hal;

#[cfg(not(test))]
use core::alloc::Layout;
#[cfg(not(test))]
use core::panic::PanicInfo;
use linked_list_allocator::LockedHeap;

use rustsbi::{print, println};

use riscv::register::{
    mcause::{self, Exception, Interrupt, Trap},
    medeleg, mepc, mhartid, mideleg, mie, mip, misa::{self, MXL},
    mstatus::{self, MPP},
    mtval,
    mtvec::{self, TrapMode},
};

#[global_allocator]
static ALLOCATOR: LockedHeap = LockedHeap::empty();

#[cfg(not(test))]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("[rustsbi-panic] {}", info);
    println!("[rustsbi-panic] system shutdown scheduled due to RustSBI panic");
    use rustsbi::Reset;
    hal::Reset.system_reset(
        rustsbi::reset::RESET_TYPE_SHUTDOWN,
        rustsbi::reset::RESET_REASON_SYSTEM_FAILURE
    );
    loop { }
}

#[cfg(not(test))]
#[alloc_error_handler]
fn oom(_layout: Layout) -> ! {
    loop {}
}

lazy_static::lazy_static! {
    // 最大的硬件线程编号；只在启动时写入，跨核软中断发生时读取
    pub static ref MAX_HART_ID: spin::Mutex<usize> = 
        spin::Mutex::new(compiled_max_hartid());
}

// #[export_name = "_mp_hook"]
pub extern "C" fn mp_hook() -> bool {
    let hartid = mhartid::read();
    if hartid == 0 {
        true
    } else {
        use riscv::asm::wfi;
        use hal::Clint;
        unsafe {
            let mut clint = Clint::new(0x200_0000 as *mut u8);
            // Clear IPI
            clint.clear_soft(hartid);
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
            clint.clear_soft(hartid);
        }
        false
    }
}

#[export_name = "_start"]
#[link_section = ".text.entry"] // this is stable
#[naked]
// extern "C" for Rust ABI is by now unsupported for naked functions
unsafe extern "C" fn start() -> ! {
    asm!(
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
        j       main
        
    _start_abort:
        wfi
        j _start_abort
    ", options(noreturn))
}

#[export_name = "main"]
fn main() -> ! {
    // Ref: https://github.com/qemu/qemu/blob/aeb07b5f6e69ce93afea71027325e3e7a22d2149/hw/riscv/boot.c#L243
    let dtb_pa = unsafe {
        let dtb_pa: usize;
        llvm_asm!("":"={a1}"(dtb_pa));
        dtb_pa
    };

    if mp_hook() {
        // init
    }

    /* setup trap */

    extern "C" {
        fn _start_trap();
    }
    unsafe {
        mtvec::write(_start_trap as usize, TrapMode::Direct);
    }


    /* main function start */

    extern "C" {
        static mut _sheap: u8;
        static _heap_size: u8;
    }
    if mhartid::read() == 0 {
        let sheap = unsafe { &mut _sheap } as *mut _ as usize;
        let heap_size = unsafe { &_heap_size } as *const u8 as usize;
        unsafe {
            ALLOCATOR.lock().init(sheap, heap_size);
        }

        // 其实这些参数不用提供，直接通过pac库生成
        let serial = hal::Ns16550a::new(0x10000000, 0, 11_059_200, 115200);

        // use through macro
        use rustsbi::legacy_stdio::init_legacy_stdio_embedded_hal;
        init_legacy_stdio_embedded_hal(serial);

        let clint = hal::Clint::new(0x2000000 as *mut u8);
        use rustsbi::init_ipi;
        init_ipi(clint);
        // todo: do not create two instances
        let clint = hal::Clint::new(0x2000000 as *mut u8);
        use rustsbi::init_timer;
        init_timer(clint);

        use rustsbi::init_reset;
        init_reset(hal::Reset);
    }

    // 把S的中断全部委托给S层
    unsafe {
        mideleg::set_sext();
        mideleg::set_stimer();
        mideleg::set_ssoft();
        medeleg::set_instruction_misaligned();
        medeleg::set_breakpoint();
        medeleg::set_user_env_call();
        medeleg::set_instruction_page_fault();
        medeleg::set_load_page_fault();
        medeleg::set_store_page_fault();
        medeleg::set_instruction_fault();
        medeleg::set_load_fault();
        medeleg::set_store_fault();
        mie::set_mext();
        // 不打开mie::set_mtimer
        mie::set_msoft();
    }

    if mhartid::read() == 0 {
        println!("[rustsbi] RustSBI version {}", rustsbi::VERSION);
        println!("{}", rustsbi::LOGO);
        println!("[rustsbi] Platform: QEMU (Version {})", env!("CARGO_PKG_VERSION"));
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
        let mut guard = MAX_HART_ID.lock();
        *guard = unsafe { count_harts(dtb_pa) };
        drop(guard);
        println!("[rustsbi] Kernel entry: 0x80200000");
    }

    unsafe {
        mepc::write(s_mode_start as usize);
        mstatus::set_mpp(MPP::Supervisor);
        rustsbi::enter_privileged(mhartid::read(), dtb_pa)
    }
}

#[naked]
#[link_section = ".text"] // must add link section for all naked functions
unsafe extern "C" fn s_mode_start() -> ! {
    asm!("
1:  auipc ra, %pcrel_hi(1f)
    ld ra, %pcrel_lo(1b)(ra)
    jr ra
.align  3
1:  .dword 0x80200000
    ", options(noreturn))
}

unsafe fn count_harts(dtb_pa: usize) -> usize {
    use device_tree::{DeviceTree, Node};
    const DEVICE_TREE_MAGIC: u32 = 0xD00DFEED;
    // 遍历“cpu_map”结构
    // 这个结构的子结构是“处理核簇”（cluster）
    // 每个“处理核簇”的子结构分别表示一个处理器核
    fn enumerate_cpu_map(cpu_map_node: &Node) -> usize {
        let mut tot = 0;
        for cluster_node in cpu_map_node.children.iter() {
            let name = &cluster_node.name;
            let count = cluster_node.children.iter().count();
            // 会输出：Hart count: cluster0 with 2 cores
            // 在justfile的“threads := "2"”处更改
            println!("[rustsbi-dtb] Hart count: {} with {} cores", name, count);
            tot += count;
        }
        tot
    }
    #[repr(C)]
    struct DtbHeader { magic: u32, size: u32 }
    let header = &*(dtb_pa as *const DtbHeader);
    // from_be 是大小端序的转换（from big endian）
    let magic = u32::from_be(header.magic);
    if magic == DEVICE_TREE_MAGIC {
        let size = u32::from_be(header.size);
        // 拷贝数据，加载并遍历
        let data = core::slice::from_raw_parts(dtb_pa as *const u8, size as usize);
        if let Ok(dt) = DeviceTree::load(data) {
            if let Some(cpu_map) = dt.find("/cpus/cpu-map") {
                return enumerate_cpu_map(cpu_map)
            }
        }
    }
    // 如果DTB的结构不对（读不到/cpus/cpu-map），返回默认的8个核
    let ans = compiled_max_hartid();
    println!("[rustsbi-dtb] Could not read '/cpus/cpu-map' from 'dtb_pa' device tree root; assuming {} cores", ans);
    ans
}

#[inline]
fn compiled_max_hartid() -> usize {
    let ans;
    unsafe { asm!("
        lui     {ans}, %hi(_max_hart_id)
        add     {ans}, {ans}, %lo(_max_hart_id)
    ", ans = out(reg) ans) };
    ans
}

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

// #[doc(hidden)]
// #[export_name = "_mp_hook"]
// pub extern "Rust" fn _mp_hook() -> bool {
//     match mhartid::read() {
//         0 => true,
//         _ => loop {
//             unsafe { riscv::asm::wfi() }
//         },
//     }
// }

#[allow(unused)]
#[derive(Debug)]
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
            let params = [trap_frame.a0, trap_frame.a1, trap_frame.a2, trap_frame.a3];
            // Call RustSBI procedure
            let ans = rustsbi::ecall(trap_frame.a7, trap_frame.a6, params);
            // Return the return value to TrapFrame
            trap_frame.a0 = ans.error;
            trap_frame.a1 = ans.value;
            // Skip ecall instruction
            mepc::write(mepc::read().wrapping_add(4));
        }
        Trap::Interrupt(Interrupt::MachineSoft) => {
            // 机器软件中断返回给S层
            unsafe {
                mip::set_ssoft();
                mie::clear_msoft();
            }
        }
        Trap::Interrupt(Interrupt::MachineTimer) => {
            // 机器时间中断返回给S层
            unsafe {
                mip::set_stimer();
                mie::clear_mtimer();
            }
        }
        Trap::Exception(Exception::IllegalInstruction) => {
            #[inline]
            unsafe fn get_vaddr_u32(vaddr: usize) -> u32 {
                let mut ans: u32;
                llvm_asm!("
                    li      t0, (1 << 17)
                    mv      t1, $1
                    csrrs   t0, mstatus, t0
                    lwu     t1, 0(t1)
                    csrw    mstatus, t0
                    mv      $0, t1
                "
                    :"=r"(ans) 
                    :"r"(vaddr)
                    :"t0", "t1");
                ans
            }
            let vaddr = mepc::read();
            let ins = unsafe { get_vaddr_u32(vaddr) };
            if ins & 0xFFFFF07F == 0xC0102073 {
                // rdtime
                let rd = ((ins >> 7) & 0b1_1111) as u8;
                // todo: one instance only
                let clint = hal::Clint::new(0x2000000 as *mut u8);
                let time_usize = clint.get_mtime() as usize;
                match rd {
                    10 => trap_frame.a0 = time_usize,
                    11 => trap_frame.a1 = time_usize,
                    12 => trap_frame.a2 = time_usize,
                    13 => trap_frame.a3 = time_usize,
                    14 => trap_frame.a4 = time_usize,
                    15 => trap_frame.a5 = time_usize,
                    16 => trap_frame.a6 = time_usize,
                    17 => trap_frame.a7 = time_usize,
                    5 => trap_frame.t0 = time_usize,
                    6 => trap_frame.t1 = time_usize,
                    7 => trap_frame.t2 = time_usize,
                    28 => trap_frame.t3 = time_usize,
                    29 => trap_frame.t4 = time_usize,
                    30 => trap_frame.t5 = time_usize,
                    31 => trap_frame.t6 = time_usize,
                    _ => panic!("invalid target"),
                }
                mepc::write(mepc::read().wrapping_add(4)); // 跳过指令
            } else {
                #[cfg(target_pointer_width = "64")]
                panic!("invalid instruction, mepc: {:016x?}, instruction: {:016x?}", mepc::read(), ins);
                #[cfg(target_pointer_width = "32")]
                panic!("invalid instruction, mepc: {:08x?}, instruction: {:08x?}", mepc::read(), ins);
            }
        }
        #[cfg(target_pointer_width = "64")]
        cause => panic!(
            "Unhandled exception! mcause: {:?}, mepc: {:016x?}, mtval: {:016x?}, trap frame: {:p}, {:x?}",
            cause,
            mepc::read(),
            mtval::read(),
            &trap_frame as *const _,
            trap_frame
        ),
        #[cfg(target_pointer_width = "32")]
        cause => panic!(
            "Unhandled exception! mcause: {:?}, mepc: {:08x?}, mtval: {:08x?}, trap frame: {:x?}",
            cause,
            mepc::read(),
            mtval::read(),
            trap_frame
        ),
    }
}
