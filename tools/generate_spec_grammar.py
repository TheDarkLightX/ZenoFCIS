#!/usr/bin/env python3
"""Generate first-order ordered grammar tables; --check refuses stale output.

The JSON is the reviewable grammar. This deterministic generator is not a proof
assistant. Runtime derivation checking uses the generated production/decision
arrays, whose exact bytes participate in the specification source identity.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SOURCE = ROOT / 'crates/zeno-fcis-spec/grammar/zeno-v1.json'
OUTPUT = ROOT / 'crates/zeno-fcis-spec/src/syntax_grammar.rs'
EXTENDED_SOURCE = ROOT / 'crates/zeno-fcis-spec/grammar/zeno-v1-extended.json'
EXTENDED_OUTPUT = ROOT / 'crates/zeno-fcis-spec/src/syntax_grammar_extended.rs'
PUNCT = {'{':'LBrace','}':'RBrace','(':'LParen',')':'RParen','[':'LBracket',']':'RBracket',';':'Semicolon',',':'Comma','.':'Dot','..':'Range',':':'Colon','=':'Equal','==':'EqEq','!=':'NotEq','<':'Less','<=':'LessEq','>':'Greater','>=':'GreaterEq','+':'Plus','-':'Minus','*':'Star','/':'Slash','&&':'AndAnd','||':'OrOr','!':'Bang','->':'Arrow'}


def compile_grammar(data):
    if data['schema'] != 'zeno/ordered-syntax-grammar/1':
        raise ValueError('unsupported grammar schema')
    source_rules = data['rules']
    names = [r['name'] for r in source_rules]
    if len(names) != len(set(names)):
        raise ValueError('duplicate rule')
    rules = []
    helpers = []
    counter = 0
    def expand(items, owner):
        nonlocal counter
        result = []
        for item in items:
            if isinstance(item,str):
                result.append(item)
                continue
            if not isinstance(item,dict) or len(item)!=1 or next(iter(item)) not in ('optional','many'):
                raise ValueError('invalid EBNF item')
            kind, body = next(iter(item.items()))
            counter += 1
            name = f'_{owner}_{kind}_{counter}'
            rhs = expand(body,owner)
            if not rhs:
                raise ValueError('empty repetition/optional')
            helpers.append({'name':name,'alternatives':[{'label':'some','rhs':rhs+([name] if kind=='many' else [])},{'label':'empty','rhs':[]}]})
            result.append(name)
        return result
    for rule in source_rules:
        rules.append({'name':rule['name'],'alternatives':[{**alt,'rhs':expand(alt['rhs'],rule['name'])} for alt in rule['alternatives']]})
    rules.extend(helpers)
    raw_terms = {s[1:] for r in rules for a in r['alternatives'] for s in a['rhs'] if s.startswith('@')}
    keywords = sorted(raw_terms - set(PUNCT) - {'IDENT','NUMBER','HEX','EOF'})
    terminals = ['EOF','IDENT','NUMBER','HEX'] + list(PUNCT) + keywords
    tags = {s:i for i,s in enumerate(terminals)}
    for rule in rules:
        if rule['name']=='Identifier':
            rule['alternatives']=[{'label':'identifier','rhs':['@IDENT']}]+[{'label':word,'rhs':['@'+word]} for word in keywords]
    rule_ids = {r['name']:i for i,r in enumerate(rules)}
    if len(rule_ids)!=len(rules):
        raise ValueError('helper name collision')
    for rule in rules:
        for alt in rule['alternatives']:
            for sym in alt['rhs']:
                if sym.startswith('@'):
                    if sym[1:] not in tags: raise ValueError('unknown terminal '+sym)
                elif sym not in rule_ids: raise ValueError('unknown rule '+sym)
    nullable = set()
    first = {name:set() for name in rule_ids}
    def first_rhs(rhs):
        found=set()
        for sym in rhs:
            if sym.startswith('@'):
                found.add(tags[sym[1:]])
                return found,False
            found.update(first[sym])
            if sym not in nullable: return found,False
        return found,True
    changed=True
    while changed:
        changed=False
        for rule in rules:
            name=rule['name']
            for alt in rule['alternatives']:
                fs,nul=first_rhs(alt['rhs'])
                if not fs<=first[name]: first[name].update(fs);changed=True
                if nul and name not in nullable: nullable.add(name);changed=True
    # A left-recursive/no-consumption cycle would defeat a bounded predictive
    # implementation's useful completeness. Refuse it at generation time.
    edges={name:set() for name in rule_ids}
    for rule in rules:
        for alt in rule['alternatives']:
            for sym in alt['rhs']:
                if sym.startswith('@'): break
                edges[rule['name']].add(sym)
                if sym not in nullable: break
    done=set()
    def visit(name,active):
        if name in active: raise ValueError('left-recursive grammar at '+name)
        if name in done:return
        for child in sorted(edges[name]):visit(child,active|{name})
        done.add(name)
    for name in rule_ids:visit(name,set())
    productions=[];rhs=[];labels=[];decisions=[];ranges=[]
    identifiers={tags['IDENT']}|{tags[word] for word in keywords}
    for rule in rules:
        begin=len(decisions)
        for alt in rule['alternatives']:
            p=len(productions)
            productions.append((rule_ids[rule['name']],len(rhs),len(alt['rhs'])))
            rhs.extend([(s.startswith('@'), tags[s[1:]] if s.startswith('@') else rule_ids[s]) for s in alt['rhs']])
            labels.append(alt['label'])
            when=alt.get('when')
            if when:
                if when.get('always'):
                    fs={None}
                elif when.get('first')=='IDENTIFIERS':
                    fs=identifiers-{tags[w] for w in when.get('except_first',[])}
                else:
                    fs={tags[w] for w in when['first']}
                second=when.get('second',[])
                seconds=identifiers if second=='IDENTIFIERS' else ({tags[w] for w in second} or {None})
            else:
                fs,nul=first_rhs(alt['rhs'])
                if nul:fs={None}
                seconds={None}
            for a in sorted(fs,key=lambda x:-1 if x is None else x):
                for b in sorted(seconds,key=lambda x:-1 if x is None else x):
                    decisions.append((rule_ids[rule['name']],a,b,p))
        ranges.append((begin,len(decisions)-begin))
    return rules,terminals,productions,rhs,labels,decisions,ranges,keywords


def render(data, source_name='zeno-v1.json', include_path='../grammar/zeno-v1.json'):
    rules,terms,prods,rhs,labels,decs,ranges,keywords=compile_grammar(data)
    tag={s:i for i,s in enumerate(terms)}
    lines=[f'// @generated by tools/generate_spec_grammar.py; edit grammar/{source_name}.',
        '// First matching decision commits. Terminal payloads remain in source tokens.',
        '#![allow(missing_docs)]','use crate::surface_lexer::{Token, TokenKind};',
        'use crate::syntax_certificate::{ANY, Decision, Production, Symbol};',
        'pub(crate) const EOF_TAG: u32 = 0;',
        f'pub(crate) const ROOT_TAG: u32 = {next(i for i,r in enumerate(rules) if r["name"]==data["root"])};',
        f'pub(crate) const GRAMMAR_JSON: &str = include_str!({json.dumps(include_path)});',
        '#[rustfmt::skip]', 'pub(crate) const RULE_NAMES: &[&str] = &[']
    lines += ['    '+json.dumps(r['name'])+',' for r in rules]+['];','#[rustfmt::skip]', 'const TERMINAL_NAMES: &[&str] = &[']
    lines += ['    '+json.dumps(t)+',' for t in terms]+['];','#[rustfmt::skip]', 'pub(crate) const PRODUCTIONS: &[Production] = &[']
    lines += [f'    Production {{ lhs: {a}, rhs_start: {b}, rhs_len: {c} }},' for a,b,c in prods]+['];','#[rustfmt::skip]', 'pub(crate) const RHS: &[Symbol] = &[']
    lines += [f'    Symbol {{ terminal: {str(a).lower()}, tag: {b} }},' for a,b in rhs]+['];','#[rustfmt::skip]', 'pub(crate) const DECISIONS: &[Decision] = &[']
    lines += [f'    Decision {{ lhs: {a}, first: {"ANY" if b is None else b}, second: {"ANY" if c is None else c}, production: {d} }},' for a,b,c,d in decs]+['];','#[rustfmt::skip]', 'pub(crate) const DECISION_RANGES: &[(usize, usize)] = &[']
    lines += [f'    ({a}, {b}),' for a,b in ranges]+['];']
    declaration = next(i for i,r in enumerate(rules) if r['name']=='Declaration')
    starts = sorted({first for lhs,first,second,prod in decs if lhs==declaration and first is not None})
    lines += ['#[rustfmt::skip]', 'const DECLARATION_START_TAGS: &[u32] = &['+', '.join(map(str,starts))+'];', 'pub(crate) fn is_declaration_start(token: &Token) -> bool {', '    DECLARATION_START_TAGS.contains(&token_tag(token))', '}']
    lines += ['pub(crate) fn rule_name(tag: u32) -> Option<&\'static str> {', '    RULE_NAMES.get(tag as usize).copied()', '}',
        'pub(crate) fn terminal_name(tag: u32) -> Option<&\'static str> {', '    TERMINAL_NAMES.get(tag as usize).copied()', '}', 'pub(crate) fn token_tag(token: &Token) -> u32 {','    match &token.kind {',
        '        TokenKind::End => EOF_TAG,','        TokenKind::Number(_) => 2,','        TokenKind::Hex => 3,','        TokenKind::Ident => match token.spelling {']
    lines += [f'            {json.dumps(word)} => {tag[word]},' for word in keywords]+['            _ => 1,','        },']
    lines += [f'        TokenKind::{variant} => {tag[word]},' for word,variant in PUNCT.items()]+['        _ => ANY,','    }','}','']
    return '\n'.join(lines)


def main():
    p=argparse.ArgumentParser(description=__doc__)
    p.add_argument('--check',action='store_true')
    p.add_argument('--grammar',type=Path,help='explicit grammar JSON (requires --output)')
    p.add_argument('--output',type=Path,help='explicit generated Rust path (requires --grammar)')
    args=p.parse_args()
    if bool(args.grammar) != bool(args.output):
        p.error('--grammar and --output must be supplied together')
    variants=[(args.grammar,args.output)] if args.grammar else [(SOURCE,OUTPUT),(EXTENDED_SOURCE,EXTENDED_OUTPUT)]
    for source,output in variants:
        data=json.loads(source.read_text())
        include_path=Path(os.path.relpath(source.resolve(),output.resolve().parent)).as_posix()
        generated=render(data,source.name,include_path)
        if args.check:
            if not output.is_file() or output.read_text()!=generated:
                raise SystemExit(f'generated syntax grammar differs for {source.name}; rerun generator')
        else:
            output.parent.mkdir(parents=True,exist_ok=True)
            output.write_text(generated)
        r,t,pd,rhs,l,d,dr,k=compile_grammar(data)
        print(json.dumps({'status':'matches' if args.check else 'generated','grammar':source.name,'rules':len(r),'productions':len(pd),'decisions':len(d),'terminals':len(t),'grammar_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'output_sha256':hashlib.sha256(generated.encode()).hexdigest()},sort_keys=True))

if __name__=='__main__':main()
