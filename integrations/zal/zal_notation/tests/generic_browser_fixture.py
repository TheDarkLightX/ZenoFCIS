"""Five independently authored subjects, for tests and the actual private viewer."""
from zal_notation import abstractions as A, builtin, core, lens_library as V, lenses as L, sharing


def entry(identifier, title, story, context=None, **kwargs):
    discussion = sharing.make_discussion(context, builtin.generic_pack(context['grammar'])) if context else None
    return A.make_entry(identifier, title, story=story, discussion=discussion, **kwargs)


def request():
    draft = entry('draft', 'An unsettled program idea',
        'Maybe a decision tree; maybe ordered chunks or a graph. Exact source: 1e-400, 1, true.\r\nMeaning is still open.',
        examples=['Tree of decisions', 'Sequence of chunks', 'Graph of dependencies'],
        open_questions=['Which observations and operations should a view preserve?'])
    grammar, _, model = builtin.get_builtin('lambda')
    term = sharing.make_context(grammar, model,
        semantics={'profile':'user/program-syntax/1','parameters':{'exact':'1/3'},'description':'Author-supplied program syntax; execution and termination are unproved.'},
        assumptions=['Only the supplied syntax constructors are established.'],
        observation={'id':'syntax','description':'Observe the supplied syntax tree.','keeps':['constructors'],'omits':['execution','termination']})
    program = entry('custom-term','A program as supplied syntax',
        'The syntax is retained. Its evaluation and intended meaning remain unresolved.',term,
        open_questions=['Does this syntax implement the intended program?'])
    grammar, _, model = builtin.get_builtin('state-machine')
    graph = sharing.make_context(grammar, model,
        semantics={'profile':'user/relational-network/1','parameters':{},'description':'A declared relational network, not an execution claim.'},
        assumptions=['Nodes and edges have only their supplied relational meaning.'],
        observation={'id':'relations','description':'Observe declared nodes/edges.','keeps':['nodes','edges'],'omits':['execution']})
    network = entry('custom-graph','A relational network',
        'The labels are authored relations; this graph is not certified program behavior.',graph)
    counter = entry('counter','Counter with increment and reset',
        'A retained four-state counter: increment modulo four and reset to unseen. Outputs are constant; the full finite step context is supplied.',L.make_source())
    states=tuple(sorted(('unseen','amber','blue')));events=('advance','restart')
    ins=(L.finite.Port('state','phase','three_phases',states),L.finite.Port('event','command','control',events),L.finite.Port('authorized','environment','control',('yes',)))
    outs=(L.finite.Port('outcome','observation','control',('accepted',)),L.finite.Port('next_state','phase','three_phases',states),L.finite.Port('reason','observation','control',('declared',)))
    next_state={'unseen':'amber','amber':'blue','blue':'unseen'}
    rows={(s,e,'yes'):(('accepted',next_state[s] if e=='advance' else 'unseen','declared'),) for s in states for e in events}
    machine=L.finite.build(ins,outs,rows,(('state','next_state','unseen'),))
    phase=entry('phase','Three named phases',
        'Advance moves unseen → amber → blue → unseen; restart returns to unseen. This is a separate three-state subject.',L.make_source(machine))
    library=A.new_library('Five retained library objects')
    for item in (draft,program,network,counter,phase):library=A.register(library,item)
    registry=V.default_registry()
    registry=V.register(registry,V.definition('category-interpretation','Category interpretation',summary='An authored interpretation requires an explicit source-to-category bridge.',assumptions=['A category interpretation map remains to be established.'],preserves=[],forgets=[],adds=['A proposed category perspective'],mode='analogy'))
    target=entry('draft-sequence','A proposed chunk view','Proposed: identify ordered chunks. Their boundaries and semantic relationship are unproved.')
    checks=[{'entry_digest':core.digest(phase),'lens_id':'parity-quotient','parameters':{'mapping':{s:'phase' for s in states}},'construction_budget':100000,'check_budget':100000},
            {'entry_digest':core.digest(counter),'lens_id':'one-object-category','parameters':{},'construction_budget':100000,'check_budget':0}]
    return {'schema':'zal.library-lens-browser-request/1','library':library,'registry':registry,
        'entries':[core.digest(e) for e in (draft,program,network,counter,phase)],'checks':checks,
        'proposals':[{'entry_digest':core.digest(draft),'lens_id':'sequence-chunks','target_entry':target,
            'correspondence':{'description':'A proposed chunk interpretation of this exact idea.','pairs':[{'source':'story','target':'story'}]}}],
        'category_operations':[]}

