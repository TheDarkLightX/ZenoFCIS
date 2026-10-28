#define _GNU_SOURCE
#include <errno.h>
#include <fcntl.h>
#include <linux/sched.h>
#include <signal.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <sys/mman.h>
#include <sys/prctl.h>
#include <sys/ptrace.h>
#include <sys/resource.h>
#include <sys/socket.h>
#include <sys/stat.h>
#include <sys/syscall.h>
#include <sys/types.h>
#include <sys/uio.h>
#include <sys/wait.h>
#include <unistd.h>
#include <time.h>

static int fail(const char *what) { fprintf(stderr,"FAIL:%s errno=%d\n",what,errno); return 101; }
static int denied(long result, const char *what) {
    if (result < 0 && errno == EPERM) return 0;
    if (result >= 0 && (!strcmp(what,"inet-socket") || !strcmp(what,"unix-socket"))) close((int)result);
    return fail(what);
}
#define DENY(call,name) do { errno=0; if (denied((call),(name))) return 101; } while (0)
static int limit(int kind, rlim_t bound) {
    struct rlimit value;
    if (getrlimit(kind,&value) || value.rlim_cur != bound || value.rlim_max != bound) return fail("limit-readback");
    return 0;
}
static int protection_probe(int argc,char **argv) {
    if (argc != 3) return fail("arguments");
    if (!strcmp(argv[1],"spin")) { volatile uint64_t n=0; for (;;) n++; }
    if (!strcmp(argv[1],"wait")) { char c; ssize_t result=read(0,&c,1); return result<0 ? 101 : 0; }
    if (strcmp(argv[1],"probe")) return fail("mode");
    char byte=0;
    errno=0;
    if (read(200,&byte,1) != -1 || errno != EBADF) return fail("inherited-fd");
    if (getenv("ZENO_OWNER_CREDENTIAL") || getenv("HOME")) return fail("environment");
    if (prctl(PR_GET_NO_NEW_PRIVS,0,0,0,0) != 1 || prctl(PR_GET_SECCOMP,0,0,0,0) != 2) return fail("kernel-readback");
    if (limit(RLIMIT_CPU,2) || limit(RLIMIT_AS,64UL<<20) || limit(RLIMIT_FSIZE,16UL<<20)
        || limit(RLIMIT_NOFILE,32) || limit(RLIMIT_CORE,0)) return 101;
    int secret=open(argv[2],O_RDONLY);
    if (secret >= 0) { close(secret); return fail("private-file-read"); }
    int store=open(argv[2],O_WRONLY|O_TRUNC);
    if (store >= 0) { close(store); return fail("private-file-write"); }
    int proc=open("/proc/1/mem",O_RDONLY);
    if (proc >= 0) { close(proc); return fail("proc-memory"); }
    DENY(socket(AF_INET,SOCK_STREAM,0),"inet-socket");
    DENY(socket(AF_UNIX,SOCK_STREAM,0),"unix-socket");
    errno=0; pid_t child=fork();
    if (child==0) _exit(0);
    if (child>0) { (void)waitpid(child,NULL,0); return fail("fork"); }
    if (errno != EPERM) return fail("fork-error");
    DENY(syscall(SYS_ptrace,PTRACE_PEEKDATA,99999999,NULL,NULL),"ptrace");
    DENY(syscall(SYS_process_vm_readv,99999999,NULL,0,NULL,0,0),"process-vm");
    DENY(kill(99999999,0),"signal");
    DENY(syscall(SYS_unshare,CLONE_NEWUSER),"unshare");
    DENY(syscall(SYS_io_uring_setup,1,NULL),"io-uring");
    struct rlimit raised={RLIM_INFINITY,RLIM_INFINITY};
    DENY(syscall(SYS_prlimit64,0,RLIMIT_AS,&raised,NULL),"raise-limits");
    const char *blocked[]={"/extra","/dev/extra","/dev/shm/extra","/tmp/extra","/lib/extra","/worker","/lib64/ld-linux-x86-64.so.2","/lib/x86_64-linux-gnu/libc.so.6"};
    for (size_t i=0;i<sizeof(blocked)/sizeof(blocked[0]);i++) {
        int created=open(blocked[i],O_CREAT|O_WRONLY|O_TRUNC,0600);
        if (created>=0) { close(created); return fail("readonly-root"); }
    }
    int scratch=open("/scratch",O_RDWR);
    if (scratch<0 || write(scratch,"ok",2)!=2 || lseek(scratch,0,SEEK_SET)!=0 || read(scratch,&byte,1)!=1 || byte!='o') return fail("scratch-positive");
    if (signal(SIGXFSZ,SIG_IGN)==SIG_ERR || lseek(scratch,(16UL<<20)-1,SEEK_SET)<0 || write(scratch,"x",1)!=1) return fail("file-size-boundary");
    errno=0; if (write(scratch,"x",1)!=-1 || errno!=EFBIG) return fail("file-size-excess");
    close(scratch);
    void *small=mmap(NULL,8UL<<20,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);
    if (small==MAP_FAILED) return fail("memory-positive");
    memset(small,42,8UL<<20); munmap(small,8UL<<20);
    errno=0; void *big=mmap(NULL,64UL<<20,PROT_READ|PROT_WRITE,MAP_PRIVATE|MAP_ANONYMOUS,-1,0);
    if (big!=MAP_FAILED || errno!=ENOMEM) return fail("memory-excess");
    int fds[40], count=0;
    for (;count<40;count++) { fds[count]=open("/dev/null",O_RDONLY); if(fds[count]<0) break; }
    if (count!=29 || errno!=EMFILE) return fail("fd-limit");
    for(int i=0;i<count;i++) close(fds[i]);
    return 0;
}

/* Independent worker-side wire oracle: no Rust encoder/decoder is linked. */
static int transfer(int fd, unsigned char *bytes, size_t size, int writing) {
    size_t used=0;
    while(used<size) {
        ssize_t count=writing ? write(fd,bytes+used,size-used) : read(fd,bytes+used,size-used);
        if(count<0 && errno==EINTR) continue;
        if(count<=0) return fail("wire-transfer");
        used+=(size_t)count;
    }
    return 0;
}
static int ask(unsigned char sequence,unsigned int slot,unsigned char status,unsigned char detail) {
    unsigned char request[16]={'Z','F','W','1'}, reply[16];
    request[4]=sequence;
    for(int i=0;i<4;i++) request[12+i]=(unsigned char)(slot>>(8*i));
    if(transfer(1,request,16,1) || transfer(0,reply,16,0)) return 101;
    unsigned char expected[16]={'Z','F','R','1'};
    expected[4]=sequence;expected[12]=status;expected[13]=detail;
    if(memcmp(reply,expected,16)) return fail("wire-reply");
    return 0;
}
int main(int argc,char **argv) {
    if(argc!=3) return fail("worker-arguments");
    if(!strcmp(argv[1],"sleep")) {
        struct timespec remaining={20,0};
        while(nanosleep(&remaining,&remaining)) if(errno!=EINTR) return fail("sleep");
        return 0;
    }
    if(!strcmp(argv[1],"wait")) { unsigned char b;ssize_t n=read(0,&b,1);return n<0 ? 101 : 0; }
    if(!strcmp(argv[1],"spin")) { volatile uint64_t n=0;for(;;) n++; }
    if(!strcmp(argv[1],"spoof-control")) {
        unsigned char record[64]={'Z','F','N','E','N','D','0','1'};
        record[8]=1;
        return transfer(2,record,sizeof(record),1);
    }
    if(!strcmp(argv[1],"stderr")) {
        unsigned char bytes[256];memset(bytes,'x',sizeof(bytes));
        for(int i=0;i<16;i++) if(transfer(2,bytes,sizeof(bytes),1)) return 101;
        return 0;
    }
    if(!strcmp(argv[1],"truncated")) {
        unsigned char fragment[7]={'Z','F','W','1',0,0,0};
        return transfer(1,fragment,sizeof(fragment),1);
    }
    char *probe_argv[]={argv[0],"probe",argv[2]};
    if(protection_probe(3,probe_argv)) return 101;
    if(!strcmp(argv[1],"forged") || !strcmp(argv[1],"slot")) {
        unsigned char frame[16]={'Z','F','W','1'};
        if(!strcmp(argv[1],"forged")) frame[0]='X';
        else memset(frame+12,255,4);
        if(transfer(1,frame,16,1)) return 101;
        unsigned char b;ssize_t n=read(0,&b,1);
        return n==0 ? 0 : fail("forgery-accepted");
    }
    if(!strcmp(argv[1],"wrong-principal")) return ask(0,0,4,3);
    if(!strcmp(argv[1],"unknown")) {
        if(ask(0,0,1,0)) return 101;
        return ask(1,0,3,0);
    }
    if(!strcmp(argv[1],"lost-reply")) {
        unsigned char frame[16]={'Z','F','W','1'};
        if(transfer(1,frame,16,1)) return 101;
        if(close(0)) return fail("close-reply-reader");
        unsigned char witness[]="reader-closed\n";
        if(transfer(2,witness,sizeof(witness)-1,1)) return 101;
        struct timespec delay={2,0};
        if(nanosleep(&delay,NULL)) return fail("reply-delay");
        return 0;
    }
    if(ask(0,0,0,0)) return 101;
    if(!strcmp(argv[1],"once")) return 0;
    if(!strcmp(argv[1],"replay")) {
        unsigned char frame[16]={'Z','F','W','1'},byte;
        if(transfer(1,frame,16,1)) return 101;
        ssize_t n=read(0,&byte,1);
        return n==0 ? 0 : fail("replay-accepted");
    }
    if(strcmp(argv[1],"valid")) return fail("worker-mode");
    if(ask(1,0,3,0)) return 101;
    return ask(2,1,4,4);
}
