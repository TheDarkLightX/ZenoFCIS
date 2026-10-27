import CollectionsReductionTallies

/-! Signed conservation separates T2 coverage from the delta induction. -/
namespace CollectionsSignedTallies
open CollectionsAggregate
open CollectionsReductionTallies (recordGroup oldGroups newGroups affected)

variable {K R : Type} [DecidableEq K]

def contribution (amount : R → Int) : Option R → Int
  | none => 0
  | some r => amount r

def total (amount : R → Int) : List (K × R) → Int
  | [] => 0
  | (_, r) :: rows => amount r + total amount rows

theorem erase_total (amount : R → Int) (key : K) (rows : List (K × R))
    (h : Unique rows) :
    total amount (erase key rows) + contribution amount (lookup key rows) = total amount rows := by
  induction rows with
  | nil => rfl
  | cons head rows ih =>
    rcases head with ⟨k, r⟩
    have hs : k ∉ keys rows ∧ Unique rows := by simpa [Unique, keys] using h
    by_cases hk : key = k
    · subst k
      simp [erase, lookup, contribution, total, erase_of_not_mem key rows hs.1, Int.add_comm]
    · have ht := ih hs.2
      simp only [erase, if_neg hk, lookup, total]
      omega

theorem total_ext (amount : R → Int) (a b : List (K × R))
    (ha : Unique a) (hb : Unique b) (same : ∀ k, lookup k a = lookup k b) :
    total amount a = total amount b := by
  induction a generalizing b with
  | nil =>
    cases b with
    | nil => rfl
    | cons head rest =>
      have h := same head.1
      simp [lookup] at h
  | cons head rest ih =>
    rcases head with ⟨key, record⟩
    have hs : key ∉ keys rest ∧ Unique rest := by simpa [Unique, keys] using ha
    have at_key : lookup key b = some record := by simpa [lookup] using (same key).symm
    have rest_same : ∀ k, lookup k rest = lookup k (erase key b) := by
      intro k
      by_cases h : k = key
      · subst k
        rw [lookup_none_of_not_mem key rest hs.1, lookup_erase_same]
      · rw [lookup_erase_other key k b h]
        simpa only [lookup, if_neg h] using same k
    have hr := ih (erase key b) hs.2 (unique_erase key b hb) rest_same
    have ht := erase_total amount key b hb
    rw [at_key] at ht
    simp only [total, contribution] at ht ⊢
    omega

theorem put_total (amount : R → Int) (key : K) (value : Option R)
    (rows : List (K × R)) (h : Unique rows) :
    total amount (put key value rows) + contribution amount (lookup key rows) =
      total amount rows + contribution amount value := by
  have he := erase_total amount key rows h
  cases value with
  | none => simpa [put, contribution] using he
  | some r =>
    change amount r + total amount (erase key rows) + contribution amount (lookup key rows) =
      total amount rows + amount r
    omega

def namedTotal (amount : R → Int) (rows : List (K × R)) : List (K × Option R) → Int
  | [] => 0
  | (key, _) :: rest => contribution amount (lookup key rows) + namedTotal amount rows rest

def writtenTotal (amount : R → Int) : List (K × Option R) → Int
  | [] => 0
  | (_, value) :: rest => contribution amount value + writtenTotal amount rest

theorem writes_total (amount : R → Int) (rows : List (K × R))
    (writes : List (K × Option R)) (h : Unique rows) (hw : Unique writes) :
    total amount (applyWrites rows writes) + namedTotal amount rows writes =
      total amount rows + writtenTotal amount writes := by
  induction writes with
  | nil => simp [applyWrites, namedTotal, writtenTotal]
  | cons head rest ih =>
    rcases head with ⟨key, value⟩
    have hs : key ∉ keys rest ∧ Unique rest := by simpa [Unique, keys] using hw
    have ht := ih hs.2
    have hp := put_total amount key value (applyWrites rows rest) (unique_applyWrites rows rest h)
    rw [lookup_applyWrites_unnamed rows rest key hs.1] at hp
    simp only [applyWrites, namedTotal, writtenTotal]
    omega

theorem summary_step (amount : R → Int) (rows : List (K × R))
    (writes : List (K × Option R)) (h : Unique rows) (hw : Unique writes)
    (before after : Int) (initial : before = total amount rows)
    (delta : after + namedTotal amount rows writes = before + writtenTotal amount writes) :
    after = total amount (applyWrites rows writes) := by
  have htotal := writes_total amount rows writes h hw
  omega

variable {A B T : Type} [DecidableEq A] [DecidableEq B]

def groupWeight (group : R → A) (weight : R → Int) (selected : R → Bool)
    (key : A) (record : R) : Int :=
  if selected record && decide (group record = key) then weight record else 0

def Invariant (targetAmount : T → Int) (group : R → A) (weight : R → Int)
    (selected : R → Bool) (target : List (A × T)) (source : List (B × R)) : Prop :=
  ∀ key, contribution targetAmount (lookup key target) =
    total (groupWeight group weight selected key) source

theorem outside_record_zero (group : R → A) (weight : R → Int) (selected : R → Bool)
    (key : A) (record : Option R) (h : key ∉ recordGroup group record) :
    contribution (groupWeight group weight selected key) record = 0 := by
  cases record with
  | none => rfl
  | some r =>
    have hn : group r ≠ key := by
      intro he
      exact h (by simp [recordGroup, he])
    simp [contribution, groupWeight, hn]

theorem outside_old_zero (group : R → A) (weight : R → Int) (selected : R → Bool)
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
      Int.zero_add]

omit [DecidableEq B] in
theorem outside_new_zero (group : R → A) (weight : R → Int) (selected : R → Bool)
    (key : A) (writes : List (B × Option R)) (h : key ∉ newGroups group writes) :
    writtenTotal (groupWeight group weight selected key) writes = 0 := by
  induction writes with
  | nil => rfl
  | cons head rest ih =>
    rcases head with ⟨k, value⟩
    have hs : key ∉ recordGroup group value ∧ key ∉ newGroups group rest := by
      simpa only [newGroups, List.mem_append, not_or] using h
    simp only [writtenTotal, outside_record_zero group weight selected key value hs.1, ih hs.2,
      Int.zero_add]

/-- T4 checks precisely the finite union of changed targets and old/new groups. -/
def DeltaChecked (targetAmount : T → Int) (group : R → A) (weight : R → Int)
    (selected : R → Bool) (target : List (A × T)) (source : List (B × R))
    (targetWrites : List (A × Option T)) (sourceWrites : List (B × Option R)) : Prop :=
  ∀ key, key ∈ affected group source targetWrites sourceWrites →
    contribution targetAmount (lookup key (applyWrites target targetWrites)) +
      namedTotal (groupWeight group weight selected key) source sourceWrites =
    contribution targetAmount (lookup key target) +
      writtenTotal (groupWeight group weight selected key) sourceWrites

theorem affected_covers_all_deltas (targetAmount : T → Int) (group : R → A) (weight : R → Int)
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

/-- Signed C3 step: the finite T4 check preserves the equality at every target key. -/
theorem tally_step (targetAmount : T → Int) (group : R → A) (weight : R → Int)
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
theorem genesis_tally (targetAmount : T → Int) (group : R → A) (weight : R → Int)
    (selected : R → Bool) :
    Invariant (B := B) targetAmount group weight selected [] [] := by
  intro key
  rfl

/-- Signed C3 run: arbitrary source size, target size, concrete keys and finite prefixes. -/
theorem tally_run (targetAmount : T → Int) (group : R → A) (weight : R → Int)
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

#print axioms writes_total
#print axioms summary_step
#print axioms tally_step
#print axioms tally_run
end CollectionsSignedTallies
