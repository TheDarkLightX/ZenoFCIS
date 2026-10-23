# Library loans

"A library loans app: members borrow and return copies of a book, a member
holds at most two copies, and no copy is lent twice."

## The journey

The librarian records loans at the desk. Member 1 borrows copy 1 and then
copy 2. A third copy for member 1 is refused: a member holds at most two.
Copy 1 for member 2 is refused too, because it is on loan. Member 1 returns
copy 1, and member 2 borrows it. The page shows each copy as "on the shelf"
or "with member N", and a refusal names its reason.

## The contract

This directory is a contract: `project.zeno`, `v2/policy.json`, `ui.json`
and `tests/decision-examples.txt`.

- The state records the holder of each of three copies: 0 is on the shelf,
  1 to 3 is a member. One holder per copy means a copy cannot be lent twice.
- A command is Borrow or Return, with a copy number and a member number. The
  context is the desk: Librarian or Visitor.
- The rules, first match decides: only the librarian desk acts (reason 200);
  a copy on loan cannot be borrowed (201); a member holding two copies cannot
  borrow (202); only the holder returns a copy (203); otherwise the borrow or
  return is accepted.
- The laws: no member holds all three copies (a state invariant), only the
  librarian commits, the other copies keep their holders, and a borrow moves
  a copy from the shelf to the named member while a return moves it back.
  Laws 504–507 also require each rejection reason's exact guard and its
  precedence. Together they require every eligible borrow and return to accept.
- This contract declares no delivery channels. Borrow, Return and every
  rejection emit no messages; a policy cannot introduce an undeclared channel.
- `ui.json` names the holder values, so a state line reads "Copy 1: with
  member 2." rather than "Copy 1: 2".

## Check and build

From the ZenoFCIS source checkout, with the CLI built:

```sh
target/debug/zeno-fcis check examples/apps/library-loans/project.zeno \
  --require-substantive --require-resolved-paths
target/debug/zeno-fcis contract review examples/apps/library-loans
target/debug/zeno-fcis contract lemma examples/apps/library-loans \
  --lemma 'post.100.110 == 0 || post.100.110 != post.100.111 || post.100.110 != post.100.112'
target/debug/zeno-fcis new library-loans --contract examples/apps/library-loans --app
```

The review enumerates all 2,304 inputs: the 12 examples agree with the
contract, no law refuses a decision from a lawful state, and 78 of 84 rule
mutants are distinguished. The lemma judge finds the at-most-two invariant
inductive over the declared domain, with 61 of 64 states reachable. Then run
the application as [Build an app](../../../docs/BUILD_AN_APP.md) describes.

The complete gap review with laws 504–507 determines all 2,304 input tuples,
including the exact rejection reasons, state changes and empty outboxes:

```sh
zeno-fcis contract review examples/apps/library-loans --gaps --format json
```

## Bounded recovery

From every lawful holder assignment, an available librarian can return all
copies in at most three `Return` actions. Choose any occupied copy and name
its recorded holder. A successful return clears that copy and preserves the
other two, so the number of occupied copies decreases by exactly one.
An empty shelf needs zero actions. This is a sequence the librarian can choose
without intervening borrows, not a promise that a scheduler will run it.

The transition lemma checks that each accepted return clears one occupied
copy and preserves the others over all 2,304 input tuples. The constructive
recovery check replays these sequences through the actual Authority for all
61 lawful assignments; the remaining three assignments violate law 500.

## Policy for the owner

- The desk continues to record loans for members; member authentication is
  outside this contract.
- An occupied copy is rejected before checking the member's loan limit.
- Returning a shelf copy, or naming a different holder, rejects with reason
  203. Returns are not converted to idempotent successful no-ops.
- No notification channel, due date or fine is introduced.

## Background and scope

The relational obligations follow C. A. R. Hoare, "An Axiomatic Basis for
Computer Programming," Communications of the ACM, 1969. The decreasing-count
recovery argument follows E. W. Dijkstra, "Guarded Commands, Nondeterminacy and
Formal Derivation of Programs," Communications of the ACM, 1975. This example
applies the existing library interpreter and finite checkers; it claims no new
verification method. The checks are exhaustive over this three-copy domain
and do not establish deployment authentication or unconditional availability.

## Scope

- The numbers of copies and members are fixed at three. The app factory
  supports flat, finite contracts; tables are not supported yet.
- Members do not sign in; the desk records loans for them. A
  [parties file](../../../docs/BUILD_AN_APP.md#several-parties-on-one-server)
  can give each desk its own credential and context.
- The decision examples were LLM-generated together with the rules, so they
  are not an independent oracle. The checks above cover the declared domain
  and laws; they do not show that the rules match a real library's policy.
