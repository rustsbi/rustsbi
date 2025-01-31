mod qemu_system_riscv64;

pub fn platform_init() {
    qemu_system_riscv64::platform_init();
}
