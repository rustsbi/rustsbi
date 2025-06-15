# Architecture identifier.
arch = "loongarch64"
# Platform identifier.
platform = "loongarch64-qemu-virt"

#
# Platform configs
#
[plat]
# Platform family.
family = "loongarch64-qemu-virt"

# Base address of the whole physical memory.
phys-memory-base = 0x8000_0000        # uint
# Size of the whole physical memory. (128M)
phys-memory-size = 0x800_0000         # uint
# Base physical address of the kernel image.
kernel-base-paddr = 0x8000_0000       # uint

# Base virtual address of the kernel image.
kernel-base-vaddr = "0xffff_0000_8000_0000"     # uint
# Linear mapping offset, for quick conversions between physical and virtual
# addresses.
phys-virt-offset = "0xffff_0000_0000_0000"      # uint
# Offset of bus address and phys address. some boards, the bus address is
# different from the physical address.
phys-bus-offset = 0                             # uint
# Kernel address space base.
kernel-aspace-base = "0xffff_0000_0000_0000"    # uint
# Kernel address space size.
kernel-aspace-size = "0x0000_ffff_ffff_f000"    # uint

#
# Device specifications
#
[devices]
# MMIO regions with format (`base_paddr`, `size`).
mmio-regions = [
    [0x100E_0000, 0x0000_1000],         # GED
    [0x1FE0_0000, 0x0000_1000],         # UART
    [0x2000_0000, 0x1000_0000],         # PCI
    [0x4000_0000, 0x0002_0000],         # PCI RANGES
]           # [(uint, uint)]
# VirtIO MMIO regions with format (`base_paddr`, `size`).
virtio-mmio-regions = []    # [(uint, uint)]
# Base physical address of the PCIe ECAM space.
pci-ecam-base = 0x2000_0000             # uint
# End PCI bus number.
pci-bus-end = 0x7f                      # uint
# PCI device memory ranges.
pci-ranges = [
    [0, 0],
    [0x4000_0000, 0x0002_0000]
]                                       # [(uint, uint)]
# poweroff {
#     value = <0x00000034>;
#     offset = <0x00000000>;
#     compatible = "syscon-poweroff";
# };
# ged@100e001c {
#     reg-io-width = <0x00000001>;
#     reg-shift = <0x00000000>;
#     reg = <0x00000000 0x100e001c 0x00000000 0x00000003>;
#     compatible = "syscon";
# };
ged-paddr  = 0x100E001C                 # uint
# serial@1fe001e0 {
#     interrupt-parent = <0x00008003>;
#     interrupts = <0x00000002 0x00000004>;
#     clock-frequency = <0x05f5e100>;
#     reg = <0x00000000 0x1fe001e0 0x00000000 0x00000100>;
#     compatible = "ns16550a";
# };
uart-paddr = 0x1FE001E0                 # uint

# Timer interrupt frequency in Hz.
timer-frequency = 100_000_000           # uint
