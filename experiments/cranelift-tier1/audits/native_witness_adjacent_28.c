#include <stdint.h>
#include <stdio.h>
#include <inttypes.h>
#include <signal.h>
#include <sys/types.h>
#include <sys/wait.h>
#include <unistd.h>
#include <stdlib.h>

/* Link the *archived*, unchanged code bytes after symbol-only renaming. */
extern uint64_t tier1_original(uint64_t,uint64_t);
extern uint64_t tier1_patched(uint64_t,uint64_t);

static uint64_t next(uint64_t *s) {
    uint64_t x=*s;
    x^=x>>12; x^=x<<25; x^=x>>27;
    *s=x;
    return x*UINT64_C(2685821657736338717);
}
static uint64_t rol(uint64_t r) { return (r<<17)|(r>>47); }
static uint64_t oracle(uint64_t x,uint64_t y) { return (x/y)^rol(x%y); }
static void check(uint64_t x,uint64_t y) {
    uint64_t e=oracle(x,y), a=tier1_original(x,y), b=tier1_patched(x,y);
    if(a!=e || b!=e) {
        fprintf(stderr,"MISMATCH x=%"PRIu64" y=%"PRIu64" oracle=%"PRIu64" orig=%"PRIu64" patched=%"PRIu64"\n",x,y,e,a,b);
        exit(3);
    }
}
static int sigfpe_check(int patched) {
    pid_t pid=fork();
    if(pid<0) return 0;
    if(pid==0) {
        volatile uint64_t a=patched?tier1_patched(1,0):tier1_original(1,0);
        (void)a;
        _exit(0);
    }
    int status=0;
    if(waitpid(pid,&status,0)!=pid) return 0;
    return WIFSIGNALED(status)&&WTERMSIG(status)==SIGFPE;
}
int main(void) {
    uint64_t edges[]={0,1,2,3,7,9,15,17,UINT64_C(65535),UINT64_C(65536),
                      UINT64_C(4294967295),UINT64_C(4294967296),
                      UINT64_C(9223372036854775807),UINT64_C(9223372036854775808),
                      UINT64_MAX-1,UINT64_MAX};
    size_t n=sizeof(edges)/sizeof(edges[0]); uint64_t count=0;
    for(size_t i=0;i<n;i++) for(size_t j=0;j<n;j++) if(edges[j]) {
        check(edges[i],edges[j]); count++;
    }
    uint64_t s=UINT64_C(202611090349);
    for(int i=0;i<100000;i++) {
        uint64_t x=next(&s),y=next(&s);
        if(!y)y=1;
        check(x,y);count++;
    }
    int a=sigfpe_check(0),b=sigfpe_check(1);
    printf("native_nonzero_input_pairs=%"PRIu64"; per_variant_executions=%"PRIu64"; original_divzero_SIGFPE=%d; patched_divzero_SIGFPE=%d\n",count,count,a,b);
    return a&&b?0:2;
}
