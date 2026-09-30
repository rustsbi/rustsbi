#include <config.h>

VM_IMAGE(linux_image, XSTR(BAO_LINUX_BIN))

struct config config = {
    CONFIG_HEADER

    .vmlist_size = 1,
    .vmlist = (struct vm_config[]) {
        {
            .image = {
                .base_addr = 0x90200000,
                .load_addr = VM_IMAGE_OFFSET(linux_image),
                .size = VM_IMAGE_SIZE(linux_image)
            },
            .entry = 0x90200000,
            .platform = {
                .cpu_num = 4,
                .region_num = 1,
                .regions = (struct vm_mem_region[]) {
                    {
                        .base = 0x90000000,
                        .size = 0x40000000,
                        .place_phys = true,
                        .phys = 0x90000000
                    }
                },
                .dev_num = 1,
                .devs = (struct vm_dev_region[]) {
                    {
                        .pa = 0x10001000,
                        .va = 0x10001000,
                        .size = 0x8000,
                        .interrupt_num = 8,
                        .interrupts = (irqid_t[]) {1, 2, 3, 4, 5, 6, 7, 8}
                    }
                },
                .arch = {
                    .irqc.plic.base = 0xc000000,
                }
            },
        },
    }
};
