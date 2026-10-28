#!/usr/bin/env python3
"""Untrusted candidate producer. Only the separate native checker grants evidence."""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REFERENCE = ROOT / 'examples/runtime-refinement/task-intents/reference'


def digest(data):
    return hashlib.sha256(data).hexdigest()


def dump(value):
    return json.dumps(value, sort_keys=True, separators=(',', ':')) + '\n'


def decode(raw):
    return [(raw // 4) % 4, raw % 4, (raw // 16) % 3, raw // 48]


def encode(s):
    return s[1] + 4*s[0] + 16*s[2] + 48*s[3]


def valid(s):
    p = s[2]
    return (p != 0 or s[3] == 0) and (p == 0 or s[p-1] in (1, 2)) and (s[3] == 0 or (p != 0 and s[p-1] == 2))


def transition(raw, action):
    s = decode(raw)
    post = s.copy()
    reason, applied = None, 0
    if action < 2:
        if s[action] == 0: post[action] = 1
        else: reason = 200
    elif action < 4:
        slot = action - 2
        if s[2] == 0 and s[slot] in (1, 2): post[2:] = [slot+1, 0]
        else: reason = 200
    elif action == 4:
        if s[2] and s[s[2]-1] in (1, 2):
            applied = s[2] if s[s[2]-1] == 1 else 0
            post[s[2]-1], post[3] = 2, 1
        else: reason = 200
    elif action == 5:
        if s[3] and s[2]:
            post[s[2]-1], post[2], post[3] = 3, 0, 0
        else: reason = 200
    elif action in (6, 7): post[2:] = [0, 0]
    else: reason = {8:201, 9:201, 10:202, 11:203}[action]
    deliveries = [] if not applied else [{'ordinal':0,'channel':300,'destination':'local-test','payload':{'140':str(applied)},'idempotency_ordinal':0}]
    return {'when':f'state == {raw} && action == {action}', 'class':'Accept' if reason is None else 'Reject',
            'reason':reason, 'post':{'110':str(encode(post))} if reason is None else {}, 'outbox':deliveries}


def compressed_cases():
    # Equivalent rows grouped by affine successor delta; no checker code is imported.
    groups = {}
    for raw in range(96):
        if not valid(decode(raw)):
            continue  # Law 500 independently refuses every invalid raw input.
        for action in range(12):
            row = transition(raw, action)
            delta = int(row['post']['110'])-raw if row['class']=='Accept' else 0
            key = (action, row['reason'], delta, dump(row['outbox']))
            groups.setdefault(key, []).append(raw)
    result = []
    for (action, reason, delta, deliveries), states in groups.items():
        result.append({'when':f'action == {action} && ('+' || '.join(f'state == {s}' for s in states)+')',
            'class':'Accept' if reason is None else 'Reject', 'reason':reason,
            'post':{'110': f'state + {delta}' if delta >= 0 else f'state - {-delta}'} if reason is None else {},
            'outbox':json.loads(deliveries)})
    return result + [{'when':'true','class':'Reject','reason':200,'post':{},'outbox':[]}]


def candidate_files():
    lawful = [s for s in range(96) if valid(decode(s))]
    pre = ' || '.join(f'pre.100.110 == {s}' for s in lawful)
    post = ' || '.join(f'post.100.110 == {s}' for s in lawful)
    project = f'''zeno 1;
project 1 task_intent_packed;
namespace 2 task_intents;
type 100 state State;
type 101 command Event;
type 102 context Context;
type 103 destination Receiver;
type 104 payload Applied;
type 106 int Packet in 0..=2;
type 108 int Action in 0..=11;
type 109 int Unit in 0..=0;
type 115 int Packed in 0..=95;
field 110 100 packed 115;
field 120 101 action 108;
field 130 102 unit 109;
field 140 104 slot 106;
reason 200 unavailable precedence 0;
reason 201 invalid_receipt_capability precedence 1;
reason 202 payload_conflict precedence 2;
reason 203 capacity precedence 3;
channel 300 receiver_applied destination 103 payload 104;
component 400 protocol {{ owns 100; reads pre.100; writes post.100; contexts context.102; outbox outbox.300; budget steps 65536; }}
merge [400];
law 500 valid_pre on any = {pre};
law 501 valid_post on commit, genesis = {post};
'''
    policy = {'schema':'zeno-fcis/template-declarative-policy/2','template':'task-intent-packed',
        'roots':{'state':100,'command':101,'context':102},'leaf_bindings':{'103':['Text',1,16]},
        'variables':{'state':'pre.100.110','action':'command.101.120'},
        'cases':compressed_cases(),
        'genesis':{'110':0},'law_kinds':{'500':'AuthoritySubjectRecipient','501':'StateInvariant'}}
    files = {'concrete/project.zeno':project.encode(), 'concrete/v2/policy.json':dump(policy).encode()}
    mapping = {'schema':'zeno-fcis/contract-refinement/1','invalid_state_law':500,
        'abstraction':[((p1*4+p2)*3+p)*2+c for p1,p2,p,c in map(decode,range(96))]}
    for label,path in [('abstract',REFERENCE),('concrete',None)]:
        for kind,name in [('project','project.zeno'),('policy','v2/policy.json')]:
            data=(path/name).read_bytes() if path else files[f'concrete/{name}']
            mapping[f'{label}_{kind}_sha256']=digest(data)
    files['abstraction.json']=dump(mapping).encode()
    certificate={'schema':'zeno-fcis/task-transport-candidate/1','profile':'task-status-sqlite-local-test/1',
        'claim':'finite-transport-representation-only','files':{p:digest(b) for p,b in files.items()}}
    files['candidate.json']=dump(certificate).encode()
    return files


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out',type=Path,required=True)
    args=parser.parse_args()
    if args.out.exists(): parser.error('output must not exist')
    files=candidate_files()
    args.out.mkdir(parents=True)
    for name,data in files.items():
        path=args.out/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(data)
    print('Candidate emitted; untrusted, not evidence and not an installable application.')

if __name__=='__main__': main()
