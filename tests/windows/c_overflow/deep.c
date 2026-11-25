#include <stdint.h>

/* Recurses `n` deep, each frame holding a little, so the stack runs out. */
int64_t deep(int64_t n) {
    volatile char pad[256];
    pad[0] = (char)n;
    if (n == 0) {
        return 0;
    }
    return deep(n - 1) + pad[0];
}
