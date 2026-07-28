from pathlib import Path


def replace_exact(path: Path, old: str, new: str, label: str) -> None:
    text = path.read_text(encoding="utf-8")
    if old not in text:
        if new in text:
            return
        raise SystemExit(f"missing composition-v2 hardening site: {label}")
    path.write_text(text.replace(old, new), encoding="utf-8")


compose = Path("crates/zeno-fcis-compose/src/lib.rs")
replace_exact(
    compose,
    "if coupling_claims.iter().any(|claim| *claim == Hash32::ZERO) {",
    "if coupling_claims.contains(&Hash32::ZERO) {",
    "coupling claim zero check",
)
replace_exact(
    compose,
    "context: ParallelVerificationContext,",
    "context: Box<ParallelVerificationContext>,",
    "boxed parity claim context",
)
replace_exact(
    compose,
    "context: parity.context().clone(),",
    "context: Box::new(parity.context().clone()),",
    "runtime parity statement context",
)
replace_exact(
    compose,
    "context: expected.clone(),",
    "context: Box::new(expected.clone()),",
    "test expected parity context",
)
replace_exact(
    compose,
    "context: mutated.clone(),",
    "context: Box::new(mutated.clone()),",
    "test mutated parity context",
)
replace_exact(
    compose,
    "blockers: vec![CompositionBlocker::CompositionIdentityFailure].into_boxed_slice(),",
    "blockers: Vec::from([CompositionBlocker::CompositionIdentityFailure]).into_boxed_slice(),",
    "no-std identity blocker",
)
