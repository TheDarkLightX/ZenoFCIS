import Mathlib.Data.Fintype.BigOperators
import Mathlib.Data.Finset.Dedup

set_option autoImplicit false

/-!
Finite Boolean signatures and independently checked candidate acceptance.

This file formalizes a mathematical model, not a refinement of the Rust
evaluator, e-graph, enumeration routine, hash table, or extractor. The semantic
observations supplied to the acceptance theorem must include every output and
every relevant error. Local expression congruence is proved only for the pure
Boolean language below; it does not license deleting eager checked-i64 nodes.
-/

namespace BooleanSignatures

abbrev BoolValuation (n : Nat) := Fin n → Bool

/-- A complete finite list of valuations. Its ordering is irrelevant to the
theorems, provided the same list is used for the two signatures. -/
noncomputable def boolValuations (n : Nat) : List (BoolValuation n) :=
  (Finset.univ : Finset (BoolValuation n)).toList

theorem boolValuations_complete (n : Nat) (v : BoolValuation n) :
    v ∈ boolValuations n := by
  simp [boolValuations]

theorem boolValuation_card (n : Nat) :
    Fintype.card (BoolValuation n) = 2 ^ n := by
  simp [BoolValuation]

theorem boolean_function_card (n : Nat) :
    Fintype.card (BoolValuation n → Bool) = 2 ^ (2 ^ n) := by
  simp [BoolValuation]

theorem boolean_multi_output_card (n m : Nat) :
    Fintype.card (BoolValuation n → Fin m → Bool) =
      (2 ^ m) ^ (2 ^ n) := by
  simp [BoolValuation]

/-- A signature records the entire observation for every listed valuation. -/
def truthSignature {Input Output : Type} (domain : List Input)
    (observe : Input → Output) : List Output := domain.map observe

theorem signature_eq_iff_on {Input Output : Type} (domain : List Input)
    (f g : Input → Output) :
    truthSignature domain f = truthSignature domain g ↔
      ∀ v ∈ domain, f v = g v := by
  exact List.map_inj_left

/-- Completeness of the enumeration is the essential acceptance premise.
No independence assumptions about intermediate Boolean atoms are required. -/
theorem signature_eq_iff {Input Output : Type} (domain : List Input)
    (complete : ∀ v, v ∈ domain) (f g : Input → Output) :
    truthSignature domain f = truthSignature domain g ↔
      ∀ v, f v = g v := by
  constructor
  · intro h v
    exact (signature_eq_iff_on domain f g).mp h v (complete v)
  · intro h
    exact (signature_eq_iff_on domain f g).mpr (fun v _ => h v)

theorem boolean_signature_complete {n : Nat} {Output : Type}
    (f g : BoolValuation n → Output) :
    truthSignature (boolValuations n) f =
        truthSignature (boolValuations n) g ↔
      ∀ v, f v = g v :=
  signature_eq_iff (boolValuations n) (boolValuations_complete n) f g

/-- Any extensional observation context preserves semantic equality. This is
a mathematical function context, not a claim about arbitrary FCIS nodes. -/
theorem observation_congruence {Input Output Result : Type}
    (f g : Input → Output) (context : Output → Result)
    (h : ∀ v, f v = g v) :
    ∀ v, context (f v) = context (g v) := by
  intro v
  rw [h v]

inductive Expr (n : Nat) where
  | variable : Fin n → Expr n
  | constant : Bool → Expr n
  | neg : Expr n → Expr n
  | conj : Expr n → Expr n → Expr n
  | disj : Expr n → Expr n → Expr n
  | ite : Expr n → Expr n → Expr n → Expr n

def Expr.eval {n : Nat} : Expr n → BoolValuation n → Bool
  | .variable i, v => v i
  | .constant b, _ => b
  | .neg e, v => !(e.eval v)
  | .conj l r, v => l.eval v && r.eval v
  | .disj l r, v => l.eval v || r.eval v
  | .ite c a b, v => if c.eval v then a.eval v else b.eval v

def Equivalent {n : Nat} (l r : Expr n) : Prop :=
  ∀ v, l.eval v = r.eval v

/-- These contexts contain only total Boolean constructors. -/
inductive Context (n : Nat) where
  | hole : Context n
  | neg : Context n → Context n
  | conjLeft : Context n → Expr n → Context n
  | conjRight : Expr n → Context n → Context n
  | disjLeft : Context n → Expr n → Context n
  | disjRight : Expr n → Context n → Context n
  | iteCondition : Context n → Expr n → Expr n → Context n
  | iteThen : Expr n → Context n → Expr n → Context n
  | iteElse : Expr n → Expr n → Context n → Context n

def Context.plug {n : Nat} : Context n → Expr n → Expr n
  | .hole, e => e
  | .neg c, e => .neg (c.plug e)
  | .conjLeft c r, e => .conj (c.plug e) r
  | .conjRight l c, e => .conj l (c.plug e)
  | .disjLeft c r, e => .disj (c.plug e) r
  | .disjRight l c, e => .disj l (c.plug e)
  | .iteCondition c a b, e => .ite (c.plug e) a b
  | .iteThen c a b, e => .ite c (a.plug e) b
  | .iteElse c a b, e => .ite c a (b.plug e)

theorem pure_context_congruence {n : Nat} (l r : Expr n)
    (h : Equivalent l r) (context : Context n) :
    Equivalent (context.plug l) (context.plug r) := by
  intro v
  induction context with
  | hole => exact h v
  | neg c ih => simp only [Context.plug, Expr.eval, ih]
  | conjLeft c other ih => simp only [Context.plug, Expr.eval, ih]
  | conjRight other c ih => simp only [Context.plug, Expr.eval, ih]
  | disjLeft c other ih => simp only [Context.plug, Expr.eval, ih]
  | disjRight other c ih => simp only [Context.plug, Expr.eval, ih]
  | iteCondition c a b ih => simp only [Context.plug, Expr.eval, ih]
  | iteThen c a b ih => simp only [Context.plug, Expr.eval, ih]
  | iteElse c a b ih => simp only [Context.plug, Expr.eval, ih]

theorem factoring_sound {n : Nat} (a b c : Expr n) :
    Equivalent (.disj (.conj a b) (.conj a c))
      (.conj a (.disj b c)) := by
  intro v
  cases ha : a.eval v <;> cases hb : b.eval v <;> cases hc : c.eval v <;>
    simp [Expr.eval, ha, hb, hc]

/-- An arbitrary additional acceptance condition can impose actual cost,
shape, type, resource, or ABI requirements. It cannot weaken equality. -/
def checkedCandidate {Input Output : Type} [DecidableEq Output]
    (domain : List Input) (original candidate : Input → Output)
    (acceptable : Prop) [Decidable acceptable] : Input → Output :=
  if truthSignature domain original = truthSignature domain candidate ∧ acceptable
  then candidate else original

theorem checked_candidate_preserves {Input Output : Type} [DecidableEq Output]
    (domain : List Input) (complete : ∀ v, v ∈ domain)
    (original candidate : Input → Output)
    (acceptable : Prop) [Decidable acceptable] :
    ∀ v, checkedCandidate domain original candidate acceptable v = original v := by
  unfold checkedCandidate
  split
  · rename_i h
    intro v
    exact ((signature_eq_iff domain complete original candidate).mp h.1 v).symm
  · intro v
    rfl

/-- Successful observations contain all m Boolean output fields in ABI order;
failed observations retain the error value rather than a Boolean SAT verdict. -/
abbrev Observation (m : Nat) (Error : Type) := Except Error (Fin m → Bool)

theorem checked_boolean_candidate_preserves {n m : Nat} {Error : Type}
    [DecidableEq (Observation m Error)]
    (original candidate : BoolValuation n → Observation m Error)
    (acceptable : Prop) [Decidable acceptable] :
    ∀ v, checkedCandidate (boolValuations n) original candidate acceptable v =
      original v :=
  checked_candidate_preserves (boolValuations n) (boolValuations_complete n)
    original candidate acceptable

#print axioms boolValuation_card
#print axioms boolean_function_card
#print axioms boolean_multi_output_card
#print axioms boolean_signature_complete
#print axioms pure_context_congruence
#print axioms factoring_sound
#print axioms checked_boolean_candidate_preserves

end BooleanSignatures
