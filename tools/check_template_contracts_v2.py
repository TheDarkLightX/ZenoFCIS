#!/usr/bin/env python3
"""Emit/check reviewed, data-only V2 template contracts; never evaluate decisions.

The .zeno source supplies the complete named schema and original law predicates.
The template-local JSON supplies reviewed branch declarations and scalar bounds.
The library, not this emitter, encodes/adopts the full canonical policy.
"""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]
TEMPLATES = ('durable-counter', 'inventory-reservation', 'order-fulfillment',
             'account-lockout', 'withdrawal-queue', 'agent-treasury-guard',
             'prepared-counter', 'compliance-gateway')
TOKEN = re.compile(r'\s*(->|&&|\|\||==|!=|<=|>=|[!<>()+*,\-/]|[A-Za-z_][A-Za-z_0-9.]*|[0-9]+)')
PRECEDENCE = {'->':1, '||':2, '&&':3, '==':4, '!=':4, '<':5, '<=':5,
              '>':5, '>=':5, '+':6, '-':6, '*':7}


def parse(text):
    if isinstance(text, bool): return ('bool', text)
    if isinstance(text, int): return ('int', text)
    tokens, position = [], 0
    while position < len(text):
        m = TOKEN.match(text, position)
        if not m: raise ValueError(f'Unsupported expression suffix {text[position:]!r}')
        tokens.append(m[1]); position=m.end()
    index=0
    def expr(level=0):
        nonlocal index
        t=tokens[index]; index+=1
        if t == '(':
            left=expr(); assert tokens[index]==')'; index+=1
        elif t in ('!', '-'):
            left=('not' if t=='!' else 'neg', expr(8))
        elif t.isdigit(): left=('int',int(t))
        elif t in ('true','false'): left=('bool',t=='true')
        elif index<len(tokens) and tokens[index]=='(':
            index+=1; args=[expr()]
            while tokens[index]==',': index+=1; args.append(expr())
            assert tokens[index]==')'; index+=1
            assert (t,len(args)) in (('choose',3),('div_floor',2),('div_ceil',2))
            left=(t,*args)
        else: left=('name',t)
        while index<len(tokens) and PRECEDENCE.get(tokens[index],-1)>=level:
            op=tokens[index]; index+=1
            right=expr(PRECEDENCE[op]+(op!='->'))
            left=(op,left,right)
        return left
    value=expr(); assert index==len(tokens), (text,tokens[index:]); return value


def rust_bytes(s): return 'b'+json.dumps(s,ensure_ascii=True)
def array(items): return '&['+', '.join(items)+']'


class Project:
    def __init__(self,name):
        self.name=name; self.base=ROOT/'crates/zeno-fcis-cli/templates'/name
        self.policy=json.loads((self.base/'v2/policy.json').read_text())
        assert self.policy['schema']=='zeno-fcis/template-declarative-policy/2'
        assert self.policy['template']==name
        assert self.policy['roots']=={'state':100,'command':101,'context':102}
        text=re.sub(r'//[^\n]*','',(self.base/'project.zeno').read_text())
        self.profile=re.search(r'project \d+ (\w+);',text)[1]
        self.types={}
        for m in re.finditer(r'type (\d+) (\w+) (\w+)(?: in (-?\d+)\.\.=(-?\d+))?;',text):
            tid=int(m[1]); leaf=self.policy['leaf_bindings'].get(m[1])
            if m[4]: leaf=['I128',int(m[4]),int(m[5])]
            self.types[tid]={'name':m[3],'leaf':leaf,'fields':[],'variants':[]}
        for m in re.finditer(r'field (\d+) (\d+) (\w+) (\d+);',text):
            self.types[int(m[2])]['fields'].append((int(m[1]),m[3],int(m[4])))
        for m in re.finditer(r'variant (\d+) (\d+) (\w+) none;',text):
            self.types[int(m[2])]['variants'].append((int(m[1]),m[3]))
        for t in self.types.values():
            t['fields'].sort(); t['variants'].sort()
            assert sum(bool(t[k]) for k in ('leaf','fields','variants'))==1, t
        self.channels=[tuple(map(int,m)) for m in re.findall(r'channel (\d+) \w+ destination (\d+) payload (\d+);',text)]
        self.reasons=[int(v) for v in re.findall(r'reason (\d+) ',text)]
        self.laws=[(int(i),scope.strip(),parse(body.strip())) for i,scope,body in re.findall(r'law (\d+) \w+ on ([^=]+)= ([^;]+);',text)]
        self.variables={k:parse(v) for k,v in self.policy['variables'].items()}
        self.inputs=[]
        for source,tid in [('pre',100),('command',101),('context',102)]:
            fields=self.types[tid]['fields']
            if fields:
                self.inputs.extend((f'{source}.{tid}.{fid}',fty) for fid,_,fty in fields)
            else: self.inputs.append((f'{source}.{tid}',tid))
    def expand(self,ast,stack=()):
        if ast[0]=='name' and ast[1] in self.variables:
            assert ast[1] not in stack
            return self.expand(self.variables[ast[1]],stack+(ast[1],))
        if ast[0] in ('name','bool','int'): return ast
        return (ast[0],*(self.expand(v,stack) for v in ast[1:]))
    def kind(self,tid):
        t=self.types[tid]
        return 'Sum' if t['variants'] else t['leaf'][0]
    def input_leaf(self,tid,wide=False):
        t=self.types[tid]; kind=self.kind(tid)
        if kind=='Sum':
            vs=t['variants']; codes=[x[0] for x in vs]
            return f'InputLeaf::Sum {{ type_id: {tid}, min: {min(codes)}, max: {max(codes)}, variants: vec!['+', '.join(f'InputVariant {{ id: {v[0]}, code: {c} }}' for v,c in zip(vs,codes))+'] }'
        if kind=='Bool': return 'InputLeaf::Bool'
        assert kind=='I128'
        lo,hi=(-9223372036854775808,9223372036854775807) if wide else t['leaf'][1:]
        return f'InputLeaf::I128 {{ min: {lo}, max: {hi} }}'
    def domain(self,tid):
        kind=self.kind(tid); t=self.types[tid]
        if kind=='Sum': return f'c::Domain::Sum {{ type_id: {tid}, variants: &{[i for i,_ in t["variants"]]} }}'
        if kind=='I128': return f'c::Domain::I128 {{ min: {t["leaf"][1]}, max: {t["leaf"][2]} }}'
        return 'c::Domain::'+kind
    def atom(self,value,tid,namespace='c'):
        kind=self.kind(tid)
        if kind=='Sum': return f'{namespace}::Atom::Sum {{ type_id: {tid}, variant: {int(value)} }}'
        if kind=='Bool': return f'{namespace}::Atom::Bool({str(bool(value)).lower()})'
        if kind=='Text': return f'{namespace}::Atom::Text({rust_bytes(value)})'
        assert kind=='I128'; return f'{namespace}::Atom::I128({int(value)})'
    def scalar_domain(self,tid,wide=False):
        t=self.types[tid]
        if self.kind(tid)=='Bool': return 'ScalarDomain::Bool'
        if wide: lo,hi=-9223372036854775808,9223372036854775807
        elif t['variants']: lo,hi=t['variants'][0][0],t['variants'][-1][0]
        else: lo,hi=t['leaf'][1:]
        return f'ScalarDomain::Int {{ min: {lo}, max: {hi} }}'


class Graph:
    def __init__(self,project,law=False):
        self.p=project; self.law=law; self.nodes=[]; self.cache={}
        self.prefix='l::Op' if law else 'Op'
    def put(self,text,kind):
        key=(text,kind)
        if key not in self.cache: self.cache[key]=len(self.nodes); self.nodes.append(text)
        return self.cache[key],kind
    def op(self,tag,*args,kind='bool'):
        return self.put(f'{self.prefix}::{tag}('+', '.join(str(a[0] if isinstance(a,tuple) else a) for a in args)+')',kind)
    def literal(self,value,kind=None):
        if kind is None: kind='bool' if isinstance(value,bool) else 'int'
        word=str(value).lower()
        if self.law: return self.put(f'l::Op::Literal(l::Atom::{"Bool" if kind=="bool" else "I128"}({word}))',kind)
        return self.put(f'Op::{"Bool" if kind=="bool" else "Int"}({word})',kind)
    def boolean(self,n):
        return n if n[1]=='bool' else self.op('Eq',n,self.literal(1))
    def numeric(self,n):
        if n[1]=='int': return n
        if self.law: return self.op('ToI128',n,kind='int')
        return self.op('Select',n,self.literal(1),self.literal(0),kind='int')
    def observe(self,selector,kind='int',guard=None,default='l::Atom::I128(0)'):
        raw=self.put(f'l::Op::Observe(l::Observation::{selector})' if guard is None else f'l::Op::ObserveWhen({guard[0]}, l::Observation::{selector}, {default})','raw')
        if kind=='raw': return raw
        if kind=='bool': return (raw[0],'bool')
        return self.numeric(raw)
    def compile(self,a):
        a=self.p.expand(a); op=a[0]
        if op in ('int','bool'): return self.literal(a[1])
        if op=='name':
            name=a[1]; parts=name.split('.'); src=parts[0].capitalize()
            if self.law:
                selector=src+'Root' if len(parts)==2 else f'{src}({parts[2]})'
                return self.observe(selector)
            i=next(i for i,(n,_) in enumerate(self.p.inputs) if n==name)
            tid=self.p.inputs[i][1]; return self.op('Input',i,kind='bool' if self.p.kind(tid)=='Bool' else 'int')
        if op=='neg': return self.op('Sub',self.literal(0),self.numeric(self.compile(a[1])),kind='int')
        if op=='not': return self.op('Not',self.boolean(self.compile(a[1])))
        if op=='choose':
            cond=self.boolean(self.compile(a[1])); x=self.compile(a[2]); y=self.compile(a[3])
            if x[1]!=y[1]: x,y=self.numeric(x),self.numeric(y)
            return self.op('Select',cond,x,y,kind=x[1])
        x,y=self.compile(a[1]),self.compile(a[2])
        if op in ('&&','||','->'):
            x,y=self.boolean(x),self.boolean(y)
            if op=='&&': return self.op('And',x,y)
            if op=='->': x=self.op('Not',x)
            return self.op('Not',self.op('And',self.op('Not',x),self.op('Not',y)))
        x,y=self.numeric(x),self.numeric(y)
        if op in ('+','-','*'): return self.op({'+':'Add','-':'Sub','*':'Mul'}[op],x,y,kind='int')
        if op in ('div_floor','div_ceil'):
            assert self.law; return self.op('Div','l::Division::'+('Floor' if op=='div_floor' else 'Ceil'),x,y,kind='int')
        if op=='==': return self.op('Eq',x,y)
        if op=='!=': return self.op('Not',self.op('Eq',x,y))
        if op=='<': return self.op('Lt',x,y)
        if op=='>': return self.op('Lt',y,x)
        if op=='<=': return self.op('Not',self.op('Lt',y,x))
        if op=='>=': return self.op('Not',self.op('Lt',x,y))
        raise ValueError(a)
    def all(self,nodes):
        n=self.literal(True)
        for item in nodes: n=self.op('And',n,self.boolean(item))
        return n


def source(project):
    p=project; policy=p.policy; graph=Graph(p); outputs=[]; output_types=[]; output_domains=[]
    def output(ast,tid=None):
        key=(ast,tid)
        if key in outputs: return outputs.index(key)
        outputs.append(key)
        if tid is None:
            output_types.append(f'InputLeaf::I128 {{ min: 0, max: {len(policy["cases"])-1} }}')
            output_domains.append(f'ScalarDomain::Int {{ min: 0, max: {len(policy["cases"])-1} }}')
        else:
            output_types.append(p.input_leaf(tid,wide=p.kind(tid)=='I128'))
            output_domains.append(p.scalar_domain(tid,wide=p.kind(tid)=='I128'))
        return len(outputs)-1
    selection=('int',len(policy['cases'])-1)
    for i in range(len(policy['cases'])-2,-1,-1): selection=('choose',parse(policy['cases'][i]['when']),('int',i),selection)
    output(p.expand(selection))
    def expression(value,tid):
        a=p.expand(parse(value))
        if a[0] in ('int','bool'): return f'c::Expr::Constant({p.atom(a[1],tid)})'
        if a[0]=='name':
            parts=a[1].split('.'); src={'pre':'State','command':'Command','context':'Context'}[parts[0]]
            return f'c::Expr::Root(c::Source::{src})' if len(parts)==2 else f'c::Expr::Input(c::Source::{src}, {parts[2]})'
        return f'c::Expr::Output({output(a,tid)})'
    fields={i:tid for i,_,tid in p.types[100]['fields']}
    branches=[]
    for i,case in enumerate(policy['cases']):
        committing=case['class']!='Reject'
        assert (set(map(int,case['post']))==set(fields)) if committing else not case['post']
        assignments=[f'c::Assignment {{ field: {fid}, value: {expression(case["post"][str(fid)],tid)}, domain: {p.domain(tid)} }}' for fid,tid in fields.items()] if committing else []
        deliveries=[]
        for d in case['outbox']:
            _,dest,pay=next(ch for ch in p.channels if ch[0]==d['channel'])
            assert set(map(int,d['payload']))=={f for f,_,_ in p.types[pay]['fields']}
            payload=[f'c::PayloadField {{ field: {fid}, value: {expression(d["payload"][str(fid)],tid)} }}' for fid,_,tid in p.types[pay]['fields']]
            deliveries.append(f'c::DeliveryPlan {{ ordinal: {d["ordinal"]}, channel: {d["channel"]}, when: c::Expr::Constant(c::Atom::Bool(true)), destination: c::Expr::Constant({p.atom(d["destination"],dest)}), payload: {array(payload)}, idempotency: c::Expr::Constant(c::Atom::U128({d["idempotency_ordinal"]})) }}')
        reason='None' if case['reason'] is None else f'Some({case["reason"]})'
        branches.append(f'c::Branch {{ code: {i}, class: c::Class::{case["class"]}, reason: {reason}, assignments: {array(assignments)}, effects: &[], outbox: {array(deliveries)} }}')
    roots=[graph.compile(ast)[0] for ast,_ in outputs]
    assert len(outputs)<=16 and len(graph.nodes)<=256 and len(p.inputs)<=32
    assert parse(policy['cases'][-1]['when']) == ('bool', True)
    laws=[]
    for ident,scope,ast in p.laws:
        g=Graph(p,True); root=g.boolean(g.compile(ast)); kind=policy['law_kinds'][str(ident)]
        sc={'commit':'Committing','accept':'Accept','failure':'CommittedFailure','reject':'Reject'}[scope.split(',')[0].strip()]
        laws.append((ident,kind,sc,'genesis' in scope,g,root))
    if not any(x[1]=='CommittedFailureEffects' for x in laws):
        g=Graph(p,True); laws.append((policy.get('framework_failure_law',908),'CommittedFailureEffects','CommittedFailure',False,g,g.literal(False)))
    if not any(x[1]=='RejectNoAuthority' for x in laws):
        g=Graph(p,True); root=g.all([g.op('Eq',g.observe(k),g.literal(0)) for k in ('PostLength','PatchLength','EffectLength','OutboxLength')]); laws.append((policy.get('framework_reject_law',909),'RejectNoAuthority','Reject',False,g,root))
    else:
        # Original durable law 503 says true; retain it and add the structural protection.
        g=Graph(p,True); root=g.all([g.op('Eq',g.observe(k),g.literal(0)) for k in ('PostLength','PatchLength','EffectLength','OutboxLength')]); laws.append((909,'AuthoritySubjectRecipient','Reject',False,g,root))
    g=Graph(p,True); checks=[]
    for fid,tid in fields.items():
        value=policy['genesis'][str(fid)]
        checks.append(g.op('Eq',g.observe(f'Initial({fid})',kind='raw'),g.put('l::Op::Literal('+p.atom(value,tid,'l')+')','raw')))
    laws.append((990,'InitialCondition','Always',True,g,g.all(checks)))
    # Complete declaration conformance: recompute the chosen case from raw inputs,
    # never from candidate outputs. Conditional observations retain missing-value errors.
    # Original .zeno predicates above remain independent acceptance conditions.
    g=Graph(p,True); tests=[]; remaining=g.literal(True)
    def eq_observation(selector,expected,tid,guard):
        default=p.atom(policy['genesis'].get(str(selector.split('(')[-1].rstrip(')')),False if p.kind(tid)=='Bool' else p.types[tid]['variants'][0][0] if p.kind(tid)=='Sum' else 0),tid,'l')
        actual=g.observe(selector,'raw',guard,default)
        ast=p.expand(parse(expected))
        if ast[0] in ('int','bool'):
            wanted=g.put('l::Op::Literal('+p.atom(ast[1],tid,'l')+')','raw')
            equal=g.op('Eq',actual,wanted)
        else:
            equal=g.op('Eq',g.numeric(actual),g.numeric(g.compile(ast)))
        return g.op('Select',guard,equal,g.literal(True))
    for case in policy['cases']:
        guard=g.op('And',remaining,g.boolean(g.compile(parse(case['when']))))
        remaining=g.op('And',remaining,g.op('Not',g.boolean(g.compile(parse(case['when'])))))
        checks=[g.op('Eq',g.observe('Class'),g.literal({'Accept':0,'Reject':1,'CommittedFailure':2}[case['class']]))]
        has=g.observe('HasReason',kind='bool')
        checks.append(has if case['reason'] is not None else g.op('Not',has))
        if case['reason'] is not None:
            checks.append(g.op('Eq',g.observe('Reason',guard=guard),g.literal(case['reason'])))
        checks.append(g.op('Eq',g.observe('EffectLength'),g.literal(0)))
        checks.append(g.op('Eq',g.observe('OutboxLength'),g.literal(len(case['outbox']))))
        checks.append(g.op('Eq',g.observe('PostLength'),g.literal(len(case['post']))))
        for fid,tid in fields.items():
            if str(fid) in case['post']: checks.append(eq_observation(f'Post({fid})',case['post'][str(fid)],tid,guard))
        for j,d in enumerate(case['outbox']):
            checks.extend([g.op('Eq',g.observe(f'OutboxOrdinal({j})',guard=guard),g.literal(d['ordinal'])),g.op('Eq',g.observe(f'OutboxChannel({j})',guard=guard),g.literal(d['channel']))])
            for selector,atom in [(f'OutboxDestination({j})','l::Atom::Text('+rust_bytes(d['destination'])+')'),(f'OutboxIdempotency({j})',f'l::Atom::U128({d["idempotency_ordinal"]})')]:
                checks.append(g.op('Eq',g.observe(selector,'raw',guard,atom),g.put('l::Op::Literal('+atom+')','raw')))
            pay=next(ch[2] for ch in p.channels if ch[0]==d['channel'])
            for fid,_,tid in p.types[pay]['fields']: checks.append(eq_observation(f'OutboxPayload({j}, {fid})',d['payload'][str(fid)],tid,guard))
        tests.append(g.op('Select',guard,g.all(checks),g.literal(True)))
    tests.append(g.op('Not',remaining))
    laws.append((991,'DecisionConformance','Always',False,g,g.all(tests)))
    definitions=[]
    for tid,t in sorted(p.types.items()):
        if t['fields']:
            kind='s::Kind::Record('+array([f's::Field {{ id: {fid}, name: {rust_bytes(name)}, type_id: {fty} }}' for fid,name,fty in t['fields']])+')'
        elif t['variants']:
            kind='s::Kind::Sum('+array([f's::Variant {{ id: {i}, name: {rust_bytes(n)} }}' for i,n in t['variants']])+')'
        elif t['leaf'][0]=='Bool': kind='s::Kind::Bool'
        else: kind=f's::Kind::{t["leaf"][0]} {{ min: {t["leaf"][1]}, max: {t["leaf"][2]} }}'
        definitions.append(f's::Definition {{ id: {tid}, name: {rust_bytes(t["name"])}, kind: {kind} }}')
    schemas=[]; members=[]; init=[]
    for name,tid in [('state',100),('command',101),('context',102)]:
        fs=p.types[tid]['fields']
        members.append(f'{name}: '+('Vec<InputField>' if fs else 'InputLeaf'))
        init.append(f'{name}: '+('vec!['+', '.join(f'InputField {{ id: {fid}, leaf: {p.input_leaf(fty)} }}' for fid,_,fty in fs)+']' if fs else p.input_leaf(tid)))
        schemas.append(f'{name}: c::Schema::{"Record" if fs else "Leaf"}(&self.{name})')
    bindings=[]
    for name,_ in p.inputs:
        a=name.split('.'); src={'pre':'State','command':'Command','context':'Context'}[a[0]]
        bindings.append(f'c::Binding {{ source: c::Source::{src}, selector: c::Selector::'+('Root' if len(a)==2 else f'Field({a[2]})')+' }')
    channels=[]
    for cid,dest,pay in p.channels:
        payload=[f'c::TypedField {{ field: {fid}, domain: {p.domain(tid)} }}' for fid,_,tid in p.types[pay]['fields']]
        channels.append(f'c::Channel {{ id: {cid}, destination: {p.domain(dest)}, payload: {array(payload)}, idempotency: c::Domain::U128 {{ min: 0, max: 0 }} }}')
    reasons=[]
    for ident in p.reasons:
        classes={c['class'] for c in policy['cases'] if c['reason']==ident}; assert len(classes)==1
        reasons.append(f'c::Reason {{ id: {ident}, class: c::Class::{classes.pop()} }}')
    meta=json.loads((p.base/'v2/schema-origin.json').read_text())
    hash_array=str(list(bytes.fromhex(meta['schema_commitment'])))
    def value_size(tid):
        t=p.types[tid]
        if t['fields']: return 5+sum(2+value_size(fty) for _,_,fty in t['fields'])
        return {'Bool':1,'I128':17,'Sum':8}[p.kind(tid)]
    sizes={name:48+value_size(tid) for name,tid in [('state',100),('command',101),('context',102)]}
    # Conservative finite budgets follow directly from graph sizes and schema widths.
    step_bound=len(graph.nodes)+sum(len(g.nodes) for _,_,_,_,g,_ in laws)+5
    read_bound=len(p.inputs)*3+sum(sum('Observe' in n for n in g.nodes) for _,_,_,_,g,_ in laws)+64
    code='''// Generated declarative data. Review v2/policy.json and project.zeno.
// Regenerate/check with tools/check_template_contracts_v2.py; no runtime mapper.
extern crate alloc;
use alloc::{vec, vec::Vec};
use zeno_fcis_synthesis::finite::{
    V2InputField as InputField, V2InputLeaf as InputLeaf, V2InputVariant as InputVariant,
    v2_composition as c, v2_laws as l, v2_catalog as catalog, v2_authority as authority,
    canonical_v2::schema as s, V2Resource as Resource, v2_zero_limits,
};
'''
    if not any(t['variants'] for t in p.types.values()): code=code.replace(', V2InputVariant as InputVariant', '')
    code+='use zeno_fcis_synthesis::finite::{Domain as ScalarDomain, Op, V2ScalarProgram};\n'
    code+='/// Exact actual original schema bytes.\npub const ORIGINAL_SCHEMA: &[u8] = include_bytes!("../v2/schema.zcve");\n/// Complete reviewed library-encoded policy.\npub const ORIGINAL_POLICY: &[u8] = include_bytes!("../v2/policy.zcve");\n'
    code+=f'/// Complete original named schema description.\npub const DESCRIPTION: s::Description<\'static> = s::Description {{ profile: {rust_bytes(p.profile)}, version: 1, root: 100, definitions: {array(definitions)} }};\n'
    code+='/// Original root and schema commitments and complete wire-size limits.\npub const FRAMING: c::Framing = c::Framing { '+', '.join(f'{name}: c::FrameBinding {{ root: {tid}, schema: {hash_array}, max_bytes: {sizes[name]} }}' for name,tid in [('state',100),('command',101),('context',102)])+' };\n'
    code+='/// Exact channel to original destination and payload type links.\npub const CHANNEL_ROOTS: &[(u32,u32,u32)] = &'+str(p.channels)+';\n'
    code+='/// Complete ordered decisions selected only by actual graph output.\npub const BRANCHES: &[c::Branch<\'static>] = '+array(branches)+';\n'
    for ident,kind,scope,genesis,g,root in laws: code+=f'const LAW_{ident}: &[l::Op<\'static>] = '+array(g.nodes)+';\n'
    code+='/// Original scoped laws plus independent structural requirements.\npub const LAWS: &[l::Law<\'static>] = '+array([f'l::Law {{ id: {i}, kind: l::Kind::{kind}, scope: l::Scope::{scope}, genesis: {str(genesis).lower()}, program: l::Program {{ nodes: LAW_{i}, root: {root[0]} }} }}' for i,kind,scope,genesis,g,root in laws])+';\n'
    code+='/// No declared law is optional at descriptor admission.\npub const REQUIRED: &[u32] = &'+str([i for i,_,_,_,_,_ in laws])+';\n'
    code+='/// Owns declarative input/output type tables; evaluation stays in the library.\npub struct Contract { '+', '.join(members)+', output_types: Vec<InputLeaf> }\n'
    code+='impl Default for Contract { fn default() -> Self { Self::new() } }\nimpl Contract {\n /// Allocate only fixed declarative type tables.\n #[must_use]\n pub fn new() -> Self { Self { '+', '.join(init)+', output_types: vec!['+', '.join(output_types)+'] } }\n'
    code+=' /// Borrow the full raw-input, complete-decision and law contract.\n #[must_use]\n pub fn descriptor(&self) -> c::Descriptor<\'_> { c::Descriptor {\n '+', '.join(schemas)+',\n'
    prog='V2ScalarProgram { inputs: '+array([p.scalar_domain(tid) for _,tid in p.inputs])+', outputs: '+array(output_domains)+', nodes: '+array(graph.nodes)+', roots: &'+str(roots)+' }'
    code+=' program: '+prog+', bindings: '+array(bindings)+', output_types: &self.output_types, decision_output: 0, branches: BRANCHES, reasons: '+array(reasons)+', channels: '+array(channels)+', laws: LAWS, required: REQUIRED,\n'
    code+=f' limits: v2_zero_limits().with_limit(Resource::Read, {read_bound}).with_limit(Resource::Write, {len(fields)}).with_limit(Resource::Candidate, 1).with_limit(Resource::Effect, 1).with_limit(Resource::Byte, {sum(sizes.values())}).with_limit(Resource::WitnessByte, 0).with_limit(Resource::Depth, 0).with_limit(Resource::Step, {step_bound}),\n }} }}\n}}\n'
    code+='/// Exact complete schema/policy correspondence precedes Authority construction.\npub fn checked_catalog<\'a>(descriptor: &\'a c::Descriptor<\'a>) -> Result<catalog::BoundCatalog<\'a>, catalog::Failure> {\n catalog::bind_original(ORIGINAL_SCHEMA, &DESCRIPTION, catalog::Limits { schema: s::Limits { bytes: ORIGINAL_SCHEMA.len() as u64, types: '+str(len(p.types))+', fields: '+str(max(len(t['fields']) for t in p.types.values()))+', variants: '+str(max(len(t['variants']) for t in p.types.values()))+' }, contract_bytes: ORIGINAL_POLICY.len() as u64 }, ORIGINAL_POLICY, descriptor, &FRAMING, CHANNEL_ROOTS)\n}\n'
    code+='/// Ordered refusal at checked catalog or private Authority construction.\n#[derive(Debug)]\n#[non_exhaustive]\npub enum BindFailure {\n /// Complete original schema/policy correspondence refused.\n Catalog(catalog::Failure),\n /// Private library source binding refused.\n Authority(authority::Refusal)\n }\n/// Sole supplied production constructor; identity comes from the checked library.\npub fn checked_authority<\'a>(descriptor: &\'a c::Descriptor<\'a>) -> Result<authority::Authority<\'a>, BindFailure> {\n let catalog = checked_catalog(descriptor).map_err(BindFailure::Catalog)?;\n authority::bind(&catalog).map_err(BindFailure::Authority)\n}\n'
    formatted=subprocess.run(['rustfmt','+1.97.1','--edition','2024','--emit','stdout'],input=code,text=True,capture_output=True,check=True).stdout
    return formatted, {'nodes':len(graph.nodes),'outputs':len(outputs),'law_nodes':sum(len(g.nodes) for _,_,_,_,g,_ in laws),'law_ids':[i for i,_,_,_,_,_ in laws],'schema_definitions':len(p.types),'budgets':{'read':read_bound,'step':step_bound,'byte':sum(sizes.values())}}


def main():
    parser=argparse.ArgumentParser(description=__doc__); parser.add_argument('--write',action='store_true'); parser.add_argument('--source-only',action='store_true',help='Check generated declarations only; does not qualify policy bytes'); args=parser.parse_args()
    report={}
    for name in TEMPLATES:
        p=Project(name); text,info=source(p); target=p.base/'src/v2_contract.rs'
        if args.write: target.write_text(text)
        elif not target.exists() or target.read_text()!=text: raise SystemExit(f'Stale declarative source: {target}')
        info['source_sha256']=hashlib.sha256(text.encode()).hexdigest()
        original=(p.base/'v2/schema.zcve').read_bytes()
        origin=json.loads((p.base/'v2/schema-origin.json').read_text())
        assert hashlib.sha256(original).hexdigest()==origin['sha256'] and len(original)==origin['bytes']
        encoded=(p.base/'v2/policy.zcve').read_bytes()
        if not args.write and not args.source_only and not encoded:
            raise SystemExit(f'Missing library-encoded complete policy: {name}; source matching alone is not acceptance')
        info['policy_bytes']=len(encoded)
        info['policy_sha256']=hashlib.sha256(encoded).hexdigest() if encoded else None
        report[name]=info
    print(json.dumps({'status':'generated' if args.write else 'declarations_match','source_only':args.source_only or args.write,'policy_qualification':'Requires native checked-catalog and independent decision tests','templates':report},sort_keys=True,indent=2))
if __name__=='__main__': main()
