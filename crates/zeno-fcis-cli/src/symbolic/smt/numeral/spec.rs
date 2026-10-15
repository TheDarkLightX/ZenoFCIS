//! Erased specification of decimal digits and SMT signed numeral syntax.

use vstd::prelude::*;

verus! {

/// Connects the string-view and character-view forms of punctuation.
pub(crate) proof fn punctuation()
    ensures "("@ == seq!['('], ")"@ == seq![')'],
        " "@ == seq![' '], "\n"@ == seq!['\n'],
{
    reveal_strlit("(");
    reveal_strlit(")");
    reveal_strlit(" ");
    reveal_strlit("\n");
    assert("("@ =~= seq!['(']);
    assert(")"@ =~= seq![')']);
    assert(" "@ =~= seq![' ']);
    assert("\n"@ =~= seq!['\n']);
}

pub(crate) open spec fn decimal(value: nat) -> Seq<char>
    decreases value,
{
    let digit = ('0' as u32 + value % 10) as char;
    if value < 10 { seq![digit] }
    else { decimal(value / 10).push(digit) }
}

pub(crate) open spec fn signed(value: int) -> Seq<char> {
    if value < 0 { seq!['(', '-', ' '] + decimal((-value) as nat) + seq![')'] }
    else { decimal(value as nat) }
}

pub(crate) open spec fn digits(text: Seq<char>) -> bool {
    text.len() > 0
        && forall|i: int| 0 <= i < text.len() ==> '0' <= #[trigger] text[i] <= '9'
}

pub(crate) open spec fn value(text: Seq<char>) -> nat
    decreases text.len(),
{
    if text.len() == 0 { 0 }
    else { 10 * value(text.drop_last()) + (text.last() as u32 - '0' as u32) as nat }
}

pub(crate) proof fn decimal_meaning(number: nat)
    ensures
        digits(decimal(number)),
        value(decimal(number)) == number,
        number > 0 ==> decimal(number)[0] != '0',
    decreases number,
{
    broadcast use {
        vstd::arithmetic::div_mod::lemma_fundamental_div_mod,
        vstd::arithmetic::div_mod::lemma_div_decreases,
        vstd::arithmetic::div_mod::group_mod_basics,
    };
    reveal_with_fuel(value, 2);
    if number >= 10 {
        decimal_meaning(number / 10);
        assert(decimal(number).drop_last() == decimal(number / 10));
        assert(decimal(number).last() as u32 == '0' as u32 + number % 10);
        vstd::arithmetic::div_mod::lemma_fundamental_div_mod(number as int, 10);
        assert(value(decimal(number)) == 10 * (number / 10) + number % 10);
        assert forall|i: int| 0 <= i < decimal(number).len()
            implies '0' <= #[trigger] decimal(number)[i] <= '9' by {
            if i < decimal(number / 10).len() {
                assert(decimal(number)[i] == decimal(number / 10)[i]);
            }
        }
        if number / 10 < 10 {
            assert(1 <= number / 10 < 10);
        }
    } else {
        vstd::arithmetic::div_mod::lemma_small_mod(number, 10);
        assert(decimal(number).drop_last() == Seq::<char>::empty());
        assert(decimal(number).last() as u32 == '0' as u32 + number);
        assert(value(decimal(number)) == number);
    }
}

}
