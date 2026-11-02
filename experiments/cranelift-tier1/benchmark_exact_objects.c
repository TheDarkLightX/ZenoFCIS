#define _POSIX_C_SOURCE 200809L
#include <errno.h>
#include <inttypes.h>
#include <limits.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

/* Linked exclusively from the hash-checked, original Cranelift ELF objects.
 * Calls are intentionally out of line. Timings include call/ret/loop overhead;
 * they are not isolated instruction latency or peak processor throughput. */
extern int64_t zeno_tier1_baseline(int64_t, int64_t);
extern int64_t zeno_tier1_candidate(int64_t, int64_t);
typedef int64_t (*satadd_t)(int64_t, int64_t);

static uint64_t now_ns(void) {
    struct timespec ts;
    if (clock_gettime(CLOCK_MONOTONIC_RAW, &ts) != 0) {
        perror("clock_gettime"); exit(2);
    }
    return (uint64_t)ts.tv_sec * UINT64_C(1000000000) + (uint64_t)ts.tv_nsec;
}

static int64_t oracle(int64_t x, int64_t y) {
    __int128 v = (__int128)x + (__int128)y;
    if (v > INT64_MAX) return INT64_MAX;
    if (v < INT64_MIN) return INT64_MIN;
    return (int64_t)v;
}

static uint64_t rand64(uint64_t *state) {
    uint64_t x = *state;
    x ^= x >> 12; x ^= x << 25; x ^= x >> 27;
    *state = x;
    return x * UINT64_C(2685821657736338717);
}

static void verify(void) {
    uint64_t state = UINT64_C(0x1234abcdef987654);
    for (size_t i = 0; i < 32000; ++i) {
        int64_t a = (int64_t)rand64(&state);
        int64_t b = (int64_t)rand64(&state);
        if (i % 11 == 0) { a = INT64_MAX - (int64_t)(i % 5); b = 7; }
        if (i % 13 == 0) { a = INT64_MIN + (int64_t)(i % 5); b = -7; }
        int64_t expect = oracle(a,b);
        if (zeno_tier1_baseline(a,b) != expect || zeno_tier1_candidate(a,b) != expect) {
            fprintf(stderr, "semantic preflight failure at %zu\n", i);
            exit(3);
        }
    }
}

/* Dependency chain: each output feeds the next input. */
__attribute__((noinline))
static uint64_t chain(satadd_t f, uint64_t iterations, int overflow) {
    int64_t value = overflow ? INT64_MAX - 1 : INT64_C(1234567890);
    int64_t operand = overflow ? 7 : 17;
    for (uint64_t i = 0; i < iterations; ++i)
        value = f(value, operand);
    return (uint64_t)value;
}

/* 8 independent dependency chains, testing out-of-order overlapping calls.
 * This is an effective call-level throughput probe, not a pure uop benchmark. */
__attribute__((noinline))
static uint64_t parallel8(satadd_t f, uint64_t iterations) {
    int64_t a0 = INT64_C(123456), a1 = INT64_C(-124578);
    int64_t a2 = INT64_MAX - 9, a3 = INT64_MIN + 9;
    int64_t a4 = INT64_C(33333), a5 = INT64_C(-22222);
    int64_t a6 = INT64_C(456789), a7 = INT64_C(-456789);
    for (uint64_t i = 0; i < iterations; ++i) {
        a0 = f(a0, 19); a1 = f(a1, -31);
        a2 = f(a2, 17); a3 = f(a3, -17);
        a4 = f(a4, 23); a5 = f(a5, -29);
        a6 = f(a6, 37); a7 = f(a7, -41);
    }
    return ((uint64_t)a0 ^ (uint64_t)a1 ^ (uint64_t)a2 ^ (uint64_t)a3 ^
            (uint64_t)a4 ^ (uint64_t)a5 ^ (uint64_t)a6 ^ (uint64_t)a7);
}

static uint64_t workload(satadd_t f, const char *mode, uint64_t calls) {
    if (!strcmp(mode, "chain_nonoverflow")) return chain(f, calls, 0);
    if (!strcmp(mode, "chain_saturating")) return chain(f, calls, 1);
    if (!strcmp(mode, "parallel8")) return parallel8(f, calls / 8);
    fprintf(stderr, "unknown workload: %s\n", mode); exit(2);
}

static uint64_t parse_uint(const char *arg, uint64_t lo, uint64_t hi) {
    errno = 0;
    char *end = NULL;
    unsigned long long v = strtoull(arg, &end, 10);
    if (errno || !end || *end || v < lo || v > hi) {
        fprintf(stderr, "invalid iteration or round limit: %s\n", arg); exit(2);
    }
    return (uint64_t)v;
}

int main(int argc, char **argv) {
    if (argc != 3) { fprintf(stderr, "usage: %s CALLS_PER_SAMPLE ROUNDS\n", argv[0]); return 2; }
    uint64_t calls = parse_uint(argv[1], 8000, 200000000);
    unsigned rounds = (unsigned)parse_uint(argv[2], 4, 100);
    if (calls % 8 != 0) { fprintf(stderr, "calls must be divisible by 8\n"); return 2; }
    verify();
    const char *modes[] = { "chain_nonoverflow", "chain_saturating", "parallel8" };
    satadd_t funcs[] = {zeno_tier1_baseline, zeno_tier1_candidate};
    /* Initial warming also verifies that every benchmark workload produces
     * exactly the same results under the two independently checked objects. */
    for (unsigned m = 0; m < 3; ++m) {
        uint64_t x = workload(funcs[0], modes[m], calls / 8);
        uint64_t y = workload(funcs[1], modes[m], calls / 8);
        if (x != y) { fprintf(stderr,"workload output mismatch for %s\n", modes[m]); return 3; }
    }
    puts("schema,round,mode,variant,calls,ns,checksum");
    for (unsigned round = 0; round < rounds; ++round) {
        for (unsigned m = 0; m < 3; ++m) {
            /* ABBA order, alternating round-by-round and mode-by-mode. */
            for (unsigned pass = 0; pass < 2; ++pass) {
                unsigned which = (round + m + pass) % 2;
                uint64_t start = now_ns();
                uint64_t checksum = workload(funcs[which], modes[m], calls);
                uint64_t elapsed = now_ns() - start;
                printf("tier1-bench-v1,%u,%s,%s,%" PRIu64 ",%" PRIu64 ",%" PRIu64 "\n",
                    round, modes[m], which ? "candidate" : "baseline", calls, elapsed, checksum);
            }
        }
    }
    return 0;
}
