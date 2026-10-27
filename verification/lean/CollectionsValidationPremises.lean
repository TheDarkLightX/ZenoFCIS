import CollectionsReductionGenesis
import CollectionsReductionTallies

/-! Joint countermodels for the explicitly stated semantic C1 premises. -/
namespace CollectionsValidationPremises
open CollectionsAggregate CollectionsReduction CollectionsReductionGenesis

abbrev Keys := fun _ : Fin 1 => Bool
abbrev Records := fun _ : Fin 1 => Bool
abbrev State1 := State Keys Records Unit 0
abbrev Binding1 := Binding Keys (fun _ => 1)
abbrev View1 := View Records Unit Bool 0 (fun _ => 1)
abbrev Update1 := Update Records Unit 0 (fun _ => 1)

def sumMap : Fin 0 → Fin 1 := Fin.elim0
def amount (j : Fin 0) : Records (sumMap j) → Nat := Fin.elim0 j
def scalarLaw (_ : Unit) (_ : Fin 0 → Nat) : Prop := True
def entryLaw (_ : Fin 1) (record : Option Bool) : Prop := record ≠ some true
def permissive (_ : Fin 1) (_ : Option Bool) : Prop := True
def lawful (_ : View1) (_ : Update1) : Prop := True
def stateLaw (state : State1) : Prop := Laws scalarLaw entryLaw state
def atKey (key : Bool) : Binding1 := binding (fun _ _ => key)
def empty : State1 := genesis ()

def keep (view : View1) : Update1 := ⟨view.scalar, view.summary, view.entry⟩
def safe (view : View1) : Update1 :=
  if view.input then ⟨view.scalar, view.summary, fun _ _ => some false⟩ else keep view
def forbidden (view : View1) : Update1 :=
  ⟨view.scalar, view.summary, fun _ _ => some true⟩
def observed (core : View1 → Update1) (state : State1) (bindings : Binding1) (input : Bool) :
    Update1 := core (frame state bindings input)

theorem keep_checked : Checked scalarLaw entryLaw sumMap amount lawful keep := by
  intro view reps _ admitted
  rcases admitted with ⟨hp, hq, _, _, _, ha, _⟩
  exact ⟨True.intro, hp, hq, ha, fun j => Fin.elim0 j⟩

theorem safe_checked : Checked scalarLaw entryLaw sumMap amount lawful safe := by
  intro view reps representatives admitted
  by_cases input : view.input = true
  · refine ⟨True.intro, True.intro, ?_, ?_, fun j => Fin.elim0 j⟩
    · intro m i
      simp [safe, input, entryLaw]
    · intro m i j _
      simp [safe, input]
  · simpa only [safe, input, Bool.false_eq_true, if_false] using
      keep_checked view reps representatives admitted

theorem forbidden_checked : Checked scalarLaw permissive sumMap amount lawful forbidden := by
  intro view reps _ _
  exact ⟨True.intro, True.intro, fun _ _ => True.intro, fun _ _ _ _ => rfl,
    fun j => Fin.elim0 j⟩

theorem empty_law : stateLaw empty := by
  simp [stateLaw, Laws, scalarLaw, entryLaw, empty, genesis, lookup]

/- P2: a first legal insertion is followed by an unnamed existence read. -/
def hidden (state : State1) (bindings : Binding1) (input : Bool) : Update1 :=
  if input then observed safe state bindings input
  else if (state.rows 0).isEmpty then observed keep state bindings input
  else observed forbidden state bindings input

def hiddenInput : Nat → Bool
  | 0 => true
  | _ + 1 => false
def hiddenBinding (n : Nat) : Binding1 := atKey (hiddenInput n)
def hiddenRun : Nat → State1
  | 0 => empty
  | n + 1 => commit (hiddenRun n) (hiddenBinding n)
      (hidden (hiddenRun n) (hiddenBinding n) (hiddenInput n))

theorem hidden_failure : ¬ stateLaw (hiddenRun 2) := by
  intro law
  have contradiction := law.2 0 false
  exact contradiction rfl

theorem p2_joint_countermodel :
    (∀ _n : Nat, Checked scalarLaw entryLaw sumMap amount lawful safe) ∧
    P3 stateLaw scalarLaw entryLaw ∧
    (∀ m, (hiddenRun 0).rows m = []) ∧
    (∀ j, (hiddenRun 0).summary j = 0) ∧
    (scalarLaw (hiddenRun 0).scalar (hiddenRun 0).summary ∧ ∀ m, entryLaw m none) ∧
    (∀ n, P4 (hiddenRun n) (hiddenRun (n + 1)) (hiddenBinding n)
      (hidden (hiddenRun n) (hiddenBinding n) (hiddenInput n))) ∧
    stateLaw (hiddenRun 0) ∧ stateLaw (hiddenRun 1) ∧
    ¬ stateLaw (hiddenRun 2) ∧ ¬ P2 safe hidden := by
  refine ⟨fun _ => safe_checked, fun _ => Iff.rfl, fun _ => rfl,
    fun j => Fin.elim0 j, ⟨True.intro, fun _ => by simp [entryLaw]⟩,
    fun _ => ⟨rfl, rfl, fun _ _ => rfl⟩, empty_law, ?_, hidden_failure, ?_⟩
  · change True ∧ ∀ _m : Fin 1, ∀ key : Bool, lookup key [(true, false)] ≠ some true
    refine ⟨True.intro, ?_⟩
    intro m key
    cases key <;> decide
  · intro factor
    have equality := congrArg (fun update : Update1 => update.entry 0 0)
      (factor (hiddenRun 1) (atKey false) false)
    change (some true : Option Bool) = none at equality
    cases equality

/- P3: the checked uniform predicate is weaker than the declared state law. -/
def permissiveRun : Nat → State1
  | 0 => empty
  | n + 1 => commit (permissiveRun n) (atKey false)
      (observed forbidden (permissiveRun n) (atKey false) false)

theorem permissive_failure : ¬ stateLaw (permissiveRun 1) := by
  intro law
  have contradiction := law.2 0 false
  exact contradiction rfl

theorem p3_joint_countermodel :
    (∀ _n : Nat, Checked scalarLaw permissive sumMap amount lawful forbidden) ∧
    (∀ _n : Nat, P2 forbidden (observed forbidden)) ∧
    (∀ m, (permissiveRun 0).rows m = []) ∧
    (∀ j, (permissiveRun 0).summary j = 0) ∧
    (scalarLaw (permissiveRun 0).scalar (permissiveRun 0).summary ∧ ∀ m, permissive m none) ∧
    (∀ n, P4 (permissiveRun n) (permissiveRun (n + 1)) (atKey false)
      (observed forbidden (permissiveRun n) (atKey false) false)) ∧
    stateLaw (permissiveRun 0) ∧ ¬ stateLaw (permissiveRun 1) ∧
    ¬ P3 stateLaw scalarLaw permissive := by
  refine ⟨fun _ => forbidden_checked, fun _ _ _ _ => rfl, fun _ => rfl,
    fun j => Fin.elim0 j, ⟨True.intro, fun _ => True.intro⟩,
    fun _ => ⟨rfl, rfl, fun _ _ => rfl⟩, empty_law, permissive_failure, ?_⟩
  intro identification
  exact permissive_failure ((identification (permissiveRun 1)).mpr
    ⟨True.intro, fun _ _ => True.intro⟩)

/- P4: the actual decision preserves the named view; publication adds a key. -/
def unnamed : State1 :=
  ⟨(), fun j => Fin.elim0 j, fun _ => [(true, true)], fun _ => by simp [Unique, keys]⟩
def unnamedRun : Nat → State1
  | 0 => empty
  | _ + 1 => unnamed

theorem unnamed_failure : ¬ stateLaw (unnamedRun 1) := by
  intro law
  have contradiction := law.2 0 true
  exact contradiction rfl

theorem p4_joint_countermodel :
    (∀ _n : Nat, Checked scalarLaw entryLaw sumMap amount lawful keep) ∧
    (∀ _n : Nat, P2 keep (observed keep)) ∧
    P3 stateLaw scalarLaw entryLaw ∧
    (∀ m, (unnamedRun 0).rows m = []) ∧
    (∀ j, (unnamedRun 0).summary j = 0) ∧
    (scalarLaw (unnamedRun 0).scalar (unnamedRun 0).summary ∧ ∀ m, entryLaw m none) ∧
    stateLaw (unnamedRun 0) ∧ ¬ stateLaw (unnamedRun 1) ∧
    ¬ P4 (unnamedRun 0) (unnamedRun 1) (atKey false)
      (observed keep (unnamedRun 0) (atKey false) false) := by
  refine ⟨fun _ => keep_checked, fun _ _ _ _ => rfl, fun _ => Iff.rfl,
    fun _ => rfl, fun j => Fin.elim0 j, ⟨True.intro, fun _ => by simp [entryLaw]⟩,
    empty_law, unnamed_failure, ?_⟩
  intro publication
  have equality := publication.2.2 0 true
  change (some true : Option Bool) = none at equality
  cases equality

#print axioms p2_joint_countermodel
#print axioms p3_joint_countermodel
#print axioms p4_joint_countermodel

namespace Tallies
open CollectionsReductionTallies

def targetAmount (value : Nat) : Nat := value
def group (value : Bool) : Bool := value
def weight (_ : Bool) : Nat := 1
def selected (_ : Bool) : Bool := true
def oneSource : List (Bool × Bool) := [(true, false)]
def oneTarget : List (Bool × Nat) := [(false, 1)]

theorem no_writes_checked (target : List (Bool × Nat)) (source : List (Bool × Bool))
    (grouping : Bool → Bool) :
    DeltaChecked targetAmount grouping weight selected target source [] [] := by
  simp [DeltaChecked, affected, keys, oldGroups, newGroups]

/- T3: every later delta is exact, but the nonempty genesis is inconsistent. -/
theorem t3_joint_countermodel :
    (∀ _n : Nat, Unique oneSource) ∧
    (∀ _n : Nat, Unique ([] : List (Bool × Option Bool))) ∧
    (∀ _n : Nat, ∀ key : Bool, lookup key ([] : List (Bool × Nat)) =
      lookup key (applyWrites [] [])) ∧
    (∀ _n : Nat, ∀ key : Bool, lookup key oneSource = lookup key (applyWrites oneSource [])) ∧
    (∀ _n : Nat, DeltaChecked targetAmount group weight selected [] oneSource [] []) ∧
    (∀ _n : Nat, ¬ Invariant targetAmount group weight selected [] oneSource) ∧
    oneSource ≠ [] := by
  refine ⟨fun _ => ?_, fun _ => ?_, fun _ _ => rfl, fun _ _ => rfl,
    fun _ => no_writes_checked [] oneSource group, ?_, ?_⟩
  · simp [oneSource, Unique, keys]
  · simp [Unique, keys]
  · intro n invariant
    have impossible := invariant false
    change 0 = 1 at impossible
    cases impossible
  · simp [oneSource]

def sourceRun : Nat → List (Bool × Bool)
  | 0 => []
  | _ + 1 => oneSource
def sourceBatch : Nat → List (Bool × Option Bool)
  | 0 => [(true, some false)]
  | _ + 1 => []

theorem source_unique (n : Nat) : Unique (sourceRun n) := by
  cases n <;> simp [sourceRun, oneSource, Unique, keys]
theorem source_batch_unique (n : Nat) : Unique (sourceBatch n) := by
  cases n <;> simp [sourceBatch, Unique, keys]
theorem source_frame (n : Nat) (key : Bool) :
    lookup key (sourceRun (n + 1)) = lookup key (applyWrites (sourceRun n) (sourceBatch n)) := by
  cases n <;> rfl

/- T4: an inserted source record has no matching target increment. -/
theorem t4_joint_countermodel :
    ([] : List (Bool × Nat)) = [] ∧ sourceRun 0 = [] ∧
    (∀ n, Unique (sourceRun n)) ∧ (∀ n, Unique (sourceBatch n)) ∧
    (∀ _n : Nat, ∀ key : Bool, lookup key ([] : List (Bool × Nat)) =
      lookup key (applyWrites [] [])) ∧
    (∀ n key, lookup key (sourceRun (n + 1)) =
      lookup key (applyWrites (sourceRun n) (sourceBatch n))) ∧
    Invariant targetAmount group weight selected [] (sourceRun 0) ∧
    ¬ Invariant targetAmount group weight selected [] (sourceRun 1) ∧
    ¬ DeltaChecked targetAmount group weight selected [] (sourceRun 0) [] (sourceBatch 0) := by
  refine ⟨rfl, rfl, source_unique, source_batch_unique, fun _ _ => rfl,
    source_frame, ?_, ?_, ?_⟩
  · exact genesis_tally targetAmount group weight selected
  · intro invariant
    have impossible := invariant false
    change 0 = 1 at impossible
    cases impossible
  · intro checked
    have impossible := checked false (by
      simp [affected, sourceRun, sourceBatch, keys, oldGroups, newGroups, recordGroup, lookup, group])
    change 0 = 1 at impossible
    cases impossible

def targetRun : Nat → List (Bool × Nat)
  | 0 => []
  | _ + 1 => oneTarget
def targetBatch : Nat → List (Bool × Option Nat)
  | 0 => [(false, some 1)]
  | _ + 1 => []
def changingGroup : Nat → Bool → Bool
  | 0, value => value
  | 1, value => value
  | _ + 2, value => !value

theorem target_frame (n : Nat) (key : Bool) :
    lookup key (targetRun (n + 1)) = lookup key (applyWrites (targetRun n) (targetBatch n)) := by
  cases n <;> rfl

theorem changing_delta (n : Nat) :
    DeltaChecked targetAmount (changingGroup n) weight selected
      (targetRun n) (sourceRun n) (targetBatch n) (sourceBatch n) := by
  cases n with
  | zero =>
    intro key _
    cases key <;> rfl
  | succ n => exact no_writes_checked oneTarget oneSource (changingGroup (n + 1))

/- T1: interpretation changes after a lawful insertion, with zero later writes. -/
theorem t1_joint_countermodel :
    targetRun 0 = [] ∧ sourceRun 0 = [] ∧
    (∀ n, Unique (sourceRun n)) ∧ (∀ n, Unique (sourceBatch n)) ∧
    (∀ n key, lookup key (targetRun (n + 1)) =
      lookup key (applyWrites (targetRun n) (targetBatch n))) ∧
    (∀ n key, lookup key (sourceRun (n + 1)) =
      lookup key (applyWrites (sourceRun n) (sourceBatch n))) ∧
    (∀ n, DeltaChecked targetAmount (changingGroup n) weight selected
      (targetRun n) (sourceRun n) (targetBatch n) (sourceBatch n)) ∧
    Invariant targetAmount (changingGroup 0) weight selected (targetRun 0) (sourceRun 0) ∧
    Invariant targetAmount (changingGroup 1) weight selected (targetRun 1) (sourceRun 1) ∧
    ¬ Invariant targetAmount (changingGroup 2) weight selected (targetRun 2) (sourceRun 2) ∧
    changingGroup 1 false ≠ changingGroup 2 false := by
  refine ⟨rfl, rfl, source_unique, source_batch_unique, target_frame, source_frame,
    changing_delta, ?_, ?_, ?_, ?_⟩
  · exact genesis_tally targetAmount (changingGroup 0) weight selected
  · intro key
    cases key <;> rfl
  · intro invariant
    have impossible := invariant false
    change 1 = 0 at impossible
    cases impossible
  · decide

end Tallies

#print axioms Tallies.t1_joint_countermodel
#print axioms Tallies.t3_joint_countermodel
#print axioms Tallies.t4_joint_countermodel
end CollectionsValidationPremises
