// Native A/B throughput microbenchmark of exact compiler-generated ELF objects.
// Scope: Linux x86-64 SysV; function-pointer call throughput, not end-to-end WASM.
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <inttypes.h>
#include <limits.h>
#include <sched.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

typedef int64_t (*satadd_fn)(int64_t, int64_t);
typedef struct {int64_t a, b;} pair;
static volatile uint64_t observable_sink = 0;
static uint64_t rng_state = UINT64_C(0x7c2b20261123dc57);
static uint64_t next64(void){uint64_t z=(rng_state+=UINT64_C(0x9e3779b97f4a7c15));z=(z^(z>>30))*UINT64_C(0xbf58476d1ce4e5b9);z=(z^(z>>27))*UINT64_C(0x94d049bb133111eb);return z^(z>>31);}
static uint64_t tick(void){struct timespec t; if(clock_gettime(CLOCK_MONOTONIC_RAW,&t)) {perror("clock");exit(1);}return (uint64_t)t.tv_sec*1000000000ull+(uint64_t)t.tv_nsec;}
static int64_t oracle(int64_t a,int64_t b){__int128 s=(__int128)a+(__int128)b;return (s>INT64_MAX)?INT64_MAX:(s<INT64_MIN)?INT64_MIN:(int64_t)s;}
static int choose_cpu(void){cpu_set_t set; if(sched_getaffinity(0,sizeof set,&set))return -1;for(int n=0;n<CPU_SETSIZE;n++){if(CPU_ISSET(n,&set)){cpu_set_t one;CPU_ZERO(&one);CPU_SET(n,&one);if(sched_setaffinity(0,sizeof one,&one))return -1;return n;}}return -1;}
static void fill(pair* p,size_t n,int mode){for(size_t i=0;i<n;i++){int64_t a,b;switch(mode){
 case 0: a=(int32_t)next64();b=(int32_t)next64();break; // no signed overflow
 case 1: a=INT64_MAX-(int64_t)(next64()&0xffffu);b=(int64_t)(next64()&0x7fffffffu)+0x10000;break; // positive overflow
 case 2: a=INT64_MIN+(int64_t)(next64()&0xffffu);b=-((int64_t)(next64()&0x7fffffffu)+0x10000);break; // negative overflow
 default: a=(int64_t)next64();b=(int64_t)next64();break; // uniform signed 64-bit
 }p[i]=(pair){a,b};}}
static void check(satadd_fn* f,pair* p,size_t n){for(size_t i=0;i<n;i++){int64_t expected=oracle(p[i].a,p[i].b);for(int t=0;t<2;t++)if(f[t](p[i].a,p[i].b)!=expected){fprintf(stderr,"oracle mismatch: %zu %d\n",i,t);exit(3);}}}
static uint64_t run(satadd_fn f,pair* p,size_t n,int repetitions){uint64_t val=0;uint64_t before=tick();for(int j=0;j<repetitions;j++){for(size_t i=0;i<n;i++){val+=(uint64_t)f(p[i].a,p[i].b);}}uint64_t elapsed=tick()-before;observable_sink ^= val;return elapsed;}
int main(int argc,char** argv){if(argc!=5){fprintf(stderr,"usage: %s base.so optimized.so blocks repetitions\n",argv[0]);return 2;}int blocks=atoi(argv[3]),repetitions=atoi(argv[4]);if(blocks<4||blocks>256||repetitions<2||repetitions>1000)return 2;void* lib[2]={dlopen(argv[1],RTLD_NOW|RTLD_LOCAL),dlopen(argv[2],RTLD_NOW|RTLD_LOCAL)};if(!lib[0]||!lib[1]){fprintf(stderr,"dlopen failed: %s\n",dlerror());return 2;}satadd_fn f[2];for(int i=0;i<2;i++){void* s=dlsym(lib[i],"%tier1_signed_satadd");if(!s){fprintf(stderr,"symbol missing: %s\n",dlerror());return 2;}f[i]=(satadd_fn)s;}const size_t n=32768;pair* p=aligned_alloc(64,n*sizeof(pair));if(!p)return 2;
 int cpu=choose_cpu();fprintf(stderr,"pin_cpu=%d; samples=%zu; reps=%d; blocks=%d; symbol=%%tier1_signed_satadd; timer=CLOCK_MONOTONIC_RAW; all samples checked against int128 oracle\n",cpu,n,repetitions,blocks);
 puts("workload,block,variant,order,elapsed_ns,calls,checksum");
 for(int mode=0;mode<4;mode++){
  fill(p,n,mode);check(f,p,n);
  run(f[0],p,n,2);run(f[1],p,n,2);
  for(int block=0;block<blocks;block++){
    int first=(block&1); // fixed alternating order across paired blocks
    for(int k=0;k<2;k++){int variant=k?1-first:first;uint64_t elapsed=run(f[variant],p,n,repetitions);printf("%s,%d,%s,%d,%" PRIu64 ",%zu,%" PRIu64 "\n",(const char*[]){"no_overflow","positive_overflow","negative_overflow","mixed_random"}[mode],block,variant?"experimental":"baseline",k,elapsed,n*(size_t)repetitions,(uint64_t)observable_sink);}
  }
 }
 free(p);fprintf(stderr,"sink=%" PRIu64 "\n",(uint64_t)observable_sink);return 0;
}
