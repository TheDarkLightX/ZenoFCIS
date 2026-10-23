import Collections

/-!
An unrestricted one-role preservation principle implies excluded middle.
The constructive reduction below uses only propositional extensionality.
The collections preservation theorems instead require decidable key equality.
-/
namespace CollectionsRequiresEM
open Collections

def entryLaw : Option Prop → Prop
  | none => True
  | some p => p
def scalarLaw (_ : Unit) : Prop := True
def lawful (_ : View Unit Prop 1) (_ : Update Unit Prop 1) : Prop := True
def update : Update Unit Prop 1 := ⟨(), fun _ => some True⟩
def decision (_ : View Unit Prop 1) : Update Unit Prop 1 := update

theorem checked : Checked scalarLaw entryLaw lawful decision := by
  intro _ _
  exact ⟨True.intro, True.intro, fun _ => True.intro, fun _ _ _ => rfl⟩

-- A single named key suffices for this unrestricted principle. Here the
-- key type is Prop; no decidable equality on all propositions is supplied.
def SpecializedStep : Prop :=
  ∀ (before after : State Prop Unit Prop) (keys : Fin 1 → Prop),
    Laws scalarLaw entryLaw before → MergedWrite before after keys update →
    Laws scalarLaw entryLaw after

theorem specialized_implies_em (step : SpecializedStep) (p : Prop) : p ∨ ¬p := by
  let before : State Prop Unit Prop := ⟨(), fun _ => some True⟩
  let after : State Prop Unit Prop := ⟨(), fun q => some (q ∨ ¬q)⟩
  let keys : Fin 1 → Prop := fun _ => True
  have initial : Laws scalarLaw entryLaw before :=
    ⟨True.intro, fun _ => True.intro⟩
  have eq_true : ∀ q : Prop, q → q = True :=
    fun q h => propext ⟨fun _ => True.intro, fun _ => h⟩
  have write : MergedWrite before after keys update := by
    refine ⟨rfl, ?_, ?_⟩
    · intro _
      change some (True ∨ ¬True) = some True
      exact congrArg some (eq_true _ (Or.inl True.intro))
    · intro q unnamed
      have nq : ¬q := fun hq => unnamed 0 (propext ⟨fun _ => hq, fun _ => True.intro⟩)
      change some (q ∨ ¬q) = some True
      exact congrArg some (eq_true _ (Or.inr nq))
  exact (step before after keys initial write).2 p

#print axioms checked
#print axioms specialized_implies_em
end CollectionsRequiresEM
