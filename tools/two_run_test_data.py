"""Deterministic small-contract corpus for the independent two-run oracle."""
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def accept(post, when=True, outbox=None):
    return dict(when=when, **{'class':'Accept'}, reason=None, post=post, outbox=outbox or [])


def base(name, types, fields, genesis, cases, extra='', kinds=None, bindings=None, variables=None):
    project=f'''zeno 1;
project 1 {name.replace('-', '_')};
namespace 2 core;
type 100 state State;
type 101 command Command;
type 102 context Context;
{types}
{fields}
component 400 decision {{
  owns 100;
  reads pre.100;
  writes post.100;
  contexts context.102;
  budget steps 256;
}}
merge [400];
law 500 bounded on commit, genesis = post.100.110 >= 0;
{extra}
'''
    policy={'schema':'zeno-fcis/template-declarative-policy/2','template':name,
            'roots':dict(state=100,command=101,context=102),
            'leaf_bindings':{'102':['I128',0,0],**(bindings or {})},
            'variables':variables or {},'cases':cases,'genesis':genesis,
            'law_kinds':{'500':'StateInvariant',**(kinds or {})}}
    return project.rstrip()+"\n",policy


def order(observe, within=0):
    return dict(kind='commute', first='command.101.120 == 0',second='command.101.120 == 1',observe=observe,within=within)


def privacy(secret='pre.100.112', public=None):
    return dict(kind='noninterference',secret=[secret],public=public or ['post.100.110','post.100.111'])


def examples():
    swap=base('constant-product',
        'type 105 int ReserveX in 1..=4;\ntype 106 int ReserveY in 0..=3;\ntype 107 int User in 0..=1;\ntype 108 int Amount in 1..=2;',
        'field 110 100 reserve_x 105;\nfield 111 100 reserve_y 106;\nfield 112 100 alice_received 106;\nfield 113 100 bob_received 106;\nfield 120 101 user 107;\nfield 121 101 amount 108;',
        {'110':1,'111':3,'112':0,'113':0},
        [accept({'110':'pre.100.110 + command.101.121','111':'pre.100.111 - received',
                 '112':'choose(command.101.120 == 0, received, pre.100.112)',
                 '113':'choose(command.101.120 == 1, received, pre.100.113)'})],
        variables={'received':'mul_div_floor(command.101.121, pre.100.111, pre.100.110 + command.101.121)'})
    yield 'constant-product',*swap,[order(['post.100.110','post.100.111','post.100.112','post.100.113']),order(['post.100.110','post.100.111','post.100.112','post.100.113'],1)]
    batch=base('uniform-price',
        'type 105 int Price in 1..=2;\ntype 106 int Payment in 0..=2;\ntype 107 int Bit in 0..=1;',
        'field 110 100 price 105;\nfield 111 100 alice_paid 106;\nfield 112 100 bob_paid 106;\nfield 113 100 alice_filled 107;\nfield 114 100 bob_filled 107;\nfield 120 101 user 107;\nfield 121 101 amount 107;',
        {'110':1,'111':0,'112':0,'113':0,'114':0},
        [accept({'111':'choose(command.101.120 == 0, command.101.121 * pre.100.110, pre.100.111)',
                 '112':'choose(command.101.120 == 1, command.101.121 * pre.100.110, pre.100.112)',
                 '113':'choose(command.101.120 == 0, command.101.121, pre.100.113)',
                 '114':'choose(command.101.120 == 1, command.101.121, pre.100.114)'})])
    yield 'uniform-price',*batch,[order(['post.100.110','post.100.111','post.100.112','post.100.113','post.100.114'])]
    for leak in (False,True):
        name='ledger-leak' if leak else 'ledger-private'
        ledger=base(name,'type 105 int Balance in 0..=2;\ntype 106 bool Flag;\ntype 107 int Bit in 0..=1;',
            'field 110 100 balance 105;\nfield 111 100 sequence 107;\nfield 112 100 private_flag 106;\nfield 120 101 amount 107;',
            {'110':0,'111':0,'112':False},
            [accept({'110':'choose(pre.100.112, 1, 0)' if leak else 'pre.100.110 + command.101.120',
                     '111':'1 - pre.100.111'})],bindings={'106':['Bool']})
        yield name,*ledger,[privacy()]


def regressions():
    name='two-step-leak'
    project,policy=base(name,'type 105 int Bit in 0..=1;',
        'field 110 100 public_value 105; field 111 100 secret 105; field 112 100 hidden_copy 105; field 120 101 input 105;',
        {'110':0,'111':1,'112':0},
        [accept({'110':'choose(command.101.120 == 1, pre.100.112, pre.100.110)',
                 '112':'choose(command.101.120 == 0, pre.100.111, pre.100.112)'})])
    yield name,project,policy,[privacy(secret='pre.100.111',public=['post.100.110']),
                              privacy(secret='pre.100.111',public=['post.100.110','post.100.112'])]


def contracts(rng, count):
    for name, _, _, questions in examples():
        folder=ROOT/'examples/two-run'/name
        yield 'example:'+name,(folder/'project.zeno').read_text(),json.loads((folder/'v2/policy.json').read_text()),questions
    for name in ('refusal-reason-only','no-admissible','invalid-payload'):
        folder=ROOT/'test-data/two-run'/name
        yield 'regression:'+name,(folder/'project.zeno').read_text(),json.loads((folder/'v2/policy.json').read_text()),[privacy(secret='pre.100.111',public=['post.100.110'])]
    for name,_,_,questions in regressions():
        folder=ROOT/'test-data/two-run'/name
        yield 'regression:'+name,(folder/'project.zeno').read_text(),json.loads((folder/'v2/policy.json').read_text()),questions
    # Isolate each observed channel: public state and class cannot mask a leak.
    for index in range(8):
        bound=index%3+1
        fields='field 110 100 marker 105; field 111 100 value 105; field 112 100 secret 106; field 120 101 user 107;'
        types=f'type 105 int Amount in 0..={bound}; type 106 bool Flag; type 107 int Bit in 0..=1;'
        for channel in ('reject-reason','outbox'):
            name=f'{channel}-only-{index:02}'
            if channel=='reject-reason':
                cases=[dict(when='pre.100.112',**{'class':'Reject'},reason=200,post={},outbox=[]),
                       dict(when=True,**{'class':'Reject'},reason=201,post={},outbox=[])]
                extra='reason 200 a precedence 0; reason 201 b precedence 1;'
                bindings={'106':['Bool']}
            else:
                delivery=dict(ordinal=0,channel=300,destination='public',payload={'130':f'choose(pre.100.112, {bound}, 0)'},idempotency_ordinal=index)
                cases=[accept({},outbox=[delivery])]
                extra='type 103 destination Desk; type 104 payload Notice; field 130 104 amount 105; channel 300 notify destination 103 payload 104;'
                bindings={'106':['Bool'],'103':['Text',1,16]}
            project,policy=base(name,types,fields,{'110':0,'111':0,'112':False},cases,extra,bindings=bindings)
            question=privacy();question['expected_difference_kind']='reason' if channel=='reject-reason' else 'outbox'
            yield 'targeted:'+name,project,policy,[question]
    for index in range(count):
        bound=rng.randint(1,3); coefficient=rng.randint(0,3); k=rng.randint(0,2)
        expressions=['command.101.120','pre.100.111 + command.101.121',
            'choose(pre.100.112, 1, 0)',f'div_floor(pre.100.111 * {coefficient} + command.101.121, {rng.randint(1,3)})',
            f'choose(command.101.120 == 0, {rng.randint(0,bound)}, pre.100.111 + 1)',
            'choose(context.102 == 1, command.101.121, pre.100.111)',
            'div_floor(1, command.101.121)',
            'mul_div_floor(pre.100.111, command.101.121, 2)']
        post={'111':rng.choice(expressions),'110':rng.choice(['pre.100.110','command.101.121',f'{rng.randint(0,bound)}'])}
        extra='reason 200 denied precedence 0;'
        cases=[dict(when=rng.choice(['pre.100.112','context.102 == 0','command.101.121 == 0']),**{'class':'Reject'},reason=200,post={},outbox=[])]
        outbox=[]; bindings={'106':['Bool'],'102':['Bool']}
        if index%4==0:
            extra+='\ntype 103 destination Desk; type 104 payload Notice; field 130 104 amount 105; channel 300 notify destination 103 payload 104;'
            bindings['103']=['Text',1,16]
            outbox=[dict(ordinal=0,channel=300,destination='public',payload={'130':rng.choice(['command.101.121','choose(pre.100.112, 1, 0)'])},idempotency_ordinal=index%3)]
        kinds={}
        if index%7==0:
            extra+='\nreason 201 failed precedence 1;\nlaw 501 failure_effect on failure = post.100.111 >= 0;'
            kinds['501']='CommittedFailureEffects'
            cases.append(dict(when='command.101.120 == 0',**{'class':'CommittedFailure'},reason=201,post=post,outbox=outbox))
        if index%5==0:
            extra+='\nlaw 502 restricted on commit, genesis = post.100.112 == 0;'
            kinds['502']='StateInvariant'
        cases.append(accept(post,outbox=outbox))
        project,policy=base(f'random-{index:03}',f'type 105 int Amount in 0..={bound};\ntype 106 bool Flag;\ntype 107 int Bit in 0..=1;',
            'field 110 100 marker 105;\nfield 111 100 value 105;\nfield 112 100 secret 106;\nfield 120 101 user 107;\nfield 121 101 amount 107;',
            {'110':0,'111':0,'112':False},cases,extra,kinds,bindings)
        yield f'random-{index:03}',project,policy,[order(['post.100.110','post.100.111'],k),privacy(public=['post.100.110','post.100.111'])]


if __name__=='__main__':
    import argparse
    ap=argparse.ArgumentParser(description=__doc__);ap.add_argument('--regressions',action='store_true');args=ap.parse_args()
    for name,project,policy,_ in regressions() if args.regressions else examples():
        folder=ROOT/('test-data/two-run' if args.regressions else 'examples/two-run')/name
        (folder/'v2').mkdir(parents=True,exist_ok=True)
        (folder/'project.zeno').write_text(project)
        (folder/'v2/policy.json').write_text(json.dumps(policy,indent=2)+'\n')
        if args.regressions:
            properties={'schema':'zeno-fcis/run-properties/1','properties':[{'kind':'predicate','name':'public_stays_zero','formula':'post.100.110 == 0'}]}
            (folder/'v2/run-properties.json').write_text(json.dumps(properties,indent=2)+'\n')
