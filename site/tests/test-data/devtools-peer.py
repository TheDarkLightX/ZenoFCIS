#!/usr/bin/env python3
"""Scripted protocol peer, not Chrome: test runner session routing over fd 3/4."""
import base64,json,os,sys
from pathlib import Path
native=json.loads(Path(os.environ['ZENOUI_TEST_NATIVE']).read_text())['search']
if os.environ.get('ZENOUI_TEST_CORRUPT_RESULT'):
    native['first_difference']['ordinal']+=1
out=os.fdopen(4,'wb',buffering=0)
def send(value):out.write(json.dumps(value).encode()+b'\0')
def event(method,params,session='page'):
    send(dict(method=method,params=params,sessionId=session))
def attach(kind,session,target):
    event('Target.attachedToTarget',dict(sessionId=session,targetInfo=dict(type=kind,targetId=target)))
def request(name,url,method='GET'):
    event('Fetch.requestPaused',dict(requestId=name,request=dict(method=method,url=url)))
fulfilled=set();denied=set();enabled=False;worker_started=False;waiting=None
reader=os.fdopen(3,'rb',buffering=0)
buffer=b''
while True:
    chunk=reader.read(65536)
    if not chunk:break
    buffer+=chunk
    while b'\0' in buffer:
        raw,buffer=buffer.split(b'\0',1);command=json.loads(raw)
        name=command['method'];session=command.get('sessionId');params=command['params'];result={}
        if name=='Target.createTarget':
            result={'targetId':'test-page'};attach('page','page','test-page')
        elif name=='Fetch.enable':
            if session!='page':
                send(dict(id=command['id'],error=dict(code=-32601,message="'Fetch.enable' wasn't found")));continue
            enabled=True
        elif name=='Target.setAutoAttach':
            assert params==dict(autoAttach=True,waitForDebuggerOnStart=True,flatten=True)
        elif name=='Page.navigate':
            assert enabled
            request('document',params['url'])
        elif name=='Fetch.fulfillRequest':
            assert session=='page'
            assert params['responseCode']==200
            assert base64.b64decode(params['body'])
            fulfilled.add(params['requestId'])
        elif name=='Fetch.failRequest':
            assert session=='page' and params['errorReason']=='BlockedByClient'
            denied.add(params['requestId'])
        elif name=='Runtime.runIfWaitingForDebugger' and session=='worker':
            worker_started=True;request('wasm','https://zenoui.test/ZenoFCIS/factory.wasm')
        elif name=='Runtime.evaluate':
            expression=params['expression']
            if 'querySelectorAll' in expression:
                result={'result':{'value':'document' in fulfilled}}
            elif 'factory-result' in expression:
                waiting=command['id'];attach('worker','worker','test-worker');continue
            elif 'Promise.all' in expression:
                waiting=command['id']
                request('unknown','https://zenoui.test/unknown-asset')
                request('external','https://external.invalid/no');continue
            else:raise AssertionError(expression)
        elif name=='Browser.close':
            send(dict(id=command['id'],result={}));sys.exit(0)
        send(dict(id=command['id'],result=result))
        if waiting and worker_started and 'wasm' in fulfilled:
            if {'unknown','external'}<=denied:
                send(dict(id=waiting,result={'result':{'value':[True,True]}}));waiting=None
            elif name=='Fetch.fulfillRequest':
                send(dict(id=waiting,result={'result':{'value':native}}));waiting=None
