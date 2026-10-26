"""Bounded arithmetic-semantics / microprogram co-search; standard library only.

This does NOT discover a universal machine. The declared one-operation grammar
has only straight-line programs. For SUBLEQ all branch targets are next, so the
conditional branch has no control-flow effect. Other candidates are arithmetic
update functions with the same source/destination selectors. All implementation
costs, branching, general memory, I/O and universality remain outside this demo.

State X=x, Y=y, T=0. Only Y/T may be written; X must remain x. Every target
restores T=0. Each search merges EXACT symbolic integer states, not sampled
observations, and breadth-first exploration gives minimum length only within
this grammar and mathematical-integer semantics. A verified integer affine
identity remains valid modulo 2**w, but minimality need not transfer to small w.
"""
from itertools import product
from pathlib import Path
import json, time, hashlib

NAMES=('X','Y','T')
INITIAL=((1,0),(0,1),(0,0))
OPS=tuple((a,b) for b in (1,2) for a in range(3))
MACHINES=('add','subtract','reverse_subtract','copy','negative_copy')
TARGETS={'add_Y_X':((1,0),(1,1),(0,0)),
         'subtract_Y_X':((1,0),(-1,1),(0,0)),
         'copy_X_to_Y':((1,0),(1,0),(0,0))}
MAX_DEPTH=5

def symbolic_step(state,instruction,machine):
    a,b=instruction
    va,vb=state[a],state[b]
    if machine == 'add': result=tuple(a+b for a,b in zip(va,vb))
    elif machine == 'subtract': result=tuple(b-a for a,b in zip(va,vb))
    elif machine == 'reverse_subtract': result=tuple(a-b for a,b in zip(va,vb))
    elif machine == 'copy': result=va
    elif machine == 'negative_copy': result=tuple(-a for a in va)
    else: raise ValueError(machine)
    updated=list(state);updated[b]=result
    return tuple(updated)

def search(machine):
    frontier={INITIAL:()}
    seen={INITIAL}
    solutions={}
    ledger=[]
    for depth in range(MAX_DEPTH+1):
        newly_found=[]
        for name,goal in TARGETS.items():
            if name not in solutions and goal in frontier:
                solutions[name]=frontier[goal]
                newly_found.append(name)
        ledger.append({'depth':depth,'frontier_states':len(frontier),'new_targets':newly_found})
        if depth == MAX_DEPTH or len(solutions)==len(TARGETS): break
        following={}
        for state,path in frontier.items():
            for instruction in OPS:
                updated=symbolic_step(state,instruction,machine)
                if updated not in seen:
                    seen.add(updated)
                    following[updated]=path+(instruction,)
        frontier=following
    return solutions,ledger,len(seen)

# Separate concrete interpretation; no use of coefficient transition function.
def interpret(program,machine,x,y,width=4,tmp=0):
    memory=[x,y,tmp]; mask=(1<<width)-1
    for a,b in program:
        source,dest=memory[a],memory[b]
        if machine=='add': updated=dest+source
        elif machine=='subtract': updated=dest-source
        elif machine=='reverse_subtract': updated=source-dest
        elif machine=='copy': updated=source
        elif machine=='negative_copy': updated=-source
        else: raise ValueError(machine)
        memory[b]=updated & mask
    return tuple(memory)

def expected(target,x,y,width):
    mask=(1<<width)-1
    if target=='add_Y_X': out=x+y
    elif target=='subtract_Y_X': out=y-x
    elif target=='copy_X_to_Y': out=x
    else: raise ValueError(target)
    return x,out&mask,0

def first_failure(program,machine,target,width=4,tmp=0):
    for x,y in product(range(1<<width),repeat=2):
        got=interpret(program,machine,x,y,width,tmp)
        want=expected(target,x,y,width)
        if got!=want: return {'inputs':{'X':x,'Y':y,'T':tmp},'actual':got,'expected':want}
    return None

def main():
    started=time.perf_counter()
    rows=[]
    all_programs={}
    for machine in MACHINES:
        solutions,ledger,states=search(machine)
        all_programs[machine]=solutions
        targets={}
        for name in TARGETS:
            if name not in solutions:
                targets[name]={'status':'no program through depth 5 in declared integer grammar'}
                continue
            program=solutions[name]
            final=INITIAL
            for instruction in program:final=symbolic_step(final,instruction,machine)
            if final!=TARGETS[name]: raise RuntimeError('Symbolic mismatch')
            counterexample=first_failure(program,machine,name)
            if counterexample is not None:raise RuntimeError(counterexample)
            targets[name]={'status':'exact integer affine identity checked',
                           'minimum_length_in_integer_grammar':len(program),
                           'program':[{'A':NAMES[a],'B':NAMES[b],'branch_target':'next'} for a,b in program],
                           'final_affine_coefficients_x_y':final,
                           'independent_modular_check':{'width':4,'input_pairs':256,'passed':True}}
        rows.append({'machine':machine,'targets':targets,'search_ledger':ledger,'distinct_states':states})
    add_program=all_programs['subtract']['add_Y_X']
    mutation=first_failure(add_program[:-1],'subtract','add_Y_X')
    wrong_premise=first_failure(add_program,'subtract','add_Y_X',tmp=1)
    if mutation is None or wrong_premise is None:raise RuntimeError('Missing negative control')
    report={
      'schema':'bounded-arithmetic-machine-search/v1',
      'claim':'bounded exact symbolic machine/microprogram co-search; no new ISA or universality claim',
      'source_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
      'semantics':{'add':'B = B + A','subtract':'B = B - A (SUBLEQ with branch-to-next)',
                   'reverse_subtract':'B = A - B','copy':'B = A','negative_copy':'B = -A'},
      'grammar':{'initial':'X=x,Y=y,T=0','writeable':['Y','T'],'readable':list(NAMES),
                 'instructions_per_position':6,'max_depth':MAX_DEPTH,
                 'required_final':'X=x,T=0, Y=declared target','branches':'none effective'},
      'results':rows,
      'controls':{'drop_final_scratch_reset':mutation,'remove_zero_scratch_precondition':wrong_premise},
      'limits':['Trusted Python symbolic interpreter, no Lean/SMT proof object',
                'Search cost is not native runtime speed or hardware area',
                'Integer minimality does not imply fixed-width minimality',
                'No dynamic memory, effective branching, I/O, compiler/runtime refinement or universality theorem'],
      'elapsed_seconds':round(time.perf_counter()-started,6)}
    print(json.dumps(report,indent=2))

if __name__=='__main__':main()
