#define _POSIX_C_SOURCE 200809L
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <inttypes.h>
#include <sys/wait.h>
#include <unistd.h>
#include <signal.h>
#include <time.h>
#include <string.h>

extern uint64_t tier1_two_div(uint64_t, uint64_t);
extern uint64_t tier1_one_div(uint64_t, uint64_t);
static uint64_t seed = UINT64_C(0x9e3779b97f4a7c15);
static uint64_t random64(void){
  uint64_t x=seed; x ^= x<<13; x ^= x>>7; x ^= x<<17; seed=x; return x;
}
static uint64_t ref(uint64_t x, uint64_t y){
  uint64_t q=x/y,r=x%y; return q ^ ((r<<17)|(r>>47));
}
static uint64_t xs[256],ys[256];
static void check_pair(uint64_t x,uint64_t y){
  if(!y)abort();
  uint64_t expected=ref(x,y),a=tier1_two_div(x,y),b=tier1_one_div(x,y);
  if(a!=expected||b!=expected){
    fprintf(stderr,"MISMATCH x=%"PRIu64" y=%"PRIu64" ref=%"PRIu64" two=%"PRIu64" one=%"PRIu64"\n",x,y,expected,a,b);exit(1);
  }
}
static void trap(void){
  uint64_t (*functions[])(uint64_t,uint64_t)={tier1_two_div,tier1_one_div};
  for(size_t k=0;k<2;k++){
    pid_t p=fork(); if(p<0){perror("fork");exit(1);} if(p==0){
      volatile uint64_t result=functions[k](UINT64_C(0xffffffffffffffff),0);(void)result;_exit(0);
    }
    int status; if(waitpid(p,&status,0)!=p){perror("waitpid");exit(1);}
    if(!WIFSIGNALED(status)||WTERMSIG(status)!=SIGFPE){fprintf(stderr,"TRAP FAIL %zu status %d\n",k,status);exit(1);}
  }
}
static double elapsed(const struct timespec *a,const struct timespec *b){return (b->tv_sec-a->tv_sec)*1e9+(b->tv_nsec-a->tv_nsec);}
static volatile uint64_t sink=0;
static double batch(uint64_t (*f)(uint64_t,uint64_t),size_t count){
  uint64_t check=0; struct timespec a,b;
  if(clock_gettime(CLOCK_MONOTONIC_RAW,&a)!=0)exit(1);
  for(size_t k=0;k<count;k++){size_t j=k&255;check+=f(xs[j],ys[j]);}
  if(clock_gettime(CLOCK_MONOTONIC_RAW,&b)!=0)exit(1);
  sink ^=check;return elapsed(&a,&b);
}
int main(int argc,char**argv){
  size_t budget=argc>1?(size_t)strtoull(argv[1],0,10):100000;
  if(budget<10000||budget>10000000){fputs("bad budget\n",stderr);return 2;}
  uint64_t edges[]={0,1,2,3,7,UINT64_C(0x8000000000000000),UINT64_C(0x7fffffffffffffff),UINT64_MAX,UINT64_MAX-1};
  size_t tested=0;
  for(size_t i=0;i<sizeof(edges)/sizeof(edges[0]);i++)for(size_t j=1;j<sizeof(edges)/sizeof(edges[0]);j++){check_pair(edges[i],edges[j]);tested++;}
  for(size_t k=0;k<100000;k++){uint64_t x=random64(),y=random64()|1;check_pair(x,y);tested++;}
  for(size_t j=0;j<256;j++){xs[j]=random64();ys[j]=random64()|1;}
  trap();
  puts("schema=zeno-fcis/tier1-x86-dual-div-native-standalone/1");
  printf("finite_nonzero_pairs=%zu\n",tested);
  puts("zero_divisor_checks=2 SIGFPE, fork-isolated");
  puts("semantics=u64 /,% with nonzero divisor; rotate remainder 17; xor quotient; SysV x86_64; output rax; clobber caller registers and flags; no poison source input");
  printf("calls_per_sample=%zu\n",budget);
  puts("block,position,variant,ns");
  for(int block=0;block<16;block++){
    uint64_t (*first)(uint64_t,uint64_t)=block%2?tier1_one_div:tier1_two_div;
    uint64_t (*second)(uint64_t,uint64_t)=block%2?tier1_two_div:tier1_one_div;
    const char *n1=block%2?"one":"two",*n2=block%2?"two":"one";
    printf("%d,0,%s,%.0f\n",block,n1,batch(first,budget));
    printf("%d,1,%s,%.0f\n",block,n2,batch(second,budget));
    printf("%d,2,%s,%.0f\n",block,n2,batch(second,budget));
    printf("%d,3,%s,%.0f\n",block,n1,batch(first,budget));
  }
  fprintf(stderr,"sink=%"PRIu64"\n",sink);
}
