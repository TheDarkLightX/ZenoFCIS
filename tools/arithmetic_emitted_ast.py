"""Reviewed Rust-subset decoder for emitted arithmetic translation validation.

This checker is trusted, not a Verus theorem. It admits only the complete eager
grammar below and byte-identical, separately verified scalar/wide helper bodies.
Boolean provenance is an IR annotation: Bool and Int(0/1) spell identically in
Rust; their numeric literal and range meanings are checked before annotation.
"""
import re
from pathlib import Path


class BindingMismatch(AssertionError):
    def __init__(self,obligation,message):
        self.obligation=obligation
        super().__init__(obligation+': '+message)


def require(ok,obligation,message):
    if not ok:raise BindingMismatch(obligation,message)


def helper_suffix(root):
    p=Path(root)/'crates/zeno-fcis-synthesis/src/finite/value'
    return ('\n#[allow(dead_code)]\nmod arithmetic {\n    pub(crate) mod wide {\n'+
        (p/'wide.rs').read_bytes().decode('utf-8')+'\n}\n    pub(crate) mod scalar {\n'+
        (p/'scalar.rs').read_bytes().decode('utf-8')+'\n}\n}\n')


def expression(text):
    m=re.fullmatch(r'input\[([0-9]+)\]',text)
    if m:return ['input',int(m[1])]
    m=re.fullmatch(r'(-?(?:0|[1-9][0-9]*))_i128',text)
    if m:return ['int',str(int(m[1]))]
    m=re.fullmatch(r'v([0-9]+)\.checked_(add|sub)\(v([0-9]+)\)\?',text)
    if m:return [m[2],int(m[1]),int(m[3])]
    m=re.fullmatch(r'arithmetic::scalar::multiply\(v([0-9]+),v([0-9]+)\)\?',text)
    if m:return ['mul',int(m[1]),int(m[2])]
    m=re.fullmatch(r'arithmetic::scalar::divide\((false|true),v([0-9]+),v([0-9]+)\)\?',text)
    if m:return ['div_ceil' if m[1]=='true' else 'div_floor',int(m[2]),int(m[3])]
    m=re.fullmatch(r'arithmetic::scalar::mul_div\((false|true),v([0-9]+),v([0-9]+),v([0-9]+)\)\?',text)
    if m:return ['mul_div_ceil' if m[1]=='true' else 'mul_div_floor',*map(int,m.groups()[1:])]
    m=re.fullmatch(r'arithmetic::scalar::signed_mul_div_floor\(v([0-9]+),v([0-9]+),v([0-9]+)\)\?',text)
    if m:return ['signed_mul_div_floor',*map(int,m.groups())]
    m=re.fullmatch(r'i128::from\(arithmetic::scalar::products_ge\(v([0-9]+),v([0-9]+),v([0-9]+),v([0-9]+),v([0-9]+),v([0-9]+)\)\?\)',text)
    if m:return ['products_ge',*map(int,m.groups())]
    m=re.fullmatch(r'i128::from\(v([0-9]+) (==|<) v([0-9]+)\)',text)
    if m:return ['eq' if m[2]=='==' else 'lt',int(m[1]),int(m[3])]
    m=re.fullmatch(r'i128::from\(v([0-9]+) == 1 && v([0-9]+) == 1\)',text)
    if m:return ['and',*map(int,m.groups())]
    m=re.fullmatch(r'i128::from\(v([0-9]+) == 0\)',text)
    if m:return ['not',int(m[1])]
    m=re.fullmatch(r'if v([0-9]+) == 1 \{ v([0-9]+) \} else \{ v([0-9]+) \}',text)
    if m:return ['select',*map(int,m.groups())]
    if text.startswith('if '):
        raise BindingMismatch('E1_eager_evaluation','selection must read already evaluated node values')
    if 'unwrap_or' in text or ('arithmetic::' in text and not text.endswith('?')):
        raise BindingMismatch('E1_partial_refusal_propagation','partial expression does not propagate refusal')
    raise BindingMismatch('E1_closed_expression_grammar','unrecognized executable expression')


def decode(source,program,root):
    suffix=helper_suffix(root);helpers=source.endswith(suffix)
    if helpers:source=source[:-len(suffix)]
    require('mod arithmetic' not in source,'E2_helper_identity','helper block diverges from verified bytes')
    require(not re.search(r'\bcfg(?:_attr)?\s*[!(]',source),'E2_native_configuration','native-only configuration in emitted transition')
    require(source.isascii(),'E1_closed_expression_grammar','non-ASCII executable text is outside the closed grammar')
    lines=source.split('\n');at=0
    require(lines[-1]=='','E1_complete_program','emitted program must end with LF')
    lines=lines[:-1]
    def take():
        nonlocal at
        require(at<len(lines),'E1_complete_program','unexpected end of emitted source')
        v=lines[at];at+=1;return v
    require(re.fullmatch(r'// Generated from zeno-fcis/finite-i128/[1-4]\. Pure code; no commit authority\.',take()),'E1_header','unexpected module header')
    require(take()=='/// Evaluates the generated finite program; no commit authority.','E1_header','unexpected ABI documentation')
    require(take()=='#[rustfmt::skip]','E1_header','unexpected attribute')
    m=re.fullmatch(r'pub fn transition\(input: &\[i128\]\) -> Option<\[i128; ([0-9]+)\]> \{',take())
    require(m is not None,'E1_abi','unexpected transition signature');noutputs=int(m[1])
    m=re.fullmatch(r'    if input.len\(\) != ([0-9]+) \{ return None; \}',take())
    require(m is not None,'E1_input_admission','missing exact input arity check');ninputs=int(m[1])
    require(ninputs==len(program['inputs']) and noutputs==len(program['outputs']),'E1_abi','arity differs from original Program')
    def guard(line,kind,index=None):
        target=r'input\[([0-9]+)\]' if kind=='input' else r'v([0-9]+)'
        m=re.fullmatch(r'    if !\((-?[0-9]+)_i128\.\.=(-?[0-9]+)_i128\)\.contains\(&'+target+r'\) \{ return None; \}',line)
        require(m is not None,'E1_'+kind+'_admission','missing inclusive domain guard')
        if index is not None:require(int(m[3])==index,'E1_input_order','input field order differs')
        return str(int(m[1])),str(int(m[2])),int(m[3])
    inputs=[]
    for i,d in enumerate(program['inputs']):
        lo,hi,_=guard(take(),'input',i)
        require((lo,hi)==(str(d['min']),str(d['max'])),'E1_input_admission','input bounds differ')
        require(not d['boolean'] or (lo,hi)==('0','1'),'E1_input_admission','invalid Boolean range annotation')
        inputs.append(dict(min=lo,max=hi,boolean=d['boolean']))
    nodes=[]
    while at<len(lines) and lines[at].startswith('    let v'):
        m=re.fullmatch(r'    let v([0-9]+): i128 = (.*);',take())
        require(m is not None and int(m[1])==len(nodes),'E1_eager_node_order','missing or reordered eager node')
        node=expression(m[2]);i=len(nodes)
        require(take()==f'    let _ = v{i};','E1_eager_node_order','unexpected instruction suffix')
        if node[0]=='input':require(node[1]<ninputs,'E1_references','input reference outside ABI')
        elif node[0] not in ('int','bool'):
            require(all(j<i for j in node[1:]),'E1_references','reference does not name an earlier node')
        if i<len(program['nodes']) and program['nodes'][i][0]=='bool':
            expected=program['nodes'][i][1]
            require(node==['int',str(int(expected))],'E1_boolean_literal','Boolean spelling differs')
            node=['bool',expected]
        nodes.append(node)
    roots=[];outputs=[]
    for d in program['outputs']:
        lo,hi,r=guard(take(),'output');roots.append(r)
        require((lo,hi)==(str(d['min']),str(d['max'])),'E1_output_admission','output bounds differ')
        require(not d['boolean'] or (lo,hi)==('0','1'),'E1_output_admission','invalid Boolean range annotation')
        require(r<len(nodes),'E1_projection','root outside evaluated nodes')
        outputs.append(dict(min=lo,max=hi,boolean=d['boolean']))
    require(take()=='    Some(['+''.join(f'v{r},' for r in roots)+'])','E1_projection','output tuple differs from checked root order')
    require(take()=='}' and at==len(lines),'E1_complete_program','unreviewed executable text outside grammar')
    partial={'mul','div_floor','div_ceil','mul_div_floor','mul_div_ceil','signed_mul_div_floor','products_ge'}
    require(helpers or not any(n[0] in partial for n in nodes),'E2_helper_identity','arithmetic helper calls without bound helper source')
    decoded=dict(inputs=inputs,outputs=outputs,nodes=nodes,roots=roots)
    require(decoded==program,'E1_ast_program_correspondence','decoded emitted AST differs from the original Program')
    return decoded,helpers
