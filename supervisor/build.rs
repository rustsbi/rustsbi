fn main() {
    println!("cargo:rustc-link-arg=-Ttarget/riscv64imac-unknown-none-elf/release/linker_riscv64-qemu-virt.lds");
}
