"""Fixed worker-free category attachment bridge; approvals never come from JSON."""
from copy import deepcopy

from . import host

OPERATIONS_SCOPE = 'Recorded finite table queries or authored symbolic hops; no live worker, source-meaning proof or universal claim.'


def approval(context):
    adapter = host.load_adapter()
    adapter.core.need(type(context) is host.OperatorContext and type(context.evidence) is adapter.EvidenceApproval,
                      'category attachments require an explicit trusted operator evidence context')
    evidence = context.evidence
    value = {'schema': 'zal/category-evidence-approval/1', 'records': sorted(evidence.record_digests),
             'binary_sha256': evidence.binary_sha256, 'build_record_sha256': evidence.build_record_sha256,
             'provider_manifest_sha256': evidence.provider_manifest_sha256}
    return {'schema': 'zal/category-browser-approval-identity/1', 'digest': adapter.core.digest(value),
            'declaration': value, 'scope': 'Identity of separately supplied operator evidence; this packet cannot enroll itself.'}


def operations(application, record):
    if record['status'] not in ('CheckedAt', 'Authored'):
        return None
    return {'kind': 'category-record', 'operation': application['operation'], 'recordedStatus': record['status'],
            'categories': deepcopy(application['parameters']['categories']),
            'selection': deepcopy(application['parameters']['selection']),
            'result': deepcopy(record['native_receipt']['result']), 'scope': OPERATIONS_SCOPE}


def attachment(library, entry, catalog, row, index, prefix, files, context, approved):
    adapter = host.load_adapter()
    identity = adapter.core.digest(entry)
    adapter.core.need(row['entry_digest'] == identity, 'category attachment source entry differs')
    app, record = row['application'], row['record']
    arguments = {'library': library, 'entry_digest': identity, 'application': app, 'record': record,
                 'evidence': context.evidence}
    incomplete = record.get('status') == 'UNKNOWN'
    audit = (adapter.inspect_incomplete_record(**arguments) if incomplete else adapter.audit_record(**arguments))
    adapter.core.need(audit['status'] == ('RecordedIncomplete' if incomplete else 'Audited'),
                      'category static validation refused: ' + audit.get('reason', audit['status']))
    native = record['native_receipt']
    declaration = app['interpretation']['declaration']
    result = native.get('result', {})
    path = prefix + f'/category-{index}.json'
    category = {'schema': 'zal/library-category-browser-record/1', 'application': deepcopy(app),
                'record': deepcopy(record), 'audit': audit, 'approvalDigest': approved['digest'],
                'applicationDigest': adapter.core.digest(app), 'recordDigest': adapter._digest(record),
                'catalogDigest': adapter.core.digest(catalog), 'registryDigest': catalog['registry_digest']}
    files[path] = (adapter.core.canonical(category) + '\n').encode('utf-8')
    mapping = []
    functor = result.get('functor')
    if functor:
        for kind, field in [('object', 'objects'), ('arrow', 'arrows')]:
            mapping += [{'source': kind + ':' + a, 'target': kind + ':' + b} for a, b in functor[field]]
    assumptions = {'interpretation': declaration['assumptions'],
                   'source': app['context']['assumptions'] if app['context'] else []}
    for i, hop in enumerate(result.get('hop_declarations', [])):
        assumptions[f'hop {i+1} ({hop["id"]})'] = hop['assumptions']
    status = {'CheckedAt': 'checked', 'RefutedAt': 'refuted', 'Authored': 'authored', 'UNKNOWN': 'incomplete'}[record['status']]
    claim = result.get('claim') or native.get('law') or native['message']
    return {'id': f'category-operation-{index}', 'name': app['operation'] + ' · recorded Lens view',
            'type': 'category', 'status': status, 'summary': claim,
            'sourceEntryDigest': identity, 'sourceContextDigest': app['context_digest'] if app['context'] else None,
            'sourcePath': prefix + '/source.entry.json', 'receiptPath': path,
            'preserves': declaration['preserves'],
            'forgets': declaration['forgets'] + result.get('cumulative_forgets', []), 'adds': [],
            'assumptions': assumptions, 'mapping': mapping, 'category': category,
            'operations': operations(app, record), 'evidenceScope': 'finite-category' if status != 'authored' else 'authored-symbolic',
            'bounds': ['Prior recorded native result: ' + record['status'],
                       'Static validation: ' + audit['static_table_audit'],
                       'Source interpretation: ' + app['interpretation']['status'], record['refutation_scope'],
                       'Work / budget: ' + str(native['work_used']) + ' / ' + str(app['budget']),
                       'Finite table laws cover explicit finite declarations; symbolic and infinite interpretations remain authored.'],
            'relationClaims': app['interpretation']['claim'] + ' Hop preservation and loss declarations remain authored.'}
