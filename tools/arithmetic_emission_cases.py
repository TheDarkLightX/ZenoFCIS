"""Shipped-program discovery and concrete Verus translation checks."""
import json
from pathlib import Path
import re
def op_text(node):
    name,*a=node
    names={'input':'Input','int':'Int','bool':'Bool','add':'Add','sub':'Sub','mul':'Mul',
           'div_floor':'DivFloor','div_ceil':'DivCeil','mul_div_floor':'MulDivFloor',
           'mul_div_ceil':'MulDivCeil','signed_mul_div_floor':'SignedMulDivFloor',
           'eq':'Eq','lt':'Lt','and':'And','not':'Not','select':'Select'}
    if name=='products_ge':
        return 'Op::ProductsGe(['+','.join(map(str,a[:3]))+'],['+','.join(map(str,a[3:]))+'])'
    if name not in names:raise ValueError('uncovered emitted operation '+name)
    if name=='int':return 'Op::Int('+str(int(a[0]))+'i128)'
    def literal(v):return str(v).lower() if isinstance(v,bool) else str(v)
    return 'Op::'+names[name]+'('+','.join(map(literal,a))+')'


def domain_text(d):
    return 'Domain::Bool' if d['boolean'] else 'Domain::Int{min:'+str(int(d['min']))+'i128,max:'+str(int(d['max']))+'i128}'




OPERATIONS=('input','int','bool','add','sub','mul','div_floor','div_ceil',
    'mul_div_floor','mul_div_ceil','signed_mul_div_floor','products_ge',
    'eq','lt','and','not','select','eager_select')


def discover(root):
    programs=[];problems=[]
    for base in ('examples','crates/zeno-fcis-cli/templates'):
        for p in sorted((root/base).rglob('*.json')):
            try:value=json.loads(p.read_text())
            except ValueError:continue
            if isinstance(value,dict) and value.get('schema')=='zeno-fcis/synthesis-problem/1':
                problems.append(str(p.relative_to(root)))
        for p in sorted((root/base).rglob('transition.rs')):
            if 'pub fn transition(input: &[i128])' in p.read_text():
                program=p.with_name('program.zcve')
                if not program.is_file():raise ValueError('shipped emitted transition has no original Program')
                programs.append(str(program.relative_to(root)))
    return dict(programs=sorted(programs),problems=sorted(problems),operations=list(OPERATIONS))


def sequence(items):return 'seq!['+','.join(items)+']'


def fields(program):
    return [sequence(map(domain_text,program['inputs'])),sequence(map(domain_text,program['outputs'])),
            sequence(map(op_text,program['nodes'])),sequence(str(r)+'u16' for r in program['roots'])]


def checks_source(root,cases):
    source=(root/'verification/verus/arithmetic_emission_proof.rs').read_text()
    source=re.sub(r'#\[path = "(\.\./\.\./crates/[^"]+)"\]',
        lambda m:'#[path = '+json.dumps(str((root/'verification/verus'/m[1]).resolve()))+']',source)
    for module in ('arithmetic_spec','arithmetic_typed_spec','arithmetic_emitted_spec','arithmetic_reference64'):
        source=source.replace('mod '+module+';',
            '#[path = '+json.dumps(str(root/'verification/verus'/f'{module}.rs'))+']\nmod '+module+';')
    source+='''\n#[cfg(verus_keep_ghost)]
mod checked_programs {
use crate::evaluation::{Domain,Op};
use vstd::prelude::*;
verus! {
'''
    for name,original,decoded in cases:
        if not re.fullmatch(r'[a-z][a-z0-9_]*',name):raise ValueError('invalid check name')
        a=','.join(fields(decoded));b=','.join(fields(original))
        source+=f'''pub(crate) proof fn check_{name}(input:Seq<i128>)
    ensures crate::arithmetic_emitted_spec::run({a},input)==
        crate::evaluation::spec::execution({b},input),
{{
    crate::arithmetic_emitted_spec::emitted_program_correspondence({b},input);
}}
'''
    source+='}\n}\n'
    return source
