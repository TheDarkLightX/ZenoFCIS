//! Erased syntax specification for the complete production SMT query printer.
//! This does not establish identifier admission or runtime-to-query denotation.

use super::super::term::{Sort, Term, denotation, numeral, printing};
use super::{Script, Values};
use vstd::prelude::*;

pub(crate) use super::prepare::empty_fields;

verus! {

pub(crate) open spec fn member(name: Seq<char>, value: i128) -> Seq<char> {
    "(= "@ + name + " "@ + numeral::spec::signed(value as int) + ")"@
}

pub(crate) open spec fn members(items: Seq<i128>, name: Seq<char>, count: nat) -> Seq<char>
    recommends count <= items.len(),
    decreases count,
{
    if count == 0 || count > items.len() { Seq::empty() }
    else { members(items, name, (count - 1) as nat) + " "@ + member(name, items[count as int - 1]) }
}

pub(crate) proof fn members_step(items: Seq<i128>, name: Seq<char>, count: nat)
    requires count < items.len(),
    ensures members(items, name, count + 1) == members(items, name, count) + " "@ + member(name, items[count as int]),
{}

pub(crate) open spec fn values(set: Values, name: Seq<char>) -> Seq<char> {
    match set {
        Values::Range { min, max } => "(and (<= "@ + numeral::spec::signed(min as int) + " "@
            + name + ") (<= "@ + name + " "@ + numeral::spec::signed(max as int) + "))"@,
        Values::Set(items) => if items@.len() == 0 { "false"@ }
            else if items@.len() == 1 { member(name, items@[0]) }
            else { "(or"@ + members(items@, name, items@.len()) + ")"@ },
    }
}

pub(crate) open spec fn comments_item(item: String) -> Seq<char> {
    "; "@ + item@ + "\n"@
}

#[verifier::opaque]
pub(crate) open spec fn comments(items: Seq<String>, count: nat) -> Seq<char>
    recommends count <= items.len(),
    decreases count,
{
    if count == 0 || count > items.len() { Seq::empty() }
    else { comments(items, (count - 1) as nat) + comments_item(items[count as int - 1]) }
}

pub(crate) proof fn comments_step(items: Seq<String>, count: nat)
    requires count < items.len(),
    ensures comments(items, count + 1) == comments(items, count) + comments_item(items[count as int]),
{
    reveal_with_fuel(comments, 2);
}

pub(crate) open spec fn constants_item(item: (String, Values)) -> Seq<char> {
    "(declare-const "@ + item.0@ + " Int)\n(assert "@ + values(item.1, item.0@) + ")\n"@
}

#[verifier::opaque]
pub(crate) open spec fn constants(items: Seq<(String, Values)>, count: nat) -> Seq<char>
    recommends count <= items.len(),
    decreases count,
{
    if count == 0 || count > items.len() { Seq::empty() }
    else { constants(items, (count - 1) as nat) + constants_item(items[count as int - 1]) }
}

pub(crate) proof fn constants_step(items: Seq<(String, Values)>, count: nat)
    requires count < items.len(),
    ensures constants(items, count + 1) == constants(items, count) + constants_item(items[count as int]),
{
    reveal_with_fuel(constants, 2);
}

pub(crate) open spec fn declarations_item(item: (String, Term)) -> Seq<char> {
    "(declare-const "@ + item.0@ + " Int)\n"@
}

#[verifier::opaque]
pub(crate) open spec fn declarations(items: Seq<(String, Term)>, count: nat) -> Seq<char>
    recommends count <= items.len(),
    decreases count,
{
    if count == 0 || count > items.len() { Seq::empty() }
    else { declarations(items, (count - 1) as nat) + declarations_item(items[count as int - 1]) }
}

pub(crate) proof fn declarations_step(items: Seq<(String, Term)>, count: nat)
    requires count < items.len(),
    ensures declarations(items, count + 1) == declarations(items, count) + declarations_item(items[count as int]),
{
    reveal_with_fuel(declarations, 2);
}

pub(crate) open spec fn definitions_item(item: (String, Sort, Term)) -> Seq<char> {
    "(define-fun "@ + item.0@ + " () "@ + (if item.1 is Int { "Int"@ } else { "Bool"@ }) + " "@ + printing::text(item.2) + ")\n"@
}

#[verifier::opaque]
pub(crate) open spec fn definitions(items: Seq<(String, Sort, Term)>, count: nat) -> Seq<char>
    recommends count <= items.len(),
    decreases count,
{
    if count == 0 || count > items.len() { Seq::empty() }
    else { definitions(items, (count - 1) as nat) + definitions_item(items[count as int - 1]) }
}

pub(crate) proof fn definitions_step(items: Seq<(String, Sort, Term)>, count: nat)
    requires count < items.len(),
    ensures definitions(items, count + 1) == definitions(items, count) + definitions_item(items[count as int]),
{
    reveal_with_fuel(definitions, 2);
}

pub(crate) open spec fn observations_item(item: (String, Term)) -> Seq<char> {
    "(assert (= "@ + item.0@ + " "@ + printing::text(item.1) + "))\n"@
}

#[verifier::opaque]
pub(crate) open spec fn observations(items: Seq<(String, Term)>, count: nat) -> Seq<char>
    recommends count <= items.len(),
    decreases count,
{
    if count == 0 || count > items.len() { Seq::empty() }
    else { observations(items, (count - 1) as nat) + observations_item(items[count as int - 1]) }
}

pub(crate) proof fn observations_step(items: Seq<(String, Term)>, count: nat)
    requires count < items.len(),
    ensures observations(items, count + 1) == observations(items, count) + observations_item(items[count as int]),
{
    reveal_with_fuel(observations, 2);
}

pub(crate) open spec fn assertions_item(item: Term) -> Seq<char> {
    "(assert "@ + printing::text(item) + ")\n"@
}

#[verifier::opaque]
pub(crate) open spec fn assertions(items: Seq<Term>, count: nat) -> Seq<char>
    recommends count <= items.len(),
    decreases count,
{
    if count == 0 || count > items.len() { Seq::empty() }
    else { assertions(items, (count - 1) as nat) + assertions_item(items[count as int - 1]) }
}

pub(crate) proof fn assertions_step(items: Seq<Term>, count: nat)
    requires count < items.len(),
    ensures assertions(items, count + 1) == assertions(items, count) + assertions_item(items[count as int]),
{
    reveal_with_fuel(assertions, 2);
}

pub(crate) open spec fn source(script: Script) -> Seq<char> {
    comments(script.comments@, script.comments@.len())
        + "(set-logic ALL)\n(set-option :produce-models true)\n"@
        + constants(script.constants@, script.constants@.len())
        + declarations(script.observations@, script.observations@.len())
        + definitions(script.definitions@, script.definitions@.len())
        + observations(script.observations@, script.observations@.len())
        + assertions(script.assertions@, script.assertions@.len())
        + "(check-sat)\n"@
}

pub(crate) proof fn empty_prefixes(script: Script)
    ensures
        comments(script.comments@, 0) == Seq::<char>::empty(),
        constants(script.constants@, 0) == Seq::<char>::empty(),
        declarations(script.observations@, 0) == Seq::<char>::empty(),
        definitions(script.definitions@, 0) == Seq::<char>::empty(),
        observations(script.observations@, 0) == Seq::<char>::empty(),
        assertions(script.assertions@, 0) == Seq::<char>::empty(),
{
    reveal(comments);
    reveal(constants);
    reveal(declarations);
    reveal(definitions);
    reveal(observations);
    reveal(assertions);
}

/// Equations for an actual appended definition window in one fixed environment.
/// This predicate does not construct an environment or evaluate a Script.
pub(crate) open spec fn definitions_hold(
    definitions: Seq<(String, Sort, Term)>, start: nat, count: nat,
    environment: Map<Seq<char>, denotation::Value>,
) -> bool {
    start + count <= definitions.len()
        && forall|j: int| start <= j < start + count ==>
            environment.contains_key(definitions[j].0@)
            && denotation::term(definitions[j].2, environment)
                == Some(environment[definitions[j].0@])
}

/// Script.define's returned Name has its recorded body's value when the
/// corresponding actual equation holds. No new native precondition is added.
pub(crate) proof fn definition_meaning(
    definitions: Seq<(String, Sort, Term)>, start: nat, count: nat, offset: nat,
    wire: Term, body: Term, environment: Map<Seq<char>, denotation::Value>,
)
    requires definitions_hold(definitions, start, count, environment), offset < count,
        definitions[start as int + offset as int].2 == body,
        match wire {
            Term::Name(name) => name@ == definitions[start as int + offset as int].0@,
            _ => false,
        },
    ensures denotation::term(wire, environment) == denotation::term(body, environment),
{
    assert(environment.contains_key(definitions[start as int + offset as int].0@));
    assert(denotation::term(body, environment)
        == Some(environment[definitions[start as int + offset as int].0@]));
    reveal_with_fuel(denotation::term, 1);
}


}
