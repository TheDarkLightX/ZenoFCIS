"""Independent explicit-table oracle, not a second Mealy library or proof kernel."""
from collections import Counter


class OracleBound(Exception):
    pass


def audit(request, answer):
    """Recompute completed finite claims; evaluate refutation witnesses separately."""
    remaining = 500_000

    def spend():
        nonlocal remaining
        remaining -= 1
        if remaining < 0:
            raise OracleBound('independent table oracle exhausted')

    categories = {c['id']: c for c in request['categories']}
    functors = {f['id']: f for f in request['functors']}

    def parts(c):
        return ({a['id']: (a['source'], a['target']) for a in c['arrows']},
                dict(c['identities']), {(a, b): z for a, b, z in c['composition']})

    def category_ok(c):
        arrows, identities, table = parts(c)
        for o in c['objects']:
            spend()
            if arrows.get(identities.get(o)) != (o, o):
                return False
        expected = {(a, b) for a in arrows for b in arrows if arrows[a][1] == arrows[b][0]}
        if set(table) != expected:
            return False
        for (a, b), z in table.items():
            spend()
            if arrows.get(z) != (arrows[a][0], arrows[b][1]):
                return False
        for a, (s, t) in arrows.items():
            spend()
            if table[identities[s], a] != a or table[a, identities[t]] != a:
                return False
        for a, b in expected:
            for c in arrows:
                spend()
                if arrows[b][1] == arrows[c][0] and table[table[a, b], c] != table[a, table[b, c]]:
                    return False
        return True

    def functor_ok(f):
        source, target = categories[f['source']], categories[f['target']]
        sa, si, sc = parts(source)
        ta, ti, tc = parts(target)
        om, am = dict(f['objects']), dict(f['arrows'])
        if set(om) != set(source['objects']) or set(am) != set(sa):
            return False
        for o in source['objects']:
            spend()
            if om[o] not in target['objects'] or am[si[o]] != ti[om[o]]:
                return False
        for a, (s, t) in sa.items():
            spend()
            if ta.get(am[a]) != (om[s], om[t]):
                return False
        for (a, b), z in sc.items():
            spend()
            if tc[am[a], am[b]] != am[z]:
                return False
        return True

    status = answer['status']
    if status == 'RefutedAt':
        w = dict(answer['witness'])
        law = answer['law']
        # Replay the *reported* finite witness, rather than equating an arbitrary
        # false candidate with every possible negative result.
        for c in categories.values():
            arrows, identities, table = parts(c)
            if law == 'identity-endpoints' and w.get('object') in identities:
                if arrows.get(w.get('identity')) != (w['object'], w['object']):
                    return True
            if law == 'composition-typing' and all(w.get(k) in arrows for k in ['before', 'after', 'result']):
                a, b, z = (arrows[w[k]] for k in ['before', 'after', 'result'])
                if a[1] != b[0] or z != (a[0], b[1]):
                    return True
            if law == 'composition-total' and w.get('before') in arrows and w.get('after') in arrows:
                a, b = w['before'], w['after']
                if arrows[a][1] == arrows[b][0] and (a, b) not in table:
                    return True
            if law == 'identity-laws' and w.get('arrow') in arrows:
                a = w['arrow']; s, t = arrows[a]
                if table[identities[s], a] == w['left'] and table[a, identities[t]] == w['right'] and (w['left'] != a or w['right'] != a):
                    return True
            if law == 'associativity' and all(w.get(k) in arrows for k in ['first', 'second', 'third']):
                a, b, z = (w[k] for k in ['first', 'second', 'third'])
                if arrows[a][1] == arrows[b][0] and arrows[b][1] == arrows[z][0]:
                    left, right = table[table[a, b], z], table[a, table[b, z]]
                    if left == w['left'] and right == w['right'] and left != right:
                        return True
        if law.startswith('functor-'):
            f = functors.get(w.get('functor'))
            if f is None:
                return False
            sa, si, sc = parts(categories[f['source']]); ta, ti, tc = parts(categories[f['target']])
            om, am = dict(f['objects']), dict(f['arrows'])
            if law == 'functor-identity':
                actual, expected = am[si[w['object']]], ti[om[w['object']]]
            elif law == 'functor-endpoints':
                a, b = w['arrow'], w['image']
                return am[a] == b and ta[b] != tuple(om[o] for o in sa[a])
            elif law == 'functor-composition':
                a, b = w['before'], w['after']
                actual, expected = am[sc[a, b]], tc[am[a], am[b]]
            else:
                return False
            return actual == w['actual'] and expected == w['expected'] and actual != expected
        if law == 'invertible':
            c = categories[request['selection']['category']]
            arrows, identities, table = parts(c)
            a = request['selection']['arrow']; s, t = arrows[a]
            return w['arrow'] == a and not any(
                ends == (t, s) and table[a, b] == identities[s] and table[b, a] == identities[t]
                for b, ends in arrows.items())
        if law in ('typed-composition', 'typed-context-composition'):
            path = request['selection']['path']
            if request['operation'] == 'compose-functors':
                hops = [functors[i] for i in path]
                return any(a['target'] != b['source'] for a, b in zip(hops, hops[1:]))
            if request['operation'] == 'compose-symbolic':
                links = {r['id']: r for r in request['relationships']}
                hops = [links[i] for i in path]
                return any(a['target_key'] != b['source_key'] for a, b in zip(hops, hops[1:]))
            arrows, _, _ = parts(categories[request['selection']['category']])
            return any(arrows[a][1] != arrows[b][0] for a, b in zip(path, path[1:]))
        return False
    if status not in ('CheckedAt', 'Authored'):
        return status in ('Invalid', 'UNKNOWN') and 'result' not in answer
    if not all(category_ok(c) for c in categories.values()) or not all(functor_ok(f) for f in functors.values()):
        return False
    op, selected, result = request['operation'], request['selection'], answer['result']
    if op == 'check-category':
        c = categories[selected['category']]
        return result == {'claim': 'finite-category-laws', 'category': c['id'], 'objects': len(c['objects']), 'arrows': len(c['arrows'])}
    if op == 'compose-path':
        arrows, _, table = parts(categories[selected['category']]); path = selected['path']; value = path[0]
        for following in path[1:]:
            spend()
            if arrows[value][1] != arrows[following][0]:
                return False
            value = table[value, following]
        return result == {'claim': 'declared-category-path-composite', 'path': path, 'composite': value}
    if op == 'invertible':
        arrows, identities, table = parts(categories[selected['category']]); a = selected['arrow']; b = result['inverse']
        s, t = arrows[a]
        return result == {'claim': 'two-sided-inverse-in-declared-category', 'arrow': a, 'inverse': b} and arrows[b] == (t, s) and table[a, b] == identities[s] and table[b, a] == identities[t]
    if op == 'compose-symbolic':
        links = {r['id']: r for r in request['relationships']}; hops = [links[i] for i in selected['path']]
        return status == 'Authored' and result == {
            'claim': 'authored-symbolic-chain-only', 'path': selected['path'],
            'endpoints': [hops[0]['source_key'], hops[-1]['target_key']], 'hop_declarations': hops,
            'cumulative_forgets': [loss for h in hops for loss in h['forgets']]
        } and all(a['target_key'] == b['source_key'] for a, b in zip(hops, hops[1:]))
    path = [selected['functor']] if op == 'check-functor' else selected['path']
    hops = [functors[i] for i in path]; om, am = dict(hops[0]['objects']), dict(hops[0]['arrows'])
    for first, second in zip(hops, hops[1:]):
        spend()
        if first['target'] != second['source']:
            return False
        o, a = dict(second['objects']), dict(second['arrows'])
        om, am = {k: o[v] for k, v in om.items()}, {k: a[v] for k, v in am.items()}
    f = result['functor']
    def fiber_map(pairs):
        groups = {}
        for a, b in pairs:
            groups.setdefault(b, []).append(a)
        return {k: Counter(v) for k, v in groups.items()}
    expected_claim = 'finite-functor-laws' if op == 'check-functor' else 'finite-composite-functor-laws'
    expected_id = hops[0]['id'] if op == 'check-functor' or len(hops) == 1 else 'composite'
    return (status == 'CheckedAt' and result['claim'] == expected_claim and f['id'] == expected_id
            and f['source'] == hops[0]['source'] and f['target'] == hops[-1]['target']
            and dict(f['objects']) == om and dict(f['arrows']) == am
            and fiber_map([(a, b) for b, xs in f['object_fibers'] for a in xs]) == fiber_map(om.items())
            and fiber_map([(a, b) for b, xs in f['arrow_fibers'] for a in xs]) == fiber_map(am.items())
            and result['hop_declarations'] == hops
            and result['cumulative_forgets'] == [loss for h in hops for loss in h['forgets']])
