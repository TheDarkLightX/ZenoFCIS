//! Generated names built with the checked decimal numeral writer.
//! The result is the exact prefix concatenated with the decimal index; it is
//! not an identifier-admission judgment for arbitrary prefixes.

use super::numeral;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

/// `prefix` followed by `index` in decimal, byte-for-byte.
#[cfg_attr(verus_keep_ghost, verus_spec(result =>
    ensures result@ == prefix@ + numeral::spec::decimal(index as nat),
))]
pub(crate) fn indexed_name(prefix: &str, index: usize) -> String {
    let mut name = String::new();
    name.push_str(prefix);
    numeral::append_unsigned(&mut name, index as u128);
    name
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indexed_names_match_rust_decimal_formatting() {
        for (prefix, index) in [
            ("", 0),
            ("x", 0),
            ("x", 9),
            ("x", 10),
            ("p", 99),
            ("q", 100),
            ("x", usize::MAX),
            ("输出", 7),
        ] {
            assert_eq!(indexed_name(prefix, index), format!("{prefix}{index}"));
        }
    }
}

#[cfg(verus_keep_ghost)]
verus! {

/// The exact generated-name language used by the scalar query builder.
pub(crate) open spec fn generated(prefix: Seq<char>, index: nat) -> Seq<char> {
    prefix + numeral::spec::decimal(index)
}
pub(crate) open spec fn scalar_identifier(name: Seq<char>) -> bool {
    name.len() > 1 && (name[0] == 'x' || name[0] == 'p' || name[0] == 'q')
        && forall|i: int| 1 <= i < name.len() ==> '0' <= #[trigger] name[i] <= '9'
}

pub(crate) proof fn identifier(prefix: char, index: nat)
    requires prefix == 'x' || prefix == 'p' || prefix == 'q',
    ensures scalar_identifier(generated(seq![prefix], index)),
        generated(seq![prefix], index)[0] == prefix,
{
    broadcast use vstd::seq::group_seq_lemmas;
    numeral::spec::decimal_meaning(index);
    assert forall|i: int| 1 <= i < generated(seq![prefix], index).len()
        implies '0' <= #[trigger] generated(seq![prefix], index)[i] <= '9' by {
        assert(generated(seq![prefix], index)[i] == numeral::spec::decimal(index)[i - 1]);
    }
}

pub(crate) proof fn fresh_index(prefix: Seq<char>, left: nat, right: nat)
    requires left != right,
    ensures generated(prefix, left) != generated(prefix, right),
{
    broadcast use vstd::seq::group_seq_lemmas;
    numeral::spec::decimal_meaning(left);
    numeral::spec::decimal_meaning(right);
    if generated(prefix, left) == generated(prefix, right) {
        assert(generated(prefix, left).subrange(prefix.len() as int, generated(prefix, left).len() as int)
            == numeral::spec::decimal(left));
        assert(generated(prefix, right).subrange(prefix.len() as int, generated(prefix, right).len() as int)
            == numeral::spec::decimal(right));
        assert(numeral::spec::decimal(left) == numeral::spec::decimal(right));
        assert(left == right);
    }
}

pub(crate) proof fn disjoint_namespaces(left: char, right: char, i: nat, j: nat)
    requires left == 'x' || left == 'p' || left == 'q',
        right == 'x' || right == 'p' || right == 'q', left != right,
    ensures generated(seq![left], i) != generated(seq![right], j),
{
    identifier(left, i);
    identifier(right, j);
}

pub(crate) proof fn avoids_observation_names(prefix: char, index: nat)
    requires prefix == 'x' || prefix == 'p' || prefix == 'q',
    ensures generated(seq![prefix], index) != "original_first"@,
        generated(seq![prefix], index) != "candidate_first"@,
{
    identifier(prefix, index);
    reveal_strlit("original_first");
    reveal_strlit("candidate_first");
    assert("original_first"@[0] == 'o');
    assert("candidate_first"@[0] == 'c');
}
}
