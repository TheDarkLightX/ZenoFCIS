"""Inert temporal-v1 lowering. No evaluation, admission or proof authority."""
from dataclasses import dataclass
import hashlib
import json
import re
from pathlib import Path

import ladder as l

# Import-time acquisition; decision calls use this immutable source identity.
IMPLEMENTATION_ID=hashlib.sha256(b''.join(Path(__file__).with_name(name).read_bytes() for name in ('ladder.py','ladder_en.json','temporal_lowering.py'))).hexdigest()

# Pinned to spec ProjectLimits and formal-tools ExportLimits. These are
# conservative producer checks; native admission/export still check independently.
NATIVE_DEPTH=256
NATIVE_NODES=4096
NATIVE_OPERATIONS=1000000
NATIVE_EXPORT_BYTES=16*1024*1024


@dataclass(frozen=True)
class _RenderSize:
    term: int
    defined: int
    bytes: int
    operations: int


@dataclass(frozen=True)
class _NativeExpression:
    text: str
    nodes: int
    depth: int
    operations: int
    sizes: tuple[_RenderSize, ...]


def _render_size(text,tag,parts,remaining):
    """Bound serialization work only; never compute a predicate's truth.

    Sizes are indexed by remaining word length. This retains decreasing suffix
    lengths in nested temporal folds instead of multiplying by the horizon at
    every level. Each recurrence includes native term and definedness copies.
    """
    sizes=[p.sizes[remaining] for p in parts]
    if tag in ('always','eventually','until'):
        term,defined,used,operations=5,4,9,1
        suffixes=range(1,remaining+1) if tag=='until' else range(remaining,0,-1)
        for suffix in suffixes:
            children=[p.sizes[suffix] for p in parts]
            used+=sum(c.bytes for c in children)
            operations+=1+sum(c.operations for c in children)
            if tag=='until':
                left,right=children
                defined=32+right.defined+left.defined+right.term+left.term+defined
                term=16+right.term+left.term+term
            else:
                child=children[0]
                defined=24+defined+term+child.defined
                term=8+term+child.term
            used+=term+defined
        return _RenderSize(term,defined,used+term+defined,operations)
    if tag=='next':
        if remaining<=1:return _RenderSize(5,4,9,1)
        child=parts[0].sizes[remaining-1]
        return _RenderSize(child.term,child.defined,child.bytes+child.term+child.defined,child.operations+1)
    terms=sum(c.term for c in sizes);definitions=sum(c.defined for c in sizes)
    used=sum(c.bytes for c in sizes);operations=sum(c.operations for c in sizes)
    if not parts:
        # Projection names add a time suffix; signed SMT literals add brackets.
        term=5 if tag=='bool' else len(text)+10
        defined=4
    elif tag in ('+','-','*'):
        term=8+terms;defined=128+definitions+2*term
    elif tag in ('floor','ceil'):
        term=64+4*sizes[0].term+6*sizes[1].term
        defined=200+definitions+sizes[1].term+2*term
    elif tag=='atom':
        term=terms;defined=definitions
    elif tag=='not':
        term=8+terms;defined=definitions
    else:
        term=8+terms;defined=8+definitions
    copies=2 if tag in ('+','-','*','and','or') else 1
    return _RenderSize(term,defined,used+copies*(term+defined),operations+copies)


def _native(text,tag,parts=(),width=1):
    """Count the emitted native tree, including scalar/atom wrapper nodes."""
    nodes=1+sum(p.nodes for p in parts)
    depth=1+max((p.depth for p in parts),default=0)
    operations=1+sum(p.operations for p in parts)*(width if tag in ('always','eventually','until') else 1)
    if depth>NATIVE_DEPTH:raise l.Refusal('lowering:native-depth-budget')
    if nodes>NATIVE_NODES:raise l.Refusal('lowering:native-shape-budget')
    if width*operations>NATIVE_OPERATIONS:
        raise l.Refusal('lowering:native-export-operations-budget')
    sizes=tuple(_render_size(text,tag,parts,remaining) for remaining in range(width+1))
    return _NativeExpression(text,nodes,depth,operations,sizes)


def _check_export_budget(formula,width,field_count):
    # Every finite length is exported. Length selection copies its accumulated
    # term/definedness at each step, then the final source includes that pair.
    used=sum(size.bytes for size in formula.sizes[1:])
    operations=sum(size.operations for size in formula.sizes[1:])
    term,defined=formula.sizes[width].term,formula.sizes[width].defined
    for size in reversed(formula.sizes[1:width]):
        term=32+size.term+term;defined=32+size.defined+defined
        used+=term+defined;operations+=1
    if operations>NATIVE_OPERATIONS:
        raise l.Refusal('lowering:native-export-operations-budget')
    # At most pre and post projections for each field, with i128 range guards.
    if used+term+defined+1024+width*field_count*512>NATIVE_EXPORT_BYTES:
        raise l.Refusal('lowering:native-export-bytes-budget')


@dataclass(frozen=True)
class TemporalObligation:
    subject: str
    source: str | None
    obligations: tuple[str, ...]
    authority: str = 'none'


def lower(tree, context, constants=()):
    """Own all inputs; return text for the existing Rust parser, never evidence.

    A replay discharges only this word. Execution membership, initialization,
    continuity and model restrictions remain obligations. Trace-wide facts must
    be supplied once as explicit Boolean constants, never reread at later events.
    """
    tree=l.freeze(tree)
    l.validate(tree,context)
    if context.data['theory']!=l.TEMPORAL_THEORY:
        raise l.Refusal('lowering:unsupported-theory')
    constants=l.freeze(constants)
    if any(type(name) is not str or type(value) is not bool for name,value in constants):
        raise l.Refusal('lowering:constant-type')
    if len({name for name,_ in constants})!=len(constants):
        raise l.Refusal('lowering:duplicate-constant')
    constant_map=dict(constants)
    if set(constant_map)-{name for name,s in context.data['symbols'].items() if s['observation']=='trace'}:
        raise l.Refusal('lowering:unknown-constant')
    subject=hashlib.sha256(json.dumps([l.scope_identity(context),l.normal(tree,context),tree,sorted(constants),IMPLEMENTATION_ID],separators=(',',':')).encode()).hexdigest()
    pending=('execution initialization, transition membership and post/pre continuity',
             'exact composed-model, attacker and environment restrictions',
             'observation correspondence, event order and checked-i128 encoding')
    if context.data['trace']['mode']=='unbounded':
        pending+=('kernel-checked unbounded theorem with explicit model and fairness assumptions',
                  'event-order and progress-preserving concrete refinement')
    schema=context.data.get('native_schema',{})
    state=schema.get('state')
    fields=schema.get('fields',())
    if type(state) is not int or not 3<=state<900000001:
        raise l.Refusal('lowering:native-state-type')
    ids=[field.get('id') for field in fields]
    if any(type(value) is not int or not 3<=value<900000001 for value in ids) or state in ids or len(set(ids))!=len(ids):
        raise l.Refusal('lowering:native-field-identity')
    if any(field.get('kind') not in ('bool','int') for field in fields):
        raise l.Refusal('lowering:native-field-kind')
    declarations='type '+str(state)+' state Observations; type 900000001 bool Flag; type 900000002 int Scalar; '
    declarations+=' '.join('field '+str(field['id'])+' '+str(state)+' observation_'+str(field['id'])+' '+('900000001' if field['kind']=='bool' else '900000002')+';' for field in fields)
    field_kinds={field['id']:field['kind'] for field in fields}
    expanded=l.expand(tree,context)
    if expanded[0]=='exists':
        return TemporalObligation(subject,None,(*pending,'unsupported-lowering:existential-trace-witness-route'))
    work=0
    width=context.data['trace']['horizon'] if context.data['trace']['mode']=='finite' else 1
    def emit(text,tag,*parts):return _native(text,tag,parts,width)
    def binary(tag,a,b):
        return emit('('+a.text+' '+({'and':'&&','or':'||'}.get(tag,tag))+' '+b.text+')',tag,a,b)
    def literal(value):
        # The native parser represents negative literals, except i128::MIN,
        # as subtraction from zero. Count those otherwise hidden scalar nodes.
        if value<0 and value!=-(1<<127):
            return emit(str(value),'-',literal(0),literal(-value))
        return emit(str(value),'value')
    def spend():
        nonlocal work
        work+=1
        if work>4096:raise l.Refusal('UNKNOWN:lowering-budget')
    def term(n,env):
        spend();tag=n[0]
        if tag=='int':
            if type(n[1]) is not int or not -(1<<127)<=n[1]<(1<<127):
                raise l.Refusal('lowering:i128-literal')
            return literal(n[1])
        if tag=='var':
            value=env[n[1]]
            if type(value) is not int:raise l.Refusal('unsupported-lowering:object-arithmetic')
            return term(('int',value),env)
        if tag=='call':return emit(projection(n,env),'value')
        if tag in ('+','-','*'):
            return binary(tag,term(n[1],env),term(n[2],env))
        if tag in ('floor','ceil'):
            a,b=term(n[1],env),term(n[2],env)
            return emit('div_'+tag+'('+a.text+', '+b.text+')',tag,a,b)
        raise l.Refusal('unsupported-lowering:integer-constructor:'+tag)
    def arguments(n,env):
        values=[]
        for arg in n[3]:
            if arg[0]=='var':values.append(env[arg[1]])
            elif arg[0]=='int':values.append(arg[1])
            else:raise l.Refusal('unsupported-lowering:nonrigid-observation-argument')
        return tuple(values)
    def projection(n,env):
        meta=context.symbol(n[1]);args=arguments(n,env)
        if meta['observation']=='trace':
            raise l.Refusal('unsupported-lowering:trace-wide-integer-observation:'+n[1])
        rows=[r for r in meta.get('native',()) if tuple(r['args'])==args]
        if len(rows)!=1:raise l.Refusal('unsupported-lowering:observation-map:'+n[1])
        value=rows[0].get('after' if n[2] else 'before','')
        if not re.fullmatch(r'(pre|post)\.'+str(state)+r'\.[1-9][0-9]*',value):
            raise l.Refusal('lowering:projection-path')
        field=int(value.rsplit('.',1)[1])
        if field_kinds.get(field)!=('bool' if meta['kind']=='predicate' else 'int'):
            raise l.Refusal('lowering:observation-field-type')
        return value
    def form(n,env):
        spend();tag=n[0]
        if tag in ('forall','exists'):
            dom=context.domain(n[2])
            if dom.get('trace'):
                if n is not expanded:raise l.Refusal('lowering:trace-binder-location')
                return form(n[3],{**env,n[1]:'$trace'})
            values=dom.get('values')
            if values is None:raise l.Refusal('unsupported-lowering:unbounded-object-domain:'+n[2])
            if len(values)>256 or any(type(v) not in (int,str) for v in values) or len(set(values))!=len(values):
                raise l.Refusal('lowering:finite-object-domain')
            parts=[form(n[3],{**env,n[1]:value}) for value in values]
            if not parts:return emit('true' if tag=='forall' else 'false','atom',emit('true' if tag=='forall' else 'false','bool'))
            def balanced(start,end):
                if end-start==1:return parts[start]
                middle=(start+end)//2
                return binary('and' if tag=='forall' else 'or',balanced(start,middle),balanced(middle,end))
            return balanced(0,len(parts))
        if tag in ('always','eventually','next','not'):
            child=form(n[1],env)
            return emit(tag+' ('+child.text+')',tag,child)
        if tag in ('and','or','until'):
            return binary(tag,form(n[1],env),form(n[2],env))
        if tag=='=>':
            a,b=form(n[1],env),form(n[2],env)
            return binary('or',emit('not ('+a.text+')','not',a),b)
        if tag=='<=>':return form(('and',('=>',n[1],n[2]),('=>',n[2],n[1])),env)
        if tag=='call':
            meta=context.symbol(n[1])
            if meta['observation']=='trace':
                if context.data['trace']['mode']=='unbounded':
                    raise l.Refusal('unsupported-lowering:unbounded-run-assumption:'+n[1])
                if arguments(n,env)!=('$trace',) or n[1] not in constant_map:
                    raise l.Refusal('unsupported-lowering:trace-wide-assumption:'+n[1])
                value=str(constant_map[n[1]]).lower()
                return emit(value,'atom',emit(value,'bool'))
            path=projection(n,env)
            relation=emit(path+' == 1','compare',emit(path,'value'),literal(1))
            return emit('atom('+relation.text+')','atom',relation)
        if tag in l.COMPARE:
            # A leading relational '(' is parsed as a Boolean group by temporal v1.
            # Adding zero preserves checked-i128 definedness and exposes a scalar prefix.
            a,b=term(n[1],env),term(n[2],env)
            # Do not parenthesize the leading addition: it disambiguates the
            # existing relational grammar while preserving checked arithmetic.
            left=emit('0 + '+a.text,'+',literal(0),a)
            relation=emit(left.text+' '+('==' if tag=='=' else tag)+' '+b.text,'compare',left,b)
            return emit('atom('+relation.text+')','atom',relation)
        raise l.Refusal('unsupported-lowering:formula-constructor:'+tag)
    try:
        formula=form(expanded,{})
    except l.Refusal as exc:
        if str(exc).startswith('unsupported-lowering:'):
            return TemporalObligation(subject,None,(*pending,str(exc)))
        raise
    trace=context.data['trace']
    mode='cvc5 finite '+str(trace['horizon']) if trace['mode']=='finite' else 'lean unbounded'
    _check_export_budget(formula,width,len(fields))
    source='zeno 1; project 1 ladder_temporal; '+declarations+' claim 2 ladder_target '+mode+' = '+formula.text+';\n'
    if len(source)>65536:raise l.Refusal('UNKNOWN:lowering-source-budget')
    subject=hashlib.sha256(json.dumps([subject,source],separators=(',',':')).encode()).hexdigest()
    return TemporalObligation(subject,source,pending)
