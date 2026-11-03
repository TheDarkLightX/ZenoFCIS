/* Native AArch64 finite differential oracle for the two exact Cranelift ELF objects.
 * Pure signed-64 saturating add. The reference computes in signed 128 bits,
 * without signed-64 overflow, using bit-preserving input/output conversions.
 * This is finite execution testing, not exhaustive 2^128-pair verification.
 */
#include <inttypes.h>
#include <limits.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

extern uint64_t tier1_original(uint64_t, uint64_t);
extern uint64_t tier1_patched(uint64_t, uint64_t);

static const uint64_t edges[] = {
    UINT64_C(0), UINT64_C(1), UINT64_C(2), UINT64_C(3),
    UINT64_C(0x7fffffffffffffff), UINT64_C(0x7ffffffffffffffe),
    UINT64_C(0x7ffffffffffffffd), UINT64_C(0x4000000000000000),
    UINT64_C(0x8000000000000000), UINT64_C(0x8000000000000001),
    UINT64_C(0x8000000000000002), UINT64_C(0xc000000000000000),
    UINT64_C(0xffffffffffffffff), UINT64_C(0xfffffffffffffffe),
    UINT64_C(0xfffffffffffffffd), UINT64_C(0x5555555555555555),
    UINT64_C(0xaaaaaaaaaaaaaaaa), UINT64_C(0x0000000100000000),
    UINT64_C(0xffffffff00000000), UINT64_C(0x00000000ffffffff),
    UINT64_C(0x0000ffff0000ffff), UINT64_C(0xffff0000ffff0000),
    UINT64_C(0x0000000080000000), UINT64_C(0xffffffff7fffffff)
};
#define NEDGES (sizeof(edges) / sizeof(edges[0]))
#define NRANDOM UINT64_C(250000)
#define SEED UINT64_C(0x9e3779b97f4a7c15)

static uint64_t prng = SEED;
static uint64_t next_bits(void) {
    prng ^= prng << 13;
    prng ^= prng >> 7;
    prng ^= prng << 17;
    return prng;
}

static uint64_t reference(uint64_t xa, uint64_t xb) {
    int64_t a, b, result;
    uint64_t bits;
    memcpy(&a, &xa, sizeof a);
    memcpy(&b, &xb, sizeof b);
    __int128 wide = (__int128)a + (__int128)b;
    if (wide > INT64_MAX) {
        result = INT64_MAX;
    } else if (wide < INT64_MIN) {
        result = INT64_MIN;
    } else {
        result = (int64_t)wide;
    }
    memcpy(&bits, &result, sizeof bits);
    return bits;
}

static uint64_t checked = 0;
static int check(uint64_t a, uint64_t b) {
    uint64_t want = reference(a, b);
    uint64_t old = tier1_original(a, b);
    uint64_t candidate = tier1_patched(a, b);
    if (old != want || candidate != want) {
        fprintf(stderr,
                "FAIL at pair=%" PRIu64 " a=%016" PRIx64 " b=%016" PRIx64
                " want=%016" PRIx64 " original=%016" PRIx64 " patched=%016" PRIx64 "\n",
                checked, a, b, want, old, candidate);
        return 1;
    }
    checked++;
    return 0;
}

int main(void) {
#if !defined(__aarch64__)
#error "This harness is only a native AArch64 qualification"
#endif
    for (size_t a = 0; a < NEDGES; ++a) {
        for (size_t b = 0; b < NEDGES; ++b) {
            if (check(edges[a], edges[b])) return 1;
        }
    }
    for (uint64_t i = 0; i < NRANDOM; ++i) {
        if (check(next_bits(), next_bits())) return 1;
    }
    printf("{\"status\":\"passed-finite-native-aarch64-pair-oracle\","
           "\"seed\":\"0x%016" PRIx64 "\",\"edge_inputs\":%zu,"
           "\"edge_pairs\":%zu,\"random_pairs\":%" PRIu64 ",\"checked_pairs\":%" PRIu64 "}\n",
           SEED, NEDGES, NEDGES * NEDGES, NRANDOM, checked);
    return 0;
}
