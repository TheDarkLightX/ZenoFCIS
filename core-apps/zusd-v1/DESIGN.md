# Native zUSD relation implementation

Specification: frozen revision-1 RELATION.md and ROUND2.md in the private build
folder. This standalone workspace is an unmounted native application. It has
no authentication or publication authority. Its checked local lock does not
change the library workspace lock, profiles, canonical bytes or evaluator.

Step 2 owns the private wide scratch, logical meter and canonical ledger
observations. Step 3 owns economic relations. Step 4 must bind authentic actor,
clock, oracle, custody and security receipts to the complete atomic application.
No API here selects an epoch duration or Stability Pool drip.
The conditional reference genesis requires an explicit funded-atom input equal
to the declared cap; that arithmetic equality authenticates no funding.

The authoritative invocation for each local relation takes immutable references
and explicit scalar inputs, starts a private Work and returns an owned
Evaluation. Policy/inputs have no interior mutation. Scratch arrays and fresh
candidate vectors have exclusive invocation ownership. Failure drops them;
Evaluation preserves the completed Step/Read/Byte prefix and denied attempt.
Retry starts from the same immutable inputs with a fresh meter. The result's
getters return shared immutable references, with no mutable getter or retained
input alias. Canonical serialization/observation is preparation, not a receipt.

Representations: six fixed 64-bit limbs for at most 384-bit mathematical
intermediates; U128 for persisted quantities; typed asset variants for AGRS,
zUSD and zDEX. A full account fold needs at most 160 bits because capacity is
u32. No signed narrowing or widened persisted type is introduced. Fixed-width
canonical map keys are (bytes32 account, U128 asset), and values are U128 atoms
at the declared e18 accounting scale. Nominal asset tags are never exchanged.

Division searches the U128 quotient in exactly 128 charged iterations and
checks 0 <= remainder < denominator. A trial product wider than scratch is
necessarily greater than its representable numerator; that trial is excluded,
not wrapped. The final remainder check detects quotient overflow. For positive
d,e, floor(floor(n/d)/e)=floor(n/(d*e)); the divided-product operation applies
this identity with a wide denominator, preserving both observable floors and
allowing the unpersisted intermediate quotient to exceed U128.

The ledger observes every key, quantity, total and actual successor. Composition
derives the exact recipient changes from the ordered movement vector, checks
debits before credits, retains old zero keys, does not create absent zero keys,
and admits only zUSD mint/burn with equal recorded-debt changes. A monetary
relation must still derive those legs from its command. Generic composition
arithmetic cannot authenticate a mint. Independent amount/recipient/frame laws
and later receipt binding cannot be replaced with conservation alone.

The existing design has immutable maps, an ordered vault subcomponent, a numeric
engine and a closed Authority. This application adds one wide scratch type,
one private meter, and native domain records rather than modifying those
profiles or adding a callback to Authority. This leaves the old source/byte
contracts intact. The extra scratch is needed for accumulated quantities and
three-factor economics; narrowing operands or rounding intermediate products
would fail full-width inputs. No existing independent guard was removed.

Evidence boundaries: native arithmetic is compared with independently generated
Python integer vectors and the existing unsigned numeric engine. Canonical map
observations, exact effects, debt/supply conservation, retained inputs, rejected
partial edits, retries and work exhaustion have separate controls. Full native
application binding, authenticated custody, all-history proofs and release
qualification remain outside steps 2 and 3.

Review status: these are component implementations, not a qualified completion
of the frozen steps' done-when conditions. PROFILE.json transcribes the complete
declared schema and parameters for review. Native component fields still use
role-labelled bytes32/U128; complete nominal lowering, original-wire admission,
generated application and full collection/effect laws are unfinished. Only the
pending-Vault and liquidation-Partition subjects have independent complete
native candidate checkers. Other tests are scoped arithmetic/behavior evidence.
No complete branch/reason or A11 coverage result is claimed. In particular the
byte-resource reasons 1060/1062/1063 have no mounted native output preflight.

MoveGainToVault returns the SP settlement and a gain-to-vault intent. The paired
borrower adjustment, committed cut and common nonce/ledger successor still need
one pure assembled relation before they can enter atomic receipt binding.
Oracle/time/identity facts supplied to component APIs are mathematical inputs;
they are not receipts. Fixed V1 numeric constants must be checked against the
selected full Params record by the assembled schema; a raw Parameters input
does not authorize a live parameter change.

The front-end snapshot stores P/G/epoch/scale, while a depositor additionally
stores S. Sharing the old larger snapshot made readers track an irrelevant
collateral snapshot and arithmetic for a party with no collateral entitlement.
The smaller front-end record retains principal and zDEX floors, tag and kickback
rules, old history coverage, ordered settlement and budget refusal. Host payout
regression checks cover the supported flow; the full parameterized equivalence
and whole-application schema binding remain open. No independent history or
custody acceptance check was removed.
