import CollectionsReduction

/-!
Grouped count and sum invariants. T1 fixes the two maps and their fields;
T2 is represented by Nat weights and a Bool selector. T3 is empty genesis
and merged named updates. T4 is required only on the finite affected set,
including old and new groups even when no target role names them.
-/
namespace CollectionsReductionTallies
open CollectionsAggregate

variable {A B T R : Type} [DecidableEq A] [DecidableEq B]

def groupWeight (group : R → A) (weight : R → Nat) (selected : R → Bool)
    (key : A) (record : R) : Nat :=
  if selected record && decide (group record = key) then weight record else 0

def recordGroup (group : R → A) : Option R → List A
  | none => []
  | some r => [group r]

def oldGroups (group : R → A) (source : List (B × R)) : List (B × Option R) → List A
  | [] => []
  | (key, _) :: rest => recordGroup group (lookup key source) ++ oldGroups group source rest

def newGroups (group : R → A) : List (B × Option R) → List A
  | [] => []
  | (_, value) :: rest => recordGroup group value ++ newGroups group rest

def affected (group : R → A) (source : List (B × R))
    (targetWrites : List (A × Option T)) (sourceWrites : List (B × Option R)) : List A :=
  keys targetWrites ++ oldGroups group source sourceWrites ++ newGroups group sourceWrites

def Invariant (targetAmount : T → Nat) (group : R → A) (weight : R → Nat)
    (selected : R → Bool) (target : List (A × T)) (source : List (B × R)) : Prop :=
  ∀ key, contribution targetAmount (lookup key target) =
    total (groupWeight group weight selected key) source

theorem outside_record_zero (group : R → A) (weight : R → Nat) (selected : R → Bool)
    (key : A) (record : Option R) (h : key ∉ recordGroup group record) :
    contribution (groupWeight group weight selected key) record = 0 := by
  cases record with
  | none => rfl
  | some r =>
    have hn : group r ≠ key := by
      intro he
      exact h (by simp [recordGroup, he])
    simp [contribution, groupWeight, hn]

theorem outside_old_zero (group : R → A) (weight : R → Nat) (selected : R → Bool)
    (key : A) (source : List (B × R)) (writes : List (B × Option R))
    (h : key ∉ oldGroups group source writes) :
    namedTotal (groupWeight group weight selected key) source writes = 0 := by
  induction writes with
  | nil => rfl
  | cons head rest ih =>
    rcases head with ⟨k, value⟩
    have hs : key ∉ recordGroup group (lookup k source) ∧ key ∉ oldGroups group source rest := by
      simpa only [oldGroups, List.mem_append, not_or] using h
    simp only [namedTotal, outside_record_zero group weight selected key _ hs.1, ih hs.2,
      Nat.zero_add]

omit [DecidableEq B] in
theorem outside_new_zero (group : R → A) (weight : R → Nat) (selected : R → Bool)
    (key : A) (writes : List (B × Option R)) (h : key ∉ newGroups group writes) :
    writtenTotal (groupWeight group weight selected key) writes = 0 := by
  induction writes with
  | nil => rfl
  | cons head rest ih =>
    rcases head with ⟨k, value⟩
    have hs : key ∉ recordGroup group value ∧ key ∉ newGroups group rest := by
      simpa only [newGroups, List.mem_append, not_or] using h
    simp only [writtenTotal, outside_record_zero group weight selected key value hs.1, ih hs.2,
      Nat.zero_add]

/-- T4 checks precisely the finite union of changed targets and old/new groups. -/
def DeltaChecked (targetAmount : T → Nat) (group : R → A) (weight : R → Nat)
    (selected : R → Bool) (target : List (A × T)) (source : List (B × R))
    (targetWrites : List (A × Option T)) (sourceWrites : List (B × Option R)) : Prop :=
  ∀ key, key ∈ affected group source targetWrites sourceWrites →
    contribution targetAmount (lookup key (applyWrites target targetWrites)) +
      namedTotal (groupWeight group weight selected key) source sourceWrites =
    contribution targetAmount (lookup key target) +
      writtenTotal (groupWeight group weight selected key) sourceWrites

theorem affected_covers_all_deltas (targetAmount : T → Nat) (group : R → A) (weight : R → Nat)
    (selected : R → Bool) (target : List (A × T)) (source : List (B × R))
    (targetWrites : List (A × Option T)) (sourceWrites : List (B × Option R))
    (checked : DeltaChecked targetAmount group weight selected target source targetWrites sourceWrites) :
    ∀ key, contribution targetAmount (lookup key (applyWrites target targetWrites)) +
      namedTotal (groupWeight group weight selected key) source sourceWrites =
    contribution targetAmount (lookup key target) +
      writtenTotal (groupWeight group weight selected key) sourceWrites := by
  intro key
  by_cases h : key ∈ affected group source targetWrites sourceWrites
  · exact checked key h
  · have hs : (key ∉ keys targetWrites ∧ key ∉ oldGroups group source sourceWrites) ∧
        key ∉ newGroups group sourceWrites := by
      simpa only [affected, List.mem_append, not_or] using h
    rw [lookup_applyWrites_unnamed target targetWrites key hs.1.1,
      outside_old_zero group weight selected key source sourceWrites hs.1.2,
      outside_new_zero group weight selected key sourceWrites hs.2]

/-- C3 step: the finite T4 check preserves the equality at every target key. -/
theorem tally_step (targetAmount : T → Nat) (group : R → A) (weight : R → Nat)
    (selected : R → Bool) (target : List (A × T)) (source : List (B × R))
    (targetWrites : List (A × Option T)) (sourceWrites : List (B × Option R))
    (source_unique : Unique source) (writes_unique : Unique sourceWrites)
    (before : Invariant targetAmount group weight selected target source)
    (checked : DeltaChecked targetAmount group weight selected target source targetWrites sourceWrites) :
    Invariant targetAmount group weight selected
      (applyWrites target targetWrites) (applyWrites source sourceWrites) := by
  intro key
  exact summary_step (groupWeight group weight selected key) source sourceWrites
    source_unique writes_unique
    (contribution targetAmount (lookup key target))
    (contribution targetAmount (lookup key (applyWrites target targetWrites)))
    (before key)
    (affected_covers_all_deltas targetAmount group weight selected target source targetWrites sourceWrites
      checked key)

omit [DecidableEq B] in
theorem genesis_tally (targetAmount : T → Nat) (group : R → A) (weight : R → Nat)
    (selected : R → Bool) :
    Invariant (B := B) targetAmount group weight selected [] [] := by
  intro key
  rfl

/-- C3 run: arbitrary source size, target size, concrete keys and finite prefixes. -/
theorem tally_run (targetAmount : T → Nat) (group : R → A) (weight : R → Nat)
    (selected : R → Bool) (target : Nat → List (A × T)) (source : Nat → List (B × R))
    (targetWrites : Nat → List (A × Option T)) (sourceWrites : Nat → List (B × Option R))
    (empty_target : target 0 = []) (empty_source : source 0 = [])
    (source_unique : ∀ n, Unique (source n)) (writes_unique : ∀ n, Unique (sourceWrites n))
    (target_frame : ∀ n key, lookup key (target (n + 1)) = lookup key (applyWrites (target n) (targetWrites n)))
    (source_frame : ∀ n key, lookup key (source (n + 1)) = lookup key (applyWrites (source n) (sourceWrites n)))
    (checked : ∀ n, DeltaChecked targetAmount group weight selected
      (target n) (source n) (targetWrites n) (sourceWrites n)) :
    ∀ n, Invariant targetAmount group weight selected (target n) (source n) := by
  intro n
  induction n with
  | zero =>
    rw [empty_target, empty_source]
    exact genesis_tally targetAmount group weight selected
  | succ n ih =>
    have step := tally_step targetAmount group weight selected (target n) (source n)
      (targetWrites n) (sourceWrites n) (source_unique n) (writes_unique n) ih (checked n)
    intro key
    rw [target_frame n key]
    rw [total_ext (groupWeight group weight selected key) (source (n + 1))
      (applyWrites (source n) (sourceWrites n)) (source_unique (n + 1))
      (unique_applyWrites _ _ (source_unique n)) (source_frame n)]
    exact step key

#print axioms affected_covers_all_deltas
#print axioms tally_step
#print axioms genesis_tally
#print axioms tally_run
end CollectionsReductionTallies
