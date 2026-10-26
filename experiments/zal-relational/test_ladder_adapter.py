import json
from pathlib import Path
import ladder_adapter as adapter
import run
import oracle
from test_backend import request, expect

ROOT = Path(__file__).resolve().parent
NATIVE = ROOT.parents[1]
CONTEXT = ROOT/'examples/owner_cars_context.json'

def main():
    ladder, path = adapter.load_ladder(NATIVE)
    ctx = ladder.load(CONTEXT)
    texts = [('intuitive', 'every owner has a car.'),
             ('typed', 'forall x in D: Owner(x) => (exists1 y in D: Car(y) and Has(x, y))'),
             ('typed', 'forall x in D: Owner(x) => #{y in D: Car(y) and Has(x, y)} = 2')]
    queries = []
    views = []
    for rung, text in texts:
        parsed = ladder.parse(text, rung, ctx)
        first = adapter.translate(ladder, ctx, parsed, 2)
        for level in ladder.RUNGS:
            rendered = ladder.render(parsed, level, ctx)
            recovered = ladder.parse(rendered, level, ctx)
            query = adapter.translate(ladder, ctx, recovered, 2)
            expect(query == first, ('ladder changed lowered query', level))
            queries.append(query)
            views.append({'source': text, 'rung': level, 'text': rendered})
    answers, binding = run.worker([request(q) for q in queries])
    for q, a in zip(queries, answers):
        expect(a.get('status') == 'ExactCount' and a.get('value') == oracle.count(q), (q, a))
    # Three simultaneously needed variables cannot be collapsed into two slots.
    third = ladder.parse('forall x in D: forall y in D: forall z in D: (Has(x, y) and Has(y, z)) => Has(x, z)', 'typed', ctx)
    try:
        adapter.translate(ladder, ctx, third, 2)
    except ValueError as error:
        expect('third live variable' in str(error), str(error))
    else: raise AssertionError('three-variable transitivity was admitted')
    model_requests = []
    model_expected = []
    # Reuse original story fixtures, now through the Haskell model evaluator.
    original = ladder.parse(texts[0][1], texts[0][0], ctx)
    for fixture in ctx.data['fixtures']:
        labels = fixture['domains']['D']
        model = {name: {tuple(labels.index(label) for label in row) for row, truth in data['rows'] if truth}
                 for name, data in fixture['symbols'].items()}
        model_requests.append(request(adapter.translate(ladder, ctx, original, len(labels)), 'evaluate', model))
        model_expected.append(fixture['expected'])
    evaluated, _ = run.worker(model_requests)
    for expected, actual in zip(model_expected, evaluated):
        expect(actual == {'status': 'Evaluated', 'value': expected}, (expected, actual))
    report = {'roundtrip_count_agreements': len(queries), 'existing_story_fixtures': len(model_requests),
              'third_variable_refused': True, 'human_semantic_approval': 'not inferred',
              'native_binary_sha256': binding['binary_sha256'],
              'input_sha256': {str(p): run.sha(p.read_bytes()) for p in [path,path.with_name('ladder_en.json'),CONTEXT,Path(adapter.__file__),Path(__file__)]},
              'views': views, 'answers': answers}
    (ROOT/'evidence/ladder-integration.json').write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps({k:v for k,v in report.items() if k not in ('input_sha256','views','answers')}, indent=2))

if __name__=='__main__': main()
