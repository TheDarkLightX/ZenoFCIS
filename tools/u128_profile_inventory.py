#!/usr/bin/env python3
"""Source-bound pending inventory for the coordinated 2.5.1 re-proof.

This never runs Verus, refreshes an approved VIR inventory, or accepts a proof.
It lists the exact changed subject sources and new obligations without reusing
old checked counts as evidence for the U128 extension.
"""
import argparse
import hashlib
import json
from pathlib import Path
ROOT=Path(__file__).resolve().parents[1]
OUT=ROOT/'verification/verus/u128_profile_extensions.json'
PROFILES=('finite-execution','metered-execution','canonical-bytes','protected-input','envelope-frame','schema-binding','decision','laws-v2','composition','catalog_v2','authority_v2','original-output','publication-v2','guarded-laws','scalar-query','checked-assembly','checker')
SUBJECTS=(
 'crates/zeno-fcis-cli/src/contract/render.rs','crates/zeno-fcis-cli/src/contract/review/unsigned.rs','crates/zeno-fcis-contract/src/internals/rules.rs','crates/zeno-fcis-contract/src/internals/schema.rs','crates/zeno-fcis-contract/src/internals/review/evaluate.rs',
 'crates/zeno-fcis-spec/src/ast.rs','crates/zeno-fcis-spec/src/parser.rs','crates/zeno-fcis-spec/src/elaborate.rs',
 'crates/zeno-fcis-contract/src/internals/declarations.rs','crates/zeno-fcis-contract/src/internals/expr.rs','crates/zeno-fcis-contract/src/internals/graph.rs','crates/zeno-fcis-contract/src/internals/model.rs','crates/zeno-fcis-contract/src/internals/quantity.rs','crates/zeno-fcis-contract/src/internals/policy.rs','crates/zeno-fcis-contract/src/internals/delivery.rs',
 'crates/zeno-fcis-synthesis/src/finite/evaluation/typed.rs','crates/zeno-fcis-synthesis/src/finite/evaluation/typed/spec.rs','crates/zeno-fcis-synthesis/src/finite/evaluation/typed/admission.rs','crates/zeno-fcis-synthesis/src/finite/evaluation/typed/admission/spec.rs',
 'crates/zeno-fcis-synthesis/src/finite/execution_v2/composition/admission/numeric.rs','crates/zeno-fcis-synthesis/src/finite/execution_v2/composition/admission/numeric/spec.rs','crates/zeno-fcis-synthesis/src/finite/execution_v2/composition/ingress/shared.rs','crates/zeno-fcis-synthesis/src/finite/execution_v2/composition/ingress/shared_spec.rs','crates/zeno-fcis-synthesis/src/finite/execution_v2/input_view/typed.rs','crates/zeno-fcis-synthesis/src/finite/execution_v2/input_view/typed/spec.rs',
 'crates/zeno-fcis-synthesis/src/finite/execution_v2/authority/numeric_metadata.rs','crates/zeno-fcis-synthesis/src/finite/execution_v2/authority/numeric_metadata_spec.rs','crates/zeno-fcis-synthesis/src/finite/value/mod.rs','crates/zeno-fcis-synthesis/src/finite/value/wide.rs',
 'verification/verus/arithmetic_lowering.rs','verification/arithmetic/unsigned_lowering.rs',
)
SUBJECTS += ('crates/zeno-fcis-synthesis/src/finite/execution_v2/tables/authority.rs', 'crates/zeno-fcis-synthesis/src/finite/execution_v2/tables/collection.rs', 'crates/zeno-fcis-synthesis/src/finite/execution_v2/tables/expression.rs', 'crates/zeno-fcis-synthesis/src/finite/execution_v2/tables/mod.rs', 'crates/zeno-fcis-synthesis/src/finite/execution_v2/tables/order_laws.rs', 'crates/zeno-fcis-synthesis/src/finite/execution_v2/tables/ordered.rs', 'crates/zeno-fcis-synthesis/src/finite/execution_v2/tables/policy.rs', 'crates/zeno-fcis-synthesis/src/finite/execution_v2/tables/redemption.rs', 'crates/zeno-fcis-synthesis/src/finite/execution_v2/tables/redemption_laws.rs', 'crates/zeno-fcis-synthesis/src/finite/execution_v2/tables/schema.rs', 'crates/zeno-fcis-contract/src/internals/tables.rs', 'crates/zeno-fcis-contract/src/internals/tables_review.rs')
def sha(path):return hashlib.sha256((ROOT/path).read_bytes()).hexdigest()
def render():
    sources={p:sha(p) for p in sorted(SUBJECTS)}
    obligations=[
      'Public unsigned bounds and canonical decimals; explicit policy /4 opt-in including signed-only subjects; strict kinds for used and unused bindings; ProjectSpec format 3 for unsigned syntax.',
      'Opposite-kind public casts, complete success and arithmetic-refusal partitions, Bool and variant exclusion.',
      'Checked unsigned add/sub/mul/division; exact 256-bit fused floor/ceil, including ceil-only overflow.',
      'Exact 256/384-bit product comparison and typed one padding.',
      'Nominal units, scales e0..e38, static scaled-operand fit, ordered chains and refusal prefixes.',
      'Compiler-to-typed-graph erasure, original schema/metadata correspondence and unchanged legacy /2 and /3 bytes.',
      'Exact sum-code projection separately from original domain equality; unsigned values never use that projection.',
      'One metered eager driver, actual refusal/retry usage, exclusive invocation scratch and immutable retained inputs/results.',
      'Original law/delivery/output/publication bytes, exact replay, catalog/source binding and independent admission.',
      'U128 tables reconstruct exact unsigned kinds; law-local U256 sums require established static capacity bounds; ordered U128 NICR uses wide intermediates. Restricted scalar consumers refuse table subjects. Native finite/oracle/defect checks do not qualify shared Verus profiles or grant publication authority.',
    ]
    return json.dumps({'schema':'zeno-fcis/u128-profile-extensions/1','status':'pending-2.5.1-freeze','verified':False,'proof_authority':False,
      'base':'8fcd0c53704a9b0bc340f805162542c2520dea0a + accepted W25b patch 5c610306b48058cb4410de8c3e6a4aa6f603c7d55a29c2bb716e8d356165276d',
      'profiles':[{'id':p,'retained_inventory':f'verification/verus/{p}.json','retained_inventory_sha256':sha(f'verification/verus/{p}.json'),'extension':'U128 typed graph, casts, sum projection, metadata and full shared-source closure; retranslate and reprove at freeze','old_counts_qualify_extension':False} for p in PROFILES],
      'arithmetic-lowering':{'harness':'verification/verus/arithmetic_lowering.rs','extension':'verification/arithmetic/unsigned_lowering.rs','obligations':obligations,'qualification':'pending; native compilation is not Verus acceptance'},
      'subject_sources_sha256':sources,'required_native_controls':['u128_checked_arithmetic_oracle','u128_conversion_boundaries','u128_fused_wide_oracle','u128_table_wide_sum_oracle','u128_table_keys_preserve_kind','u128_table_nicr_wide_oracle','u128_sum_requires_static_bound'],'next_gate':'At the 2.5.1 freeze, translate the complete final subjects, inspect every inventory delta, prove and run pinned semantic controls together with tables. Do not copy previous checked counts.'},indent=2)+'\n'
if __name__=='__main__':
    p=argparse.ArgumentParser();p.add_argument('--write',action='store_true');a=p.parse_args();text=render()
    if a.write:OUT.write_text(text)
    assert OUT.read_text()==text,'pending U128 inventory is stale'
    print('17 affected profile extensions + arithmetic-lowering source inventory current; Verus PENDING')
