//! Finite-data Boolean junction construction with exact AST contracts.
//! These contracts are construction guarantees; query refinement is separate.
use super::Term;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;
#[cfg(verus_keep_ghost)]
use vstd::std_specs::iter::IteratorSpec;
#[cfg(verus_keep_ghost)]
verus! {
pub(crate) open spec fn neutral(item: Term, conjunction: bool) -> bool {
    match item { Term::Bool(b) => b == conjunction, _ => false }
}
pub(crate) open spec fn absorber(item: Term, conjunction: bool) -> bool {
    match item { Term::Bool(b) => b != conjunction, _ => false }
}
#[verifier::opaque]
pub(crate) open spec fn retained(items: Seq<Term>, count: nat, conjunction: bool) -> Seq<Term>
    recommends count <= items.len(),
    decreases count,
{
    if count == 0 || count > items.len() { Seq::empty() }
    else {
        let previous = retained(items, (count - 1) as nat, conjunction);
        let item = items[count as int - 1];
        if neutral(item, conjunction) { previous } else { previous.push(item) }
    }
}
pub(crate) closed spec fn folded(items: Seq<Term>, conjunction: bool, output: Term) -> bool {
    if exists|i: int| 0 <= i < items.len() && absorber(#[trigger] items[i], conjunction) {
        match output { Term::Bool(b) => b != conjunction, _ => false }
    } else {
        let kept = retained(items, items.len(), conjunction);
        if kept.len() == 0 { match output { Term::Bool(b) => b == conjunction, _ => false } }
        else if kept.len() == 1 { spec_eq(output, kept[0]) }
        else if conjunction { match output { Term::And(v) => v@ == kept, _ => false } }
        else { match output { Term::Or(v) => v@ == kept, _ => false } }
    }
}
pub(crate) proof fn retain_step(items: Seq<Term>, count: nat, conjunction: bool)
    requires count < items.len(),
    ensures retained(items, count + 1, conjunction) ==
        if neutral(items[count as int], conjunction) { retained(items, count, conjunction) }
        else { retained(items, count, conjunction).push(items[count as int]) },
{
    reveal_with_fuel(retained, 2);
}

pub(crate) open spec fn booleans(items: Seq<Term>, environment: Map<Seq<char>, super::denotation::Value>) -> bool {
    forall|i: int| 0 <= i < items.len() ==>
        match super::denotation::term(#[trigger] items[i], environment) {
            Some(super::denotation::Value::Boolean(_)) => true, _ => false,
        }
}

pub(crate) proof fn junction_prefix(a: Seq<Term>, b: Seq<Term>, count: nat,
    environment: Map<Seq<char>, super::denotation::Value>, conjunction: bool)
    requires count <= a.len(), count <= b.len(),
        forall|i: int| 0 <= i < count ==> #[trigger] a[i] == b[i],
    ensures super::denotation::junction(a, count, environment, conjunction) ==
        super::denotation::junction(b, count, environment, conjunction),
    decreases count,
{
    reveal_with_fuel(super::denotation::junction, 2);
    reveal_with_fuel(super::denotation::term, 2);
    if count > 0 {
        assert(a[count as int - 1] == b[count as int - 1]);
        junction_prefix(a, b, (count - 1) as nat, environment, conjunction);
    }
}

pub(crate) proof fn junction_push(items: Seq<Term>, item: Term,
    environment: Map<Seq<char>, super::denotation::Value>, conjunction: bool)
    ensures super::denotation::junction(items.push(item), items.len() + 1, environment, conjunction) ==
        match (super::denotation::junction(items, items.len(), environment, conjunction),
            super::denotation::term(item, environment)) {
            (Some(super::denotation::Value::Boolean(a)), Some(super::denotation::Value::Boolean(b))) =>
                Some(super::denotation::Value::Boolean(if conjunction { a && b } else { a || b })),
            _ => None,
        },
{
    broadcast use vstd::seq::group_seq_lemmas;
    reveal_with_fuel(super::denotation::junction, 2);
    reveal_with_fuel(super::denotation::term, 2);
    junction_prefix(items, items.push(item), items.len(), environment, conjunction);
}

pub(crate) proof fn boolean_prefix(items: Seq<Term>, count: nat,
    environment: Map<Seq<char>, super::denotation::Value>, conjunction: bool)
    requires count <= items.len(), booleans(items, environment),
    ensures match super::denotation::junction(items, count, environment, conjunction) {
        Some(super::denotation::Value::Boolean(_)) => true, _ => false,
    },
    decreases count,
{
    reveal_with_fuel(super::denotation::junction, 2);
    reveal_with_fuel(super::denotation::term, 2);
    if count > 0 {
        boolean_prefix(items, (count - 1) as nat, environment, conjunction);
    }
}

pub(crate) proof fn absorbed_meaning(items: Seq<Term>, count: nat, index: int,
    environment: Map<Seq<char>, super::denotation::Value>, conjunction: bool)
    requires count <= items.len(), 0 <= index < count,
        booleans(items, environment), absorber(items[index], conjunction),
    ensures super::denotation::junction(items, count, environment, conjunction) ==
        Some(super::denotation::Value::Boolean(!conjunction)),
    decreases count,
{
    reveal_with_fuel(super::denotation::junction, 2);
    reveal_with_fuel(super::denotation::term, 2);
    if index == count as int - 1 {
        boolean_prefix(items, (count - 1) as nat, environment, conjunction);
    } else {
        absorbed_meaning(items, (count - 1) as nat, index, environment, conjunction);
    }
}

pub(crate) proof fn retained_meaning(items: Seq<Term>, count: nat,
    environment: Map<Seq<char>, super::denotation::Value>, conjunction: bool)
    requires count <= items.len(), booleans(items, environment),
    ensures super::denotation::junction(retained(items, count, conjunction),
        retained(items, count, conjunction).len(), environment, conjunction) ==
        super::denotation::junction(items, count, environment, conjunction),
    decreases count,
{
    reveal_with_fuel(retained, 2);
    reveal_with_fuel(super::denotation::junction, 2);
    reveal_with_fuel(super::denotation::term, 2);
    if count > 0 {
        let previous = retained(items, (count - 1) as nat, conjunction);
        let item = items[count as int - 1];
        retained_meaning(items, (count - 1) as nat, environment, conjunction);
        boolean_prefix(items, (count - 1) as nat, environment, conjunction);
        if !neutral(item, conjunction) {
            junction_push(previous, item, environment, conjunction);
        }
    }
}

/// The actual finite constructor's shape contract preserves Boolean meaning.
/// Every supplied term must be well-defined Boolean in this environment.
pub(crate) proof fn folded_meaning(items: Seq<Term>, conjunction: bool, output: Term,
    environment: Map<Seq<char>, super::denotation::Value>)
    requires folded(items, conjunction, output), booleans(items, environment),
    ensures super::denotation::term(output, environment) ==
        super::denotation::junction(items, items.len(), environment, conjunction),
{
    reveal(folded);
    reveal_with_fuel(super::denotation::term, 2);
    reveal_with_fuel(super::denotation::junction, 2);
    if exists|i: int| 0 <= i < items.len() && absorber(#[trigger] items[i], conjunction) {
        let i = choose|i: int| 0 <= i < items.len() && absorber(#[trigger] items[i], conjunction);
        absorbed_meaning(items, items.len(), i, environment, conjunction);
    } else {
        let kept = retained(items, items.len(), conjunction);
        retained_meaning(items, items.len(), environment, conjunction);
        boolean_prefix(items, items.len(), environment, conjunction);
        if kept.len() == 1 {
            broadcast use vstd::seq::group_seq_lemmas;
            assert(kept =~= seq![kept[0]]);
            junction_push(Seq::empty(), kept[0], environment, conjunction);
        }
    }
}

}

#[cfg_attr(verus_keep_ghost, verus_verify)]
impl Term {
    /// Conjunction of a finite list, preserving order and nested singleton terms.
    pub(crate) fn and(items: Vec<Self>) -> Self {
        #[cfg(verus_keep_ghost)]
        #[cfg_attr(verus_keep_ghost, cfg(verus_keep_ghost_body))]
        verus_proof_expr!(ensures(|output: Term| folded(items@, true, output) &&
            forall|environment: Map<Seq<char>, super::denotation::Value>|
                booleans(items@, environment) ==> super::denotation::term(output, environment) ==
                    super::denotation::junction(items@, items@.len(), environment, true)));
        #[cfg(verus_keep_ghost)]
        proof! { reveal(folded); }
        #[cfg(verus_keep_ghost)]
        let original = verus_exec_expr!(Ghost(items@));
        let output = junction(items, true);
        #[cfg(verus_keep_ghost)]
        proof! {
            assert forall|environment: Map<Seq<char>, super::denotation::Value>|
                booleans(original@, environment) implies
                    super::denotation::term(output, environment) ==
                    super::denotation::junction(original@, original@.len(), environment, true) by {
                folded_meaning(original@, true, output, environment);
            }
        }
        output
    }
    /// Disjunction of a finite list, preserving order and nested singleton terms.
    pub(crate) fn or(items: Vec<Self>) -> Self {
        #[cfg(verus_keep_ghost)]
        #[cfg_attr(verus_keep_ghost, cfg(verus_keep_ghost_body))]
        verus_proof_expr!(ensures(|output: Term| folded(items@, false, output) &&
            forall|environment: Map<Seq<char>, super::denotation::Value>|
                booleans(items@, environment) ==> super::denotation::term(output, environment) ==
                    super::denotation::junction(items@, items@.len(), environment, false)));
        #[cfg(verus_keep_ghost)]
        proof! { reveal(folded); }
        #[cfg(verus_keep_ghost)]
        let original = verus_exec_expr!(Ghost(items@));
        let output = junction(items, false);
        #[cfg(verus_keep_ghost)]
        proof! {
            assert forall|environment: Map<Seq<char>, super::denotation::Value>|
                booleans(original@, environment) implies
                    super::denotation::term(output, environment) ==
                    super::denotation::junction(original@, original@.len(), environment, false) by {
                folded_meaning(original@, false, output, environment);
            }
        }
        output
    }
}

#[cfg_attr(verus_keep_ghost, verus_spec(output =>
    ensures folded(items@, conjunction, output),
))]
#[allow(
    clippy::while_let_loop,
    reason = "The explicit exit carries the verified finite-cursor exhaustion postcondition."
)]
fn junction(items: Vec<Term>, conjunction: bool) -> Term {
    #[cfg(verus_keep_ghost)]
    let original = verus_exec_expr!(Ghost(items@));
    #[cfg(verus_keep_ghost)]
    let mut done = verus_exec_expr!(Ghost(0int));
    let mut iter = items.into_iter();
    let mut kept = Vec::new();
    #[cfg(verus_keep_ghost)]
    proof! {
        reveal(folded);
        reveal_with_fuel(retained, 1);
    }
    #[cfg_attr(verus_keep_ghost, verus_spec(
        invariant original@ == items@, 0 <= done@ <= original@.len(),
            iter.obeys_prophetic_iter_laws(),
            iter.remaining() == original@.subrange(done@, original@.len() as int),
            kept@ == retained(original@, done@ as nat, conjunction),
            forall|i: int| 0 <= i < done@ ==> !absorber(#[trigger] original@[i], conjunction),
        ensures original@ == items@, done@ == original@.len(),
            kept@ == retained(original@, original@.len(), conjunction),
            forall|i: int| 0 <= i < original@.len() ==> !absorber(#[trigger] original@[i], conjunction),
        decreases original@.len() - done@,
    ))]
    loop {
        let item = match iter.next() {
            Some(item) => item,
            None => {
                #[cfg(verus_keep_ghost)]
                proof! {
                    broadcast use vstd::seq::group_seq_lemmas;
                    assert(done@ == original@.len());
                }
                break;
            }
        };
        #[cfg(verus_keep_ghost)]
        proof! {
            broadcast use vstd::seq::group_seq_lemmas;
            assert(done@ < original@.len());
            assert(spec_eq(item, original@[done@]));
            retain_step(original@, done@ as nat, conjunction);
        }
        match item {
            Term::Bool(b) if b == conjunction => {}
            Term::Bool(b) => {
                #[cfg(verus_keep_ghost)]
                proof! {
                    assert(absorber(original@[done@], conjunction));
                    assert(exists|i: int| 0 <= i < original@.len() && absorber(#[trigger] original@[i], conjunction));
                    assert(folded(original@, conjunction, Term::Bool(b)));
                }
                return Term::Bool(b);
            }
            item => kept.push(item),
        }
        #[cfg(verus_keep_ghost)]
        proof! {
            *done.borrow_mut() = done@ + 1;
        }
    }
    #[cfg(verus_keep_ghost)]
    proof! {
        assert(done@ == original@.len());
    }
    match kept.len() {
        0 => Term::Bool(conjunction),
        1 => kept.pop().unwrap_or(Term::Bool(conjunction)),
        _ => {
            if conjunction {
                Term::And(kept)
            } else {
                Term::Or(kept)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaf(name: &str) -> Term {
        Term::name(name)
    }

    #[test]
    fn conjunction_neutralizes_absorbs_and_preserves_kept_items() {
        // Empty and all-neutral inputs produce the literal neutral element.
        assert_eq!(Term::and(vec![]), Term::Bool(true));
        assert_eq!(
            Term::and(vec![Term::Bool(true), Term::Bool(true)]),
            Term::Bool(true)
        );
        // A sole kept item is returned unchanged, even a nested empty node.
        assert_eq!(Term::and(vec![leaf("x")]), leaf("x"));
        assert_eq!(Term::and(vec![Term::And(vec![])]), Term::And(vec![]));
        // Kept items stay in order with duplicates and nesting intact.
        assert_eq!(
            Term::and(vec![
                leaf("a"),
                Term::Bool(true),
                leaf("a"),
                Term::Or(vec![leaf("b")])
            ]),
            Term::And(vec![leaf("a"), leaf("a"), Term::Or(vec![leaf("b")])])
        );
        // An absorber after kept items collapses the conjunction.
        assert_eq!(
            Term::and(vec![leaf("a"), Term::Bool(false), leaf("b")]),
            Term::Bool(false)
        );
    }

    #[test]
    fn disjunction_neutralizes_absorbs_and_preserves_kept_items() {
        assert_eq!(Term::or(vec![]), Term::Bool(false));
        assert_eq!(
            Term::or(vec![Term::Bool(false), Term::Bool(false)]),
            Term::Bool(false)
        );
        assert_eq!(Term::or(vec![leaf("x")]), leaf("x"));
        assert_eq!(Term::or(vec![Term::Or(vec![])]), Term::Or(vec![]));
        assert_eq!(
            Term::or(vec![
                leaf("a"),
                Term::Bool(false),
                leaf("a"),
                Term::And(vec![leaf("b")])
            ]),
            Term::Or(vec![leaf("a"), leaf("a"), Term::And(vec![leaf("b")])])
        );
        assert_eq!(
            Term::or(vec![leaf("a"), Term::Bool(true), leaf("b")]),
            Term::Bool(true)
        );
    }

    #[test]
    fn finite_lists_absorb_at_any_position_and_pass_nested_singletons_through() {
        // The absorber decides the result wherever it sits in the list.
        for items in [
            vec![Term::Bool(false), leaf("a"), leaf("b")],
            vec![leaf("a"), Term::Bool(false), leaf("b")],
            vec![leaf("a"), leaf("b"), Term::Bool(false)],
        ] {
            assert_eq!(Term::and(items), Term::Bool(false));
        }
        for items in [
            vec![Term::Bool(true), leaf("a"), leaf("b")],
            vec![leaf("a"), Term::Bool(true), leaf("b")],
            vec![leaf("a"), leaf("b"), Term::Bool(true)],
        ] {
            assert_eq!(Term::or(items), Term::Bool(true));
        }
        // Items past the absorber change nothing: these equal the lists above
        // truncated at the absorber.
        assert_eq!(
            Term::and(vec![
                leaf("a"),
                Term::Bool(false),
                leaf("dropped"),
                leaf("dropped")
            ]),
            Term::Bool(false)
        );
        assert_eq!(
            Term::or(vec![
                leaf("a"),
                Term::Bool(true),
                leaf("dropped"),
                leaf("dropped")
            ]),
            Term::Bool(true)
        );
        // A lone nested empty junction passes through unchanged.
        assert_eq!(Term::and(vec![Term::Or(vec![])]), Term::Or(vec![]));
        assert_eq!(Term::or(vec![Term::And(vec![])]), Term::And(vec![]));
        assert_eq!(Term::and(vec![Term::And(vec![])]), Term::And(vec![]));
        assert_eq!(Term::or(vec![Term::Or(vec![])]), Term::Or(vec![]));
    }
}
