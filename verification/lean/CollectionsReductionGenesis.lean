import CollectionsReduction

/-! A genesis law observes the state root, without command, context or key
equality positions. Absence exposes the declared least field values. -/
namespace CollectionsReductionGenesis
open CollectionsAggregate CollectionsReduction

variable {maps sums : Nat} {K R : Fin maps → Type} {S I : Type}

structure RecordView (R : Type) where
  present : Bool
  fields : R
  deriving DecidableEq

def canonical {A : Type} (least : A) : Option A → RecordView A
  | none => ⟨false, least⟩
  | some r => ⟨true, r⟩

structure StateView (R : Fin maps → Type) (S : Type) (sums : Nat) (arity : Fin maps → Nat) where
  scalar : S
  summary : Fin sums → Nat
  entry : (m : Fin maps) → Fin (arity m) → RecordView (R m)

def stateView (least : (m : Fin maps) → R m) (v : View R S I sums arity) :
    StateView R S sums arity :=
  ⟨v.scalar, v.summary, fun m i => canonical (least m) (v.entry m i)⟩

def genesis (scalar : S) : State K R S sums :=
  ⟨scalar, fun _ => 0, fun _ => [], fun _ => List.nodup_nil⟩

def genesisView (least : (m : Fin maps) → R m) (scalar : S) : StateView R S sums arity :=
  ⟨scalar, fun _ => 0, fun m _ => ⟨false, least m⟩⟩

variable [∀ m, DecidableEq (K m)]

theorem frame_genesis (least : (m : Fin maps) → R m) (scalar : S)
    (b : Binding K arity) (input : I) :
    stateView least (frame (genesis (sums := sums) scalar) b input) =
      genesisView least scalar := rfl

def check (laws : List (StateView R S sums arity → Bool))
    (least : (m : Fin maps) → R m) (scalar : S) : Bool :=
  laws.all (fun law => law (genesisView least scalar))

/-- The one canonical state check is equivalent to all selected genesis laws. -/
theorem genesis_check_iff (laws : List (StateView R S sums arity → Bool))
    (least : (m : Fin maps) → R m) (scalar : S) (b : Binding K arity) (input : I) :
    check laws least scalar = true ↔
      ∀ law, law ∈ laws → law (stateView least (frame (genesis scalar) b input)) = true := by
  rw [frame_genesis]
  exact List.all_eq_true

/-- Changing any key selection or input cannot change the genesis-law result. -/
theorem genesis_uniform (laws : List (StateView R S sums arity → Bool))
    (least : (m : Fin maps) → R m) (scalar : S)
    (passed : check laws least scalar = true) :
    ∀ (b : Binding K arity) (input : I) law, law ∈ laws →
      law (stateView least (frame (genesis scalar) b input)) = true := by
  intro b input
  exact (genesis_check_iff laws least scalar b input).mp passed

theorem canonical_none {A : Type} (least : A) : canonical least none = ⟨false, least⟩ := rfl

theorem removed_payload_is_least (least : Nat) :
    (canonical least (none : Option Nat)).fields = least := rfl

#print axioms frame_genesis
#print axioms genesis_check_iff
#print axioms genesis_uniform
#print axioms canonical_none
#print axioms removed_payload_is_least
end CollectionsReductionGenesis
