#!/usr/bin/env python3
"""Loopback HTTP journey, reopen audit and receipt-identical fresh-store replay."""
import argparse,http.client,json,os,selectors,subprocess,time
from pathlib import Path
p=argparse.ArgumentParser(description=__doc__);p.add_argument('--transport',choices=['tcp','stdio'],default='tcp');p.add_argument('--app',required=True);p.add_argument('--work',required=True,type=Path);a=p.parse_args()
a.work.mkdir(parents=True,exist_ok=True);database=a.work/'service.db';binary=str(Path(a.app).resolve());log=(a.work/'server.log').open('w');steps=[]
def start():
 if a.transport=='stdio':return None,None,None
 proc=subprocess.Popen([binary,'serve',str(database),'0'],stdout=subprocess.PIPE,stderr=log,text=True)
 selector=selectors.DefaultSelector();selector.register(proc.stdout,selectors.EVENT_READ)
 if not selector.select(60):proc.terminate();raise AssertionError('server startup timeout')
 first=json.loads(proc.stdout.readline());host,port=first['listening'].split(':');return proc,host,int(port)
def send(method,path,body=None):
 if a.transport=='stdio':
  data=json.dumps(body).encode() if body is not None else b''
  request=f'{method} {path} HTTP/1.1\r\nHost: localhost\r\nContent-Length: {len(data)}\r\n\r\n'.encode()+data
  result=subprocess.run([binary,'http-stdio',str(database)],input=request,capture_output=True)
  assert result.returncode==0,result.stderr
  headers,body=result.stdout.split(b'\r\n\r\n',1)
  return int(headers.split(b' ')[1]),json.loads(body)
 connection=http.client.HTTPConnection(host,port,timeout=30)
 connection.request(method,path,body=json.dumps(body) if body is not None else None,headers={'Content-Type':'application/json'})
 response=connection.getresponse();result=json.loads(response.read());status=response.status;connection.close();return status,result
def command(key,op,target,status=200):
 actual,body=send('POST','/request',{'principal':'local','key':key,'values':[op,target]})
 assert actual==status,(key,actual,body)
 steps.append({'key':key,'status':actual,'body':body});return body
proc,host,port=start()
try:
 assert send('GET','/state')[1]['version']==0
 for key,op,target in [('publish-one',0,1),('anchor-one',3,1),('publish-two',0,2),('anchor-two',3,1),('deliver',4,1),('read-before',6,1),('revoke-one',2,1)]:command(key,op,target)
 before=send('GET','/state')[1];denied=command('revoked-read',5,1,422);assert denied['error'].startswith('reject:mailbox:203')
 assert send('GET','/state')[1]==before,'whole refusal must leave state/version/root unchanged'
 accepted=command('active-read',6,1);recovered=command('active-read',6,1)
 assert recovered['recovered'] and recovered['receipt']==accepted['receipt'] and recovered['version']==accepted['version']
 assert send('POST','/request',{'principal':'local','key':'malformed','values':[True,1]})[0]==400
 assert send('POST','/request',{'principal':'local','key':'active-read','values':[6,2]})[0]==422
 final=send('GET','/state')[1];assert final['version']==8
finally:
 if proc is not None:proc.terminate();proc.wait(timeout=10)
proc,host,port=start()
try:assert send('GET','/state')[1]==final,'reopen must audit same complete tuple'
finally:
 if proc is not None:proc.terminate();proc.wait(timeout=10);log.close()
replay=subprocess.run([binary,'replay',str(database),str(a.work/'fresh.db')],capture_output=True,text=True)
assert replay.returncode==0,replay.stderr
assert json.loads(replay.stdout)['receipts']==8
for path in [database,a.work/'fresh.db']:
 audit=subprocess.run([binary,'audit',str(path)],capture_output=True,text=True)
 assert audit.returncode==0,audit.stderr
 assert json.loads(audit.stdout)['version']==8
report={'status':'passed','transport':a.transport,'tcp_qualified':a.transport=='tcp','committed_receipts':8,'refused_requests':1,'recovery_receipts_equal':True,'reopen_equal':True,'replayed_receipts':8,'steps':steps}
(a.work/'journey.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({k:v for k,v in report.items() if k!='steps'},indent=2))
