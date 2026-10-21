import assert from 'node:assert/strict';
import {test} from 'node:test';
import {spawn} from 'node:child_process';
import {openSync,closeSync} from 'node:fs';
import {fileURLToPath} from 'node:url';
const launcher=fileURLToPath(new URL('./exec_chrome.py',import.meta.url));
const probe=`
import json,os
open_fds=[]
for fd in range(128):
 try:os.fstat(fd);open_fds.append(fd)
 except OSError:pass
assert os.read(3,4)==b'ping'
os.write(4,b'pong')
print(json.dumps(open_fds))
`;
async function descriptors(useLauncher){
  const extra=openSync(new URL('../public/index.html',import.meta.url),'r');
  let child;
  try{
    child=spawn('python3',[...(useLauncher?[launcher,'python3']:[]),'-S','-c',probe],
      {stdio:['ignore','pipe','pipe','pipe','pipe',extra,extra,extra,extra]});
  }finally{closeSync(extra);}
  let output='',reply='',errors='';
  child.stdout.setEncoding('utf8');child.stdout.on('data',data=>output+=data);
  child.stderr.setEncoding('utf8');child.stderr.on('data',data=>errors+=data);
  child.stdio[4].setEncoding('utf8');child.stdio[4].on('data',data=>reply+=data);
  child.stdio[3].end('ping');
  const code=await new Promise((resolve,reject)=>{child.on('error',reject);child.on('close',resolve);});
  assert.equal(code,0,errors);assert.equal(reply,'pong','both DevTools directions survive exec');
  return JSON.parse(output);
}
test('exec_boundary_closes_planted_extra_fds_and_preserves_devtools_3_4',async()=>{
  assert.deepEqual(await descriptors(true),[0,1,2,3,4]);
  const leaking=await descriptors(false);
  assert.ok(leaking.includes(5)&&leaking.includes(8),'bypassing the exec boundary leaks the planted inherited descriptors');
  assert.notDeepEqual(leaking,[0,1,2,3,4]);
});
