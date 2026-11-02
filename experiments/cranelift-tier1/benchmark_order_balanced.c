#define _POSIX_C_SOURCE 200809L
/*
 * Order-balanced extension of the previously checked call-level workload.
 * Include, rather than copy, the exact semantic oracle and workload functions
 * from the v1 harness. Its main is renamed and never invoked. This file does
 * NOT change either Cranelift-generated object or the v1 evidence.
 */
#define main tier1_legacy_benchmark_main
#include "benchmark_exact_objects.c"
#undef main

struct observation {
    unsigned round, mode, position, candidate;
    uint64_t calls, ns, checksum;
};

int main(int argc, char **argv) {
    if (argc != 3) {
        fprintf(stderr, "usage: %s CALLS_PER_SAMPLE ROUNDS\n", argv[0]);
        return 2;
    }
    uint64_t calls = parse_uint(argv[1], 8000, 200000000);
    unsigned rounds = (unsigned)parse_uint(argv[2], 4, 100);
    if (calls % 8) { fprintf(stderr, "calls must be divisible by eight\n"); return 2; }
    verify();
    const char *modes[] = { "chain_nonoverflow", "chain_saturating", "parallel8" };
    satadd_t funcs[] = { zeno_tier1_baseline, zeno_tier1_candidate };
    for (unsigned mode = 0; mode < 3; mode++) {
        uint64_t a = workload(funcs[0], modes[mode], calls / 8);
        uint64_t b = workload(funcs[1], modes[mode], calls / 8);
        if (a != b) { fprintf(stderr, "semantic preflight failed for %s\n", modes[mode]); return 3; }
    }
    size_t n = (size_t)rounds * 3u * 4u;
    struct observation *records = calloc(n, sizeof(*records));
    if (!records) { perror("calloc"); return 2; }
    size_t used = 0;
    for (unsigned round = 0; round < rounds; round++) {
        /* Rotate mode order to avoid always testing one workload at a
         * particular time. ABBA and BAAB each place every candidate at both
         * an outer and an inner slot over alternating rounds. */
        for (unsigned slot = 0; slot < 3; slot++) {
            unsigned mode = (round + slot) % 3;
            unsigned first = (round + mode) % 2;
            const unsigned order[4] = { first, 1u - first, 1u - first, first };
            uint64_t expected_checksum = 0;
            for (unsigned position = 0; position < 4; position++) {
                unsigned candidate = order[position];
                uint64_t start = now_ns();
                uint64_t checksum = workload(funcs[candidate], modes[mode], calls);
                uint64_t elapsed = now_ns() - start;
                if (position == 0) expected_checksum = checksum;
                else if (checksum != expected_checksum) {
                    fprintf(stderr, "benchmark workload changed by implementation at round %u, mode %s\n",
                            round, modes[mode]);
                    free(records);
                    return 3;
                }
                records[used++] = (struct observation) { round, mode, position,
                    candidate, calls, elapsed, checksum };
            }
        }
    }
    puts("schema,round,mode,position,variant,calls,ns,checksum");
    for (size_t i = 0; i < used; i++) {
        const struct observation *o = &records[i];
        printf("tier1-bench-balanced-v2,%u,%s,%u,%s,%" PRIu64 ",%" PRIu64 ",%" PRIu64 "\n",
               o->round, modes[o->mode], o->position,
               o->candidate ? "candidate" : "baseline", o->calls, o->ns, o->checksum);
    }
    free(records);
    return 0;
}
