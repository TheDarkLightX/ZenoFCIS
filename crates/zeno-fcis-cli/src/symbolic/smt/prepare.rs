//! Production script initialization and ordered declaration/assertion updates.

use super::super::term::{Sort, Term};
use super::{Script, Values};
#[cfg(verus_keep_ghost)]
use vstd::{prelude::*, std_specs::iter::IteratorSpec};

#[cfg(verus_keep_ghost)]
verus! {
    impl View for Values {
        type V = Values;
        #[verifier::opaque]
        open spec fn view(&self) -> Values { *self }
    }

    pub closed spec fn empty(script: Script) -> bool {
        script.comments@.len() == 0 && script.constants@.len() == 0
        && script.definitions@.len() == 0 && script.observations@.len() == 0
        && script.assertions@.len() == 0
    }

    /// Expose the actual initializer's field facts from their defining owner.
    pub(crate) proof fn empty_fields(script: Script)
        requires empty(script),
        ensures script.comments@.len() == 0, script.constants@.len() == 0,
            script.definitions@.len() == 0, script.observations@.len() == 0,
            script.assertions@.len() == 0,
    {
        reveal(empty);
    }
}

// Keep the actual initializer body as the proof subject, rather than a derive.
#[allow(
    clippy::derivable_impls,
    reason = "The production initializer body has a directly checked exact contract."
)]
#[cfg_attr(verus_keep_ghost, verus_verify)]
impl Default for Script {
    fn default() -> Script {
        #[cfg(verus_keep_ghost)]
        verus_proof_expr!(ensures(|result: Script| empty(result)));
        #[cfg(verus_keep_ghost)]
        proof! { reveal(empty); }
        Script {
            comments: Vec::new(),
            constants: Vec::new(),
            definitions: Vec::new(),
            observations: Vec::new(),
            assertions: Vec::new(),
        }
    }
}

impl Script {
    /// A comment line at the top of the query.
    #[cfg_attr(verus_keep_ghost, verus_spec(
        ensures final(self).constants@ == old(self).constants@,
            final(self).definitions@ == old(self).definitions@,
            final(self).observations@ == old(self).observations@,
            final(self).assertions@ == old(self).assertions@,
    ))]
    pub(crate) fn comment(&mut self, text: &str) {
        let mut chars = text.chars();
        let mut line = String::new();
        let mut trailing_cr = false;
        #[cfg_attr(verus_keep_ghost, verus_spec(
            invariant chars.decrease().is_some(),
                self.constants@ == old(self).constants@,
                self.definitions@ == old(self).definitions@,
                self.observations@ == old(self).observations@,
                self.assertions@ == old(self).assertions@,
            decreases chars.decrease().unwrap(),
        ))]
        loop {
            match chars.next() {
                Some('\n') => {
                    if trailing_cr {
                        let _ = line.pop();
                    }
                    self.comments.push(line);
                    line = String::new();
                    trailing_cr = false;
                }
                Some(c) => {
                    line.push(c);
                    trailing_cr = c == '\r';
                }
                None => break,
            }
        }
        if !line.is_empty() {
            self.comments.push(line);
        }
    }

    /// Declares an integer constant restricted to `values`.
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures
            final(self).constants@.len() == old(self).constants@.len() + 1,
            final(self).constants@.subrange(0, old(self).constants@.len() as int) == old(self).constants@,
            final(self).constants@.last().0@ == name@,
            spec_eq(final(self).constants@.last().1, values@),
            final(self).comments@ == old(self).comments@,
            final(self).definitions@ == old(self).definitions@,
            final(self).observations@ == old(self).observations@,
            final(self).assertions@ == old(self).assertions@,
            match result { Term::Name(n) => spec_eq(n@, name@), _ => false },
    ))]
    pub(crate) fn constant(&mut self, name: &str, values: Values) -> Term {
        #[cfg(verus_keep_ghost)]
        proof! {
            reveal(<Values as View>::view);
            broadcast use vstd::seq::group_seq_lemmas;
        }
        let mut owned = String::new();
        owned.push_str(name);
        self.constants.push((owned, values));
        Term::name(name)
    }

    /// Defines `name` as `term` and returns a reference to it.
    #[cfg_attr(verus_keep_ghost, verus_spec(result =>
        ensures
            final(self).definitions@.len() == old(self).definitions@.len() + 1,
            final(self).definitions@.subrange(0, old(self).definitions@.len() as int) == old(self).definitions@,
            final(self).definitions@.last().0@ == name@,
            final(self).definitions@.last().1 == sort,
            spec_eq(final(self).definitions@.last().2, term@),
            final(self).comments@ == old(self).comments@,
            final(self).constants@ == old(self).constants@,
            final(self).observations@ == old(self).observations@,
            final(self).assertions@ == old(self).assertions@,
            match result { Term::Name(n) => spec_eq(n@, name@), _ => false },
    ))]
    pub(crate) fn define(&mut self, name: &str, sort: Sort, term: Term) -> Term {
        #[cfg(verus_keep_ghost)]
        proof! {
            reveal(<Term as View>::view);
            broadcast use vstd::seq::group_seq_lemmas;
        }
        let mut owned = String::new();
        owned.push_str(name);
        self.definitions.push((owned, sort, term));
        Term::name(name)
    }

    #[cfg_attr(verus_keep_ghost, verus_spec(
        ensures
            spec_eq(final(self).assertions@, old(self).assertions@.push(term@)),
            final(self).comments@ == old(self).comments@,
            final(self).constants@ == old(self).constants@,
            final(self).definitions@ == old(self).definitions@,
            final(self).observations@ == old(self).observations@,
    ))]
    pub(crate) fn assert(&mut self, term: Term) {
        #[cfg(verus_keep_ghost)]
        proof! { reveal(<Term as View>::view); }
        self.assertions.push(term);
    }

    /// Declares an integer observation equal to `term` for model replay.
    #[cfg_attr(verus_keep_ghost, verus_spec(
        ensures
            final(self).observations@.len() == old(self).observations@.len() + 1,
            final(self).observations@.subrange(0, old(self).observations@.len() as int) == old(self).observations@,
            final(self).observations@.last().0@ == name@,
            spec_eq(final(self).observations@.last().1, term@),
            final(self).comments@ == old(self).comments@,
            final(self).constants@ == old(self).constants@,
            final(self).definitions@ == old(self).definitions@,
            final(self).assertions@ == old(self).assertions@,
    ))]
    pub(crate) fn observe(&mut self, name: &str, term: Term) {
        #[cfg(verus_keep_ghost)]
        proof! {
            reveal(<Term as View>::view);
            broadcast use vstd::seq::group_seq_lemmas;
        }
        let mut owned = String::new();
        owned.push_str(name);
        self.observations.push((owned, term));
    }
}
