# Disposable x86-64 Linux test launcher, derived from the c25 independent review.
import ctypes as C, errno, os, platform, sys
assert platform.machine()=='x86_64'
class F(C.Structure): _fields_=[('code',C.c_ushort),('jt',C.c_ubyte),('jf',C.c_ubyte),('k',C.c_uint)]
class P(C.Structure): _fields_=[('len',C.c_ushort),('filter',C.POINTER(F))]
# Restrict only this disposable CLI tree; the external harness remains unaffected.
if sys.argv[1]=='deny-killpg':
 ins=[(0x20,0,0,0),(0x15,0,3,62),(0x20,0,0,16),(0x45,0,1,0x80000000),(0x06,0,0,0x50000|errno.EPERM),(0x06,0,0,0x7fff0000)]
elif sys.argv[1]=='deny-group-absence':
 ins=[(0x20,0,0,0),(0x15,0,5,62),(0x20,0,0,16),(0x45,0,3,0x80000000),(0x20,0,0,24),(0x15,0,1,0),(0x06,0,0,0x50000|errno.EPERM),(0x06,0,0,0x7fff0000)]
elif sys.argv[1]=='deny-waitid':
 ins=[(0x20,0,0,0),(0x15,0,1,247),(0x06,0,0,0x50000|errno.EPERM),(0x06,0,0,0x7fff0000)]
elif sys.argv[1]=='noop-setrlimit':
 ins=[(0x20,0,0,0),(0x15,0,3,302),(0x20,0,0,32),(0x15,1,0,0),(0x06,0,0,0x50000),(0x06,0,0,0x7fff0000)]
elif sys.argv[1].startswith('noop-limit-'):
 resource=int(sys.argv[1].rsplit('-',1)[1])
 ins=[(0x20,0,0,0),(0x15,0,5,302),(0x20,0,0,24),(0x15,0,3,resource),(0x20,0,0,32),(0x15,1,0,0),(0x06,0,0,0x50000),(0x06,0,0,0x7fff0000)]
else:raise ValueError(sys.argv[1])
a=(F*len(ins))(*(F(*x) for x in ins));p=P(len(ins),a);libc=C.CDLL(None,use_errno=True)
assert libc.prctl(38,1,0,0,0)==0, C.get_errno()
assert libc.prctl(22,2,C.byref(p),0,0)==0,C.get_errno()
os.execv(sys.argv[2],sys.argv[2:])
