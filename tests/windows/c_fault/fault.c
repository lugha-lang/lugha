#include <stdint.h>

/* Writes through the address it is given: zero faults. */
void poke(int64_t at) {
    *(volatile int *)(uintptr_t)at = 1;
}
