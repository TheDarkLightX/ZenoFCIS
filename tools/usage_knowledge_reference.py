#!/usr/bin/env python3
"""Independent scoped-knowledge model and finite controls, not native qualification.

Applicability/checking/planning use explicit host relations, independently of
Datalog's derived advice views. Datalog supplies trace witnesses for comparison.
"""
from __future__ import annotations
from collections import deque
from dataclasses import dataclass
import hashlib
import json
from pathlib import Path
import re
import strategy_knowledge_reference as R

RULES = '''
isa(X,Z) :- isa(X,Y),isa(Y,Z).
usage_replaces(X,Z) :- supersedes(X,Z).
usage_replaces(X,Z) :- supersedes(X,Y),usage_replaces(Y,Z).
usage_known(K,V) :- task(K,V),V!=unknown.
usage_match(K,V) :- usage_known(K,V).
usage_match(K,any) :- usage_known(K,_).
usage_match(K,V) :- usage_known(K,S),isa(S,V),K!=version,K!=environment.
usage_missing(L,K,V) :- scope(L,K,V),not usage_match(K,V).
usage_topic(L,S) :- lesson(L,S).
usage_topic(L,S) :- lesson(L,C),isa(S,C).
usage_unknown_environment(L) :- usage_topic(L,S),usage_known(subject,S),blocker(L,_),not usage_known(environment,_).
usage_scope(L,S) :- usage_topic(L,S),usage_known(subject,S),not usage_missing(L,_,_).
usage_tier_one(L) :- strength(L,proved).
usage_tier_one(L) :- strength(L,refuted).
usage_tier_one(L) :- strength(L,certified).
usage_ready(L) :- record(L,promoted,usage-v1,_,_,_),usage_validated(L),not usage_tier_one(L).
usage_active_replace(N,O) :- supersedes(N,O),usage_ready(N).
usage_active_replace(N,O) :- supersedes(N,M),usage_ready(N),usage_active_replace(M,O).
usage_defeated(O,S) :- usage_active_replace(N,O),usage_scope(N,S),usage_scope(O,S).
usage_clashing(A,S) :- conflict(A,B),usage_ready(A),usage_ready(B),usage_scope(A,S),usage_scope(B,S),not usage_active_replace(A,B),not usage_active_replace(B,A).
usage_clashing(B,S) :- conflict(A,B),usage_ready(A),usage_ready(B),usage_scope(A,S),usage_scope(B,S),not usage_active_replace(A,B),not usage_active_replace(B,A).
usage_applies(L,S) :- usage_ready(L),usage_scope(L,S),not usage_defeated(L,S),not usage_clashing(L,S).
'''
SCHEMA = {"lesson":2,"scope":3,"strength":2,"effect":2,"evidence":3,"isa":2,
          "limit":4,"blocker":2,"cost":3,"prefer":3,"supersedes":2,"conflict":2,"record":6}
METADATA = set(SCHEMA)-{"isa","supersedes","conflict"}
DIMENSIONS = {"tool","version","family","environment","capability","prerequisite"}
MODES = {"observation","exact-precondition","reproduced-blocker","proved","refuted","certified"}


def conflict_failure(usage,a,b):
    replaces=usage.db.get('usage_replaces',())
    return usage.overlap(a,b) and (a,b) not in replaces and (b,a) not in replaces


def evidence_present(rows):
    return bool(rows)


def same_provenance(actual,expected):
    return actual==expected


def atom(pred,args):
    return pred+"("+",".join(t.name if isinstance(t,R.Var) else json.dumps(t) for t in args)+")"


def serialize(facts,rules):
    out=[atom(p,a)+"." for p,a in facts]
    for (p,a),body in rules:
        literals=[]
        for kind,args in body:
            if kind in ("yes","not"):
                literals.append(("not " if kind=="not" else "")+atom(*args))
            else:
                left,right=(t.name if isinstance(t,R.Var) else json.dumps(t) for t in args)
                literals.append(left+kind+right)
        out.append(atom(p,a)+" :- "+",".join(literals)+".")
    return "\n".join(out)


@dataclass(frozen=True)
class Validation:
    lesson:str
    recipe_sha256:str
    dependency_sha256:str
    mode:str
    family:str
    statement:tuple=()
    status:str="validated"
    last_validation:int=1


class Usage:
    def __init__(self,layers,task="",validations=()):
        if len(layers)>8 or sum(len(s.encode()) for s in layers)>65536:
            raise ValueError("layer-budget")
        facts,rules=[],[]
        self.overrides=[]
        for i,source in enumerate(layers):
            incoming,more=R.Parser(source).program()
            for pred,args in incoming+[head for head,_ in more]:
                if pred.startswith("usage_") or pred=="task":raise ValueError("reserved-predicate")
                if pred in SCHEMA and len(args)!=SCHEMA[pred]:raise ValueError("usage-arity")
            if any(p in METADATA|{'isa','supersedes'} for (p,_),_body in more):raise ValueError('record-rules-refused')
            ids={a[0] for p,a in incoming if p=="lesson"}
            for id in sorted(ids):
                old={(p,a) for p,a in facts if p in METADATA and a[0]==id}
                new={(p,a) for p,a in incoming if p in METADATA and a[0]==id}
                if old and old!=new:self.overrides.append((i,id,old,new))
            facts=[(p,a) for p,a in facts if p not in METADATA or a[0] not in ids]
            facts+=incoming;rules+=more
        tasks,tr=R.Parser(task).program()
        if tr or any(p!="task" or len(a)!=2 for p,a in tasks):raise ValueError("task-facts-only")
        self.task_rows={a for _,a in tasks}
        scalar={}
        for key,value in self.task_rows:
            named=key in DIMENSIONS|{'subject'}
            numeric=type(key) is str and re.fullmatch('[a-z][a-z0-9-]*-(count|bytes|work)',key) is not None
            if not named and not numeric:raise ValueError('task-key')
            if named and (type(value) is not str or not value or value=="any") or numeric and not (type(value) is int and value>=0 or value=='unknown'):raise ValueError('task-value')
            if key in {'tool','version','family','environment'} or numeric:
                if key in scalar and scalar[key]!=value:raise ValueError('task-conflict')
                scalar[key]=value
        records={a[0]:a for p,a in facts if p=="record"}
        evidence={a[0]:a for p,a in facts if p=="evidence"}
        for v in validations:
            record=records.get(v.lesson);ev=evidence.get(v.lesson)
            statement=tuple(sorted({(p,a) for p,a in facts if p in METADATA-{'record','evidence'} and a[0]==v.lesson},key=repr))
            if statement==v.statement and record== (v.lesson,"promoted","usage-v1",v.dependency_sha256,v.mode,v.family) and ev and ev[2]==v.recipe_sha256 and v.status=="validated" and v.last_validation>0:
                facts.append(("usage_validated",(v.lesson,)))
        self.direct_isa={a for p,a in facts if p=='isa'}
        self.kb=R.KB(serialize(facts+tasks,rules)+"\n"+RULES)
        self.db=self.kb.db
        self.lessons={r[0]:r[1] for r in self.db.get("lesson",())}

    def rows(self,pred,id=None):
        return sorted((r for r in self.db.get(pred,()) if id is None or r[0]==id),key=lambda r:tuple(R.key(v) for v in r))

    def subsumes(self,specific,general):
        return specific==general or (specific,general) in self.db.get("isa",())

    def matches(self,id,subject):
        if subject=="unknown" or not self.subsumes(subject,self.lessons[id]):return False
        for _,key,value in self.rows("scope",id):
            if not any(k==key and v!="unknown" and (value=='any' or v==value or key not in {"version","environment"} and self.subsumes(v,value)) for k,v in self.task_rows):return False
        return True

    def ready(self,id):
        return (id,) in self.db.get("usage_ready",())

    def overlap(self,a,b):
        for _,key,value in self.rows("scope",a):
            for _,k,v in self.rows("scope",b):
                if key!=k or value==v or value=='any' or v=='any':continue
                if key in {'version','environment'}:return False
                if key in {'tool','family'} and not self.subsumes(value,v) and not self.subsumes(v,value) and not any(self.subsumes(child,v) for child,parent in self.rows('isa') if parent==value):return False
        return True

    def check(self,require_validation=True):
        problems=set()
        for p,n in SCHEMA.items():
            for row in self.rows(p):
                if len(row)!=n:problems.add("kb-usage-arity");continue
                if type(row[0]) is not str:problems.add("kb-usage-type")
                if p in METADATA-{"lesson"} and row[0] not in self.lessons:problems.add("kb-unknown-lesson")
                if p in {"limit","cost"} and (type(row[2]) is not int or row[2]<0):problems.add("kb-usage-number")
                if p=="blocker" and row[1] in {"unknown","any"}:problems.add("kb-blocker-environment")
                if p=="limit" and row[3] not in {"count","bytes","work"}:problems.add("kb-limit-unit")
                if p=="evidence" and (not row[1] or type(row[2]) is not str or re.fullmatch('[0-9a-fA-F]{64}',row[2]) is None):problems.add("kb-evidence-format")
                if p=="scope" and (row[1] not in DIMENSIONS or type(row[2]) is not str or row[2] in {"","unknown"}):problems.add("kb-scope-key")
                if p=="effect" and row[1] not in {"helps","hurts","neutral","defect","limit"}:problems.add("kb-effect")
                if p=="strength" and row[1] not in {"declared","observed","measured","preregistered","proposed","proved","refuted","certified"}:problems.add("kb-strength")
                if p=="record" and (type(row[3]) is not str or re.fullmatch("[0-9a-fA-F]{64}",row[3]) is None or row[1] not in {"proposed","promoted","rejected"} or row[2]!="usage-v1" or row[4] not in MODES):problems.add("kb-record-schema")
        for id,strength in self.rows("strength"):
            if strength in {"proved","refuted","certified"} and (id,) not in self.db.get("usage_validated",()):
                problems.add("kb-fact-replay-required")
        for id in self.lessons:
            if re.fullmatch('[a-z0-9-]{1,96}',id) is None:problems.add("kb-lesson-id")
            for p,code in (("evidence","kb-lesson-without-evidence"),("strength","kb-lesson-without-strength"),("effect","kb-lesson-without-effect")):
                if not (evidence_present(self.rows(p,id)) if p=='evidence' else self.rows(p,id)):problems.add(code)
            if any(len(self.rows(p,id))!=1 for p in ("lesson","strength","effect")):problems.add("kb-lesson-ambiguous")
            if any(sum(r[1]==key for r in self.rows("scope",id))>1 for key in ("tool","version","family","environment")):problems.add("kb-scope-ambiguous")
            if len(self.rows("record",id))!=1:problems.add("kb-lesson-record")
            if (id,id) in self.db.get("usage_replaces",()):problems.add("kb-supersedes-cycle")
        for a,b in self.rows("isa"):
            if a==b:problems.add("kb-isa-cycle")
        for p in ("supersedes","conflict"):
            for pair in self.rows(p):
                if any(id not in self.lessons for id in pair):problems.add("kb-unknown-lesson")
        replaces=self.db.get("usage_replaces",())
        for a,b in self.rows("conflict"):
            if a in self.lessons and b in self.lessons and conflict_failure(self,a,b):problems.add("kb-unresolved-conflict")
        if require_validation and any(r[1]=='promoted' and not self.ready(r[0]) for r in self.rows('record')):problems.add('kb-lesson-unvalidated')
        if self.overrides:problems.add("kb-layer-conflict")
        return sorted(problems)

    def advise(self):
        if self.check(require_validation=False):return []
        replaces=self.db.get("usage_active_replace",())
        result=set()
        for key,subject in self.task_rows:
            if key!="subject":continue
            for id in self.lessons:
                if not self.ready(id) or not self.matches(id,subject):continue
                if any(self.ready(new) and self.matches(new,subject) and (new,id) in replaces for new in self.lessons):continue
                if any(self.ready(other) and self.matches(other,subject) and ((id,other) in self.db.get("conflict",()) or (other,id) in self.db.get("conflict",())) and (id,other) not in replaces and (other,id) not in replaces for other in self.lessons):continue
                result.add(id)
        return sorted(result)

    def exclusions(self):
        out=[]
        for id in self.advise():
            record=self.rows("record",id)[0]
            if record[4] not in {"exact-precondition","reproduced-blocker"}:continue
            for key,subject in self.task_rows:
                if key!="subject" or not self.matches(id,subject):continue
                for _,environment in self.rows("blocker",id):
                    if ("environment",environment) in self.task_rows:out.append((subject,id,"environment-blocked"))
                for _,metric,maximum,unit in self.rows("limit",id):
                    values=[v for k,v in self.task_rows if k==metric+'-'+unit]
                    if not values or 'unknown' in values:out.append((subject,id,"size-unknown"))
                    elif any(type(v) is not int or v<0 or v>maximum for v in values):out.append((subject,id,"size-limit"))
        return sorted(set(out))

    def explain(self,id):
        roots=[("usage_applies",row) for row in self.rows("usage_applies",id)]
        traces=[]
        for root in roots:
            stack=[(root,False)];seen=set();trace=[]
            while stack:
                f,expanded=stack.pop()
                if f in seen:continue
                d=self.kb.proofs[f]
                if expanded:seen.add(f);trace.append(d)
                else:stack.append((f,True));stack.extend((p,False) for p in reversed(d['premises']))
            traces.append(trace)
        return traces

    def verify_trace(self,trace):
        seen=set()
        for d in trace:
            assert d['fact'] in self.kb.proofs,"trace-snapshot"
            assert all(p in seen for p in d['premises']),"trace-parent-order"
            for p,args in d['absent']:
                assert not any(all(t==R.Var('_') or t==v for t,v in zip(args,row)) for row in self.db.get(p,())),"trace-negation"
            assert same_provenance(d,self.kb.proofs[d['fact']]),"trace-provenance"
            seen.add(d['fact'])
        assert trace,"trace-empty"


def validation_inputs(source):
    """Synthetic typed replay inputs for MODEL TESTS ONLY, never product receipts."""
    facts,_=R.Parser(source).program()
    evidence={a[0]:a for p,a in facts if p=="evidence"}
    return [Validation(a[0],evidence[a[0]][2],a[3],a[4],a[5],tuple(sorted({(p,args) for p,args in facts if p in METADATA-{'record','evidence'} and args[0]==a[0]},key=repr))) for p,a in facts if p=="record" and a[0] in evidence]


def plan(layers,task,validations,have,want,max_steps=8,produced=()):
    # Only observed artifacts are added. No possible output of a proposed step
    # becomes available until the caller supplies it on the next decision.
    task=task+''.join('task(subject,'+json.dumps(s)+').' for s in R.SIGNATURES)
    u=Usage(layers,task,validations)
    if R.check_code(u.kb) or u.check(require_validation=False):raise ValueError("inconsistent")
    exclusions=u.exclusions();blocked={s for s,_,reason in exclusions if reason!='size-unknown'}
    start=frozenset(set(have)|set(produced));want=set(want)
    if not start|want<=R.ARTIFACTS:raise ValueError("types")
    def rank(step):
        penalty=cost=0
        for id in u.advise():
            for _,preferred,other in u.rows('prefer',id):
                if u.rows('effect',id)[0][1] in {'helps','neutral'}:penalty+=other==step
            if u.matches(id,step):cost+=sum(n for _,unit,n in u.rows('cost',id) if unit=='work')
        return penalty,cost,step
    order=sorted(R.SIGNATURES,key=rank)
    queue=deque([(start,[])]);seen={start}
    while queue:
        state,path=queue.popleft()
        if want<=state:return {'steps':path,'next_step':path[0] if path else None,'exclusions':exclusions}
        if len(path)>=max_steps:continue
        for step in order:
            needs,makes=R.SIGNATURES[step]
            if step not in blocked and needs<=state and not makes<=state and state|makes not in seen:
                seen.add(state|makes);queue.append((state|makes,path+[step]))
    return {'steps':None,'next_step':None,'exclusions':exclusions}


def self_test(stub=None):
    from unittest.mock import patch
    public=R.EXAMPLE.read_text();validations=validation_inputs(public)
    task='task(subject,learn-terms).task(tool,zeno-fcis).task(version,v241).task(environment,capacity-demo).task(family,finite-learner).task(capability,finite-domain).task(prerequisite,vocabulary).task(tuples-count,4097).'
    u=Usage([public],task,validations)
    assert u.advise()==['learner-bound'],'advice-positive'
    unknown=Usage([public],'task(subject,learn-terms).',validations)
    if stub=='advice':
        with patch.object(Usage,'matches',lambda self,id,subject:self.subsumes(subject,self.lessons[id])):result=unknown.advise()
    else:result=unknown.advise()
    assert result==[],'advice-unknown-control'
    bad=public+'lesson(planted,check-program). strength(planted,declared).effect(planted,hurts).evidence(planted,r,"'+('a'*64)+'").record(planted,promoted,usage-v1,"'+('b'*64)+'",observation,f).'
    negative=Usage([bad],validations=validation_inputs(bad))
    if stub=='conflict':
        with patch(__name__+'.conflict_failure',return_value=False):problems=negative.check()
    else:problems=negative.check()
    assert problems==['kb-unresolved-conflict'],'conflict-control'
    missing=public+'lesson(unevidenced,sample).strength(unevidenced,observed).effect(unevidenced,neutral).record(unevidenced,proposed,usage-v1,"'+('b'*64)+'",observation,f).'
    negative=Usage([missing],validations=validations)
    if stub=='evidence':
        with patch(__name__+'.evidence_present',return_value=True):problems=negative.check()
    else:problems=negative.check()
    assert problems==['kb-lesson-without-evidence'],'evidence-control'
    trace=u.explain('learner-bound')[0];u.verify_trace(trace)
    forged=[dict(d) for d in trace];forged[-1]['rule']=999
    rejected=False
    try:
        if stub=='provenance':
            with patch(__name__+'.same_provenance',return_value=True):u.verify_trace(forged)
        else:u.verify_trace(forged)
    except AssertionError as e:
        assert str(e)=='trace-provenance';rejected=True
    assert rejected,'provenance-control'
    if stub=='inheritance':
        with patch.object(Usage,'subsumes',lambda self,a,b:a==b or (a,b) in self.direct_isa):assert u.advise()==['learner-bound'],'inheritance-control'




def verify_native_explanations(report, task='', layers=None):
    """Replay actual Rust wire traces against source rules and an independent DB.

    The Rust rule inventory supplies ordinal meaning only. Relation truth is
    recomputed by the independent snapshot evaluator above.
    """
    layers=layers or [R.EXAMPLE.read_text()]
    context=task
    if report.get('status')=='proposed-plan' or report.get('reason')=='kb-bounded-no-plan':
        context+=''.join('task(subject,'+json.dumps(s)+').' for s in R.SIGNATURES)
    u=Usage(layers,context,validation_inputs(R.EXAMPLE.read_text()))
    user_rules=[];origins=[];bases=[]
    for i,source in enumerate(layers):
        facts,rules=R.Parser(source).program();bases.append(set(facts));user_rules+=rules;origins += [i]*len(rules)
    source=(R.EXAMPLE.parents[1]/'src/strategy_knowledge/reasoning.rs').read_text()
    builtin=source.split('r#"',1)[1].split('"#;',1)[0]
    rules=user_rules+R.Parser(builtin).program()[1]
    origins += [None]*(len(rules)-len(origins))
    def ground(f):
        assert type(f) is dict and type(f.get('predicate')) is str and type(f.get('arguments')) is list,'trace-fact-shape'
        assert all(type(v) in (str,int) for v in f['arguments']),'trace-constant-type'
        return f['predicate'],tuple(f['arguments'])
    def witnesses(value):
        if type(value) is dict:
            if 'conclusion' in value and 'trace' in value:yield value
            else:
                for v in value.values():yield from witnesses(v)
        elif type(value) is list:
            for v in value:yield from witnesses(v)
    found=0
    for explanation in witnesses(report):
        found+=1;seen=set()
        trace=explanation['trace'];assert type(trace) is list and trace,'trace-empty'
        for node in trace:
            f=ground(node['fact']);pred,args=f
            assert args in u.db.get(pred,()),'trace-snapshot'
            parents=[ground(p) for p in node['premises']]
            assert all(p in seen for p in parents),'trace-parent-order'
            index=node['rule']
            if index is None:
                assert u.kb.proofs[f]['rule'] is None,'trace-base'
                layer=node['source_layer']
                if layer is not None:assert type(layer) is int and 0<=layer<len(bases) and f in bases[layer],'trace-layer'
                else:assert pred in {'task','usage_validated'},'trace-base-origin'
                assert not parents and not node['absent'] and not node['comparisons'],'trace-base-derivation'
            else:
                assert type(index) is int and 0<=index<len(rules),'trace-rule'
                (head,terms),body=rules[index]
                binding=node['bindings']
                assert node['source_layer']==origins[index],'trace-rule-origin'
                assert (head,tuple(u.kb.value(t,binding) for t in terms))==f,'trace-head'
                positives=[a for kind,a in body if kind=='yes']
                assert len(positives)==len(parents),'trace-premise-count'
                for (p,terms),(q,row) in zip(positives,parents):
                    assert p==q and len(terms)==len(row),'trace-premise'
                    assert all(t==R.Var('_') or u.kb.value(t,binding)==v for t,v in zip(terms,row)),'trace-binding'
                negatives=[a for kind,a in body if kind=='not']
                assert len(negatives)==len(node['absent']),'trace-negative-count'
                for (p,terms),lookup in zip(negatives,node['absent']):
                    expected=[None if t==R.Var('_') else u.kb.value(t,binding) for t in terms]
                    assert lookup=={'predicate':p,'arguments':expected},'trace-negative-binding'
                    assert not any(all(t is None or t==v for t,v in zip(expected,row)) for row in u.db.get(p,())),'trace-negation'
                comparisons=[(op,terms) for op,terms in body if op in {'=','!=','<','<=','>','>='}]
                assert len(comparisons)==len(node['comparisons']),'trace-comparison-count'
                for (op,terms),comparison in zip(comparisons,node['comparisons']):
                    x,y=(u.kb.value(t,binding) for t in terms)
                    assert comparison=={'operator':op,'left':x,'right':y},'trace-comparison-binding'
                    assert {'=':lambda:x==y,'!=':lambda:x!=y,'<':lambda:x<y,'<=':lambda:x<=y,'>':lambda:x>y,'>=':lambda:x>=y}[op](),'trace-comparison'
            seen.add(f)
        assert ground(explanation['conclusion']) in seen,'trace-conclusion'
    if report.get('status')=='advised':
        assert report['lessons']==u.advise(),'native-advice-exact'
        assert found>=len(report['lessons']),'native-advice-provenance'
    return found


if __name__=='__main__':
    import argparse
    p=argparse.ArgumentParser();p.add_argument('--self-test',action='store_true');p.add_argument('--stub',choices=('advice','conflict','evidence','provenance','inheritance'))
    a=p.parse_args();self_test(a.stub);print('usage-reference: PASS')

