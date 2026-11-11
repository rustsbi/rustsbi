#include <microkit.h>
#include <stdint.h>

/* qemu virt RISC-V UART0: 0x10000000, 16550 */
#define UART0_BASE 0x10000000UL

static void uart_putc(char c) {
    /* THR (offset 0) */
    *(volatile uint8_t *)(UART0_BASE + 0) = (uint8_t)c;
}

static void uart_puts(const char *s) {
    for (; *s; s++) {
        uart_putc(*s);
        if (*s == '\n') uart_putc('\r');
    }
}

void init(void) {
    uart_puts("\n[hello] Microkit PD started on RISC-V!\n");
}

void notified(microkit_channel ch) {
    (void)ch;
}
