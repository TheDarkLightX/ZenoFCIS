import CollectionsAggregate

/-!
The reduction's abstract layer. A decision receives scalar observations,
named records and equality patterns. Its output is the canonical merged
post-view, after raw alias agreement and declared-law checks. Record decoding,
key-field class translation and the concrete finite enumerator are separate
refinement obligations. The entry law is uniform and scalar-independent.
-/
namespace CollectionsReduction
open CollectionsAggregate

variable {maps sums : Nat} {K R : Fin maps → Type} {S I : Type}

structure State (K R : Fin maps → Type) (S : Type) (sums : Nat) where
  scalar : S
  summary : Fin sums → Nat
  rows : (m : Fin maps) → List (K m × R m)
  unique : ∀ m, Unique (rows m)

/-- Positions may alias. Representatives count each concrete key once. -/
structure Binding (K : Fin maps → Type) (arity : Fin maps → Nat) where
  key : (m : Fin maps) → Fin (arity m) → K m
  reps : (m : Fin maps) → List (Fin (arity m))
  distinct : ∀ m, (reps m).Pairwise (fun i j => key m i ≠ key m j)
  covers : ∀ m i, ∃ j, j ∈ reps m ∧ key m j = key m i

def select {A B : Type} [DecidableEq B] (key : A → B) : List A → List A
  | [] => []
  | i :: rest =>
    let selected := select key rest
    if selected.any (fun j => decide (key j = key i)) then selected else i :: selected

theorem select_distinct {A B : Type} [DecidableEq B] (key : A → B) (indices : List A) :
    (select key indices).Pairwise (fun i j => key i ≠ key j) := by
  induction indices with
  | nil => exact List.Pairwise.nil
  | cons i rest ih =>
    by_cases h : (select key rest).any (fun j => decide (key j = key i)) = true
    · simpa only [select, h, Bool.true_eq, if_true] using ih
    · have hn : (select key rest).any (fun j => decide (key j = key i)) = false :=
        Bool.eq_false_iff.mpr h
      simp only [select, hn, Bool.false_eq_true, if_false]
      apply List.pairwise_cons.mpr
      refine ⟨?_, ih⟩
      intro j hj he
      apply h
      exact List.any_eq_true.mpr ⟨j, hj, by simp [he]⟩

theorem select_covers {A B : Type} [DecidableEq B] (key : A → B) (indices : List A) :
    ∀ i, i ∈ indices → ∃ j, j ∈ select key indices ∧ key j = key i := by
  induction indices with
  | nil => intro i hi; simp at hi
  | cons a rest ih =>
    intro i hi
    by_cases h : (select key rest).any (fun j => decide (key j = key a)) = true
    · simp only [select, h, if_true]
      rcases List.mem_cons.mp hi with he | hr
      · subst i
        obtain ⟨j, hj, he⟩ := List.any_eq_true.mp h
        exact ⟨j, hj, of_decide_eq_true he⟩
      · exact ih i hr
    · have hn : (select key rest).any (fun j => decide (key j = key a)) = false :=
        Bool.eq_false_iff.mpr h
      simp only [select, hn, Bool.false_eq_true, if_false]
      rcases List.mem_cons.mp hi with he | hr
      · subst i
        exact ⟨a, List.mem_cons_self, rfl⟩
      · obtain ⟨j, hj, he⟩ := ih i hr
        exact ⟨j, List.mem_cons_of_mem a hj, he⟩

/-- Every finite choice of concrete keys has a valid representative binding. -/
def binding [∀ m, DecidableEq (K m)] (key : (m : Fin maps) → Fin (arity m) → K m) :
    Binding K arity where
  key := key
  reps m := select (key m) (List.finRange (arity m))
  distinct m := select_distinct (key m) _
  covers m i := select_covers (key m) _ i (by simp)

/-- P1/P2 observation boundary: the key family K does not occur in this type. -/
structure View (R : Fin maps → Type) (S I : Type) (sums : Nat) (arity : Fin maps → Nat) where
  scalar : S
  summary : Fin sums → Nat
  input : I
  entry : (m : Fin maps) → Fin (arity m) → Option (R m)
  same : (m : Fin maps) → Fin (arity m) → Fin (arity m) → Bool

structure Update (R : Fin maps → Type) (S : Type) (sums : Nat) (arity : Fin maps → Nat) where
  scalar : S
  summary : Fin sums → Nat
  entry : (m : Fin maps) → Fin (arity m) → Option (R m)

variable [∀ m, DecidableEq (K m)]

def frame (s : State K R S sums) (b : Binding K arity) (input : I) : View R S I sums arity :=
  ⟨s.scalar, s.summary, input, fun m i => lookup (b.key m i) (s.rows m),
    fun m i j => decide (b.key m i = b.key m j)⟩

/-- P1: equal observations and equality patterns survive any key renaming. -/
theorem frame_renaming {L : Fin maps → Type} [∀ m, DecidableEq (L m)]
    (s : State K R S sums) (t : State L R S sums)
    (a : Binding K arity) (b : Binding L arity) (input : I)
    (hs : s.scalar = t.scalar) (ht : s.summary = t.summary)
    (he : ∀ m i, lookup (a.key m i) (s.rows m) = lookup (b.key m i) (t.rows m))
    (hp : ∀ m i j, a.key m i = a.key m j ↔ b.key m i = b.key m j) :
    frame s a input = frame t b input := by
  have entries : (fun m i => lookup (a.key m i) (s.rows m)) =
      (fun m i => lookup (b.key m i) (t.rows m)) := funext (fun m => funext (he m))
  have patterns : (fun m i j => decide (a.key m i = a.key m j)) =
      (fun m i j => decide (b.key m i = b.key m j)) := by
    funext m i j
    by_cases h : a.key m i = a.key m j
    · simp [h, (hp m i j).mp h]
    · have hn : b.key m i ≠ b.key m j := fun hb => h ((hp m i j).mpr hb)
      simp [h, hn]
  simp only [frame, hs, ht, entries, patterns]

/-- P2: the core's only argument is the complete finite observation. -/
theorem decision_renaming {L : Fin maps → Type} [∀ m, DecidableEq (L m)]
    (core : View R S I sums arity → Update R S sums arity)
    (s : State K R S sums) (t : State L R S sums)
    (a : Binding K arity) (b : Binding L arity) (input : I)
    (hs : s.scalar = t.scalar) (ht : s.summary = t.summary)
    (he : ∀ m i, lookup (a.key m i) (s.rows m) = lookup (b.key m i) (t.rows m))
    (hp : ∀ m i j, a.key m i = a.key m j ↔ b.key m i = b.key m j) :
    core (frame s a input) = core (frame t b input) :=
  congrArg core (frame_renaming s t a b input hs ht he hp)

def batch (b : Binding K arity) (u : Update R S sums arity) (m : Fin maps) :
    List (K m × Option (R m)) :=
  (b.reps m).map (fun i => (b.key m i, u.entry m i))

omit [∀ m, DecidableEq (K m)] in
theorem unique_batch (b : Binding K arity) (u : Update R S sums arity) (m : Fin maps) :
    Unique (batch b u m) := by
  simpa only [Unique, keys, batch, List.map_map, List.Nodup, List.pairwise_map,
    Function.comp_def] using b.distinct m

def slotTotal (amount : A → Nat) (entry : Fin n → Option A) : List (Fin n) → Nat
  | [] => 0
  | i :: rest => contribution amount (entry i) + slotTotal amount entry rest

theorem slotTotal_as_total {A B : Type} (key : Fin n → B) (entry : Fin n → Option A)
    (amount : A → Nat) (indices : List (Fin n)) :
    total (contribution amount) (indices.map (fun i => (key i, entry i))) =
      slotTotal amount entry indices := by
  induction indices with
  | nil => rfl
  | cons i rest ih => simp only [List.map_cons, total, slotTotal, ih]

/-- Any representative choice gives the same sum of aliased observations. -/
theorem representative_sum {A B : Type} [DecidableEq B]
    (key : Fin n → B) (entry : Fin n → Option A) (amount : A → Nat)
    (a b : List (Fin n))
    (ha : a.Pairwise (fun i j => key i ≠ key j))
    (hb : b.Pairwise (fun i j => key i ≠ key j))
    (ca : ∀ i, ∃ j, j ∈ a ∧ key j = key i)
    (cb : ∀ i, ∃ j, j ∈ b ∧ key j = key i)
    (agree : ∀ i j, key i = key j → entry i = entry j) :
    slotTotal amount entry a = slotTotal amount entry b := by
  let ra := a.map (fun i => (key i, entry i))
  let rb := b.map (fun i => (key i, entry i))
  have ua : Unique ra := by
    simpa only [ra, Unique, keys, List.map_map, Function.comp_def, List.Nodup,
      List.pairwise_map] using ha
  have ub : Unique rb := by
    simpa only [rb, Unique, keys, List.map_map, Function.comp_def, List.Nodup,
      List.pairwise_map] using hb
  have same : ∀ k, lookup k ra = lookup k rb := by
    intro k
    by_cases hm : k ∈ a.map key
    · obtain ⟨i, hi, he⟩ := List.mem_map.mp hm
      obtain ⟨j, hj, hji⟩ := cb i
      have hjk : key j = k := hji.trans he
      have ma : (k, entry i) ∈ ra := List.mem_map.mpr ⟨i, hi, by simp only [he]⟩
      have mb : (k, entry j) ∈ rb := List.mem_map.mpr ⟨j, hj, by simp only [hjk]⟩
      rw [lookup_of_mem ra ua k _ ma, lookup_of_mem rb ub k _ mb]
      exact congrArg some (agree i j hji.symm)
    · have hmb : k ∉ b.map key := by
        intro hm'
        obtain ⟨i, hi, he⟩ := List.mem_map.mp hm'
        obtain ⟨j, hj, hji⟩ := ca i
        exact hm (List.mem_map.mpr ⟨j, hj, hji.trans he⟩)
      have na : k ∉ keys ra := by simpa only [ra, keys, List.map_map, Function.comp_def] using hm
      have nb : k ∉ keys rb := by simpa only [rb, keys, List.map_map, Function.comp_def] using hmb
      rw [lookup_none_of_not_mem k ra na, lookup_none_of_not_mem k rb nb]
  have h := total_ext (contribution amount) ra rb ua ub same
  simpa only [ra, rb, slotTotal_as_total] using h

theorem two_representative_sum {A : Type} (same : Fin 2 → Fin 2 → Bool)
    (entry : Fin 2 → Option A) (amount : A → Nat) (reps : List (Fin 2))
    (refl : ∀ i, same i i = true)
    (symm : ∀ i j, same i j = true → same j i = true)
    (distinct : reps.Pairwise (fun i j => same i j = false))
    (covers : ∀ i, ∃ j, j ∈ reps ∧ same j i = true)
    (agree : ∀ i j, same i j = true → entry i = entry j) :
    slotTotal amount entry reps =
      if same 0 1 then contribution amount (entry 0)
      else contribution amount (entry 0) + contribution amount (entry 1) := by
  let key : Fin 2 → Fin 2 := fun i => if same 0 1 then 0 else i
  let canonical : List (Fin 2) := if same 0 1 then [0] else [0, 1]
  have h10 : same 1 0 = same 0 1 := Bool.eq_iff_iff.mpr ⟨symm 1 0, symm 0 1⟩
  have patterns : ∀ i j, key i = key j ↔ same i j = true := by
    cases hs : same 0 1 <;>
      simp [Fin.forall_fin_two, key, hs, refl 0, refl 1, h10]
  have hd : reps.Pairwise (fun i j => key i ≠ key j) := by
    apply distinct.imp
    intro i j h he
    have ht := (patterns i j).mp he
    simp [ht] at h
  have hc : ∀ i, ∃ j, j ∈ reps ∧ key j = key i := by
    intro i
    obtain ⟨j, hj, he⟩ := covers i
    exact ⟨j, hj, (patterns j i).mpr he⟩
  have cd : canonical.Pairwise (fun i j => key i ≠ key j) := by
    cases hs : same 0 1 <;> simp [canonical, key, hs]
  have cc : ∀ i, ∃ j, j ∈ canonical ∧ key j = key i := by
    cases hs : same 0 1 <;> simp [Fin.forall_fin_two, canonical, key, hs]
  have h := representative_sum key entry amount reps canonical hd cd hc cc
    (fun i j he => agree i j ((patterns i j).mp he))
  cases hs : same 0 1 <;> simpa [canonical, slotTotal, hs] using h

theorem namedTotal_slots (s : State K R S sums) (b : Binding K arity)
    (u : Update R S sums arity) (m : Fin maps) (amount : R m → Nat) (input : I) :
    namedTotal amount (s.rows m) (batch b u m) =
      slotTotal amount ((frame s b input).entry m) (b.reps m) := by
  change namedTotal amount (s.rows m) ((b.reps m).map (fun i => (b.key m i, u.entry m i))) =
    slotTotal amount (fun i => lookup (b.key m i) (s.rows m)) (b.reps m)
  induction b.reps m with
  | nil => rfl
  | cons i rest ih => simp only [List.map_cons, namedTotal, slotTotal, ih]

omit [∀ m, DecidableEq (K m)] in
theorem writtenTotal_slots (b : Binding K arity) (u : Update R S sums arity)
    (m : Fin maps) (amount : R m → Nat) :
    writtenTotal amount (batch b u m) = slotTotal amount (u.entry m) (b.reps m) := by
  unfold batch
  induction b.reps m with
  | nil => rfl
  | cons i rest ih => simp only [List.map_cons, writtenTotal, slotTotal, ih]

def commit (s : State K R S sums) (b : Binding K arity) (u : Update R S sums arity) :
    State K R S sums :=
  ⟨u.scalar, u.summary, fun m => applyWrites (s.rows m) (batch b u m),
    fun m => unique_applyWrites (s.rows m) (batch b u m) (s.unique m)⟩

theorem commit_named (s : State K R S sums) (b : Binding K arity)
    (u : Update R S sums arity)
    (agree : ∀ m i j, b.key m i = b.key m j → u.entry m i = u.entry m j)
    (m : Fin maps) (i : Fin (arity m)) :
    lookup (b.key m i) ((commit s b u).rows m) = u.entry m i := by
  obtain ⟨j, hj, he⟩ := b.covers m i
  have hm : (b.key m j, u.entry m j) ∈ batch b u m := List.mem_map.mpr ⟨j, hj, rfl⟩
  have hw := lookup_applyWrites_named (s.rows m) (batch b u m) (unique_batch b u m)
    (b.key m j) (u.entry m j) hm
  simpa only [commit, he, agree m j i he] using hw

theorem commit_unnamed (s : State K R S sums) (b : Binding K arity)
    (u : Update R S sums arity) (m : Fin maps) (key : K m)
    (unnamed : ∀ i, b.key m i ≠ key) :
    lookup key ((commit s b u).rows m) = lookup key (s.rows m) := by
  apply lookup_applyWrites_unnamed
  intro hm
  obtain ⟨pair, hp, he⟩ := List.mem_map.mp hm
  obtain ⟨i, _, hi⟩ := List.mem_map.mp hp
  subst pair
  exact unnamed i he

/-- P3: a scalar predicate and one uniform predicate per map, including absence. -/
def Laws (p : S → (Fin sums → Nat) → Prop) (q : (m : Fin maps) → Option (R m) → Prop)
    (s : State K R S sums) : Prop :=
  p s.scalar s.summary ∧ ∀ m k, q m (lookup k (s.rows m))

def Summaries (sumMap : Fin sums → Fin maps) (amount : (j : Fin sums) → R (sumMap j) → Nat)
    (s : State K R S sums) : Prop :=
  ∀ j, s.summary j = total (amount j) (s.rows (sumMap j))

def Representatives (v : View R S I sums arity)
    (reps : (m : Fin maps) → List (Fin (arity m))) : Prop :=
  (∀ m, (reps m).Pairwise (fun i j => v.same m i j = false)) ∧
  ∀ m i, ∃ j, j ∈ reps m ∧ v.same m j i = true

def Admissible (p : S → (Fin sums → Nat) → Prop)
    (q : (m : Fin maps) → Option (R m) → Prop)
    (sumMap : Fin sums → Fin maps) (amount : (j : Fin sums) → R (sumMap j) → Nat)
    (v : View R S I sums arity) (reps : (m : Fin maps) → List (Fin (arity m))) : Prop :=
  p v.scalar v.summary ∧
  (∀ m i, q m (v.entry m i)) ∧
  (∀ m i, v.same m i i = true) ∧
  (∀ m i j, v.same m i j = true → v.same m j i = true) ∧
  (∀ m i j k, v.same m i j = true → v.same m j k = true → v.same m i k = true) ∧
  (∀ m i j, v.same m i j = true → v.entry m i = v.entry m j) ∧
  ∀ j, slotTotal (amount j) (v.entry (sumMap j)) (reps (sumMap j)) ≤ v.summary j

def Checked (p : S → (Fin sums → Nat) → Prop)
    (q : (m : Fin maps) → Option (R m) → Prop)
    (sumMap : Fin sums → Fin maps) (amount : (j : Fin sums) → R (sumMap j) → Nat)
    (lawful : View R S I sums arity → Update R S sums arity → Prop)
    (core : View R S I sums arity → Update R S sums arity) : Prop :=
  ∀ v reps, Representatives v reps → Admissible p q sumMap amount v reps →
    lawful v (core v) ∧ p (core v).scalar (core v).summary ∧
    (∀ m i, q m ((core v).entry m i)) ∧
    (∀ m i j, v.same m i j = true → (core v).entry m i = (core v).entry m j) ∧
    ∀ j, (core v).summary j + slotTotal (amount j) (v.entry (sumMap j)) (reps (sumMap j)) =
      v.summary j + slotTotal (amount j) ((core v).entry (sumMap j)) (reps (sumMap j))

theorem frame_representatives (s : State K R S sums) (b : Binding K arity) (input : I) :
    Representatives (frame s b input) b.reps := by
  constructor
  · intro m
    exact (b.distinct m).imp (fun h => by simpa [frame] using h)
  · intro m i
    obtain ⟨j, hj, he⟩ := b.covers m i
    exact ⟨j, hj, by simpa [frame] using he⟩

theorem frame_admissible (p : S → (Fin sums → Nat) → Prop)
    (q : (m : Fin maps) → Option (R m) → Prop)
    (sumMap : Fin sums → Fin maps) (amount : (j : Fin sums) → R (sumMap j) → Nat)
    (s : State K R S sums) (b : Binding K arity) (input : I)
    (laws : Laws p q s) (sums_hold : Summaries sumMap amount s) :
    Admissible p q sumMap amount (frame s b input) b.reps := by
  refine ⟨laws.1, fun m i => laws.2 m (b.key m i), ?_, ?_, ?_, ?_, ?_⟩
  · intro m i; simp [frame]
  · intro m i j h
    simp only [frame, decide_eq_true_eq] at h ⊢
    exact h.symm
  · intro m i j k h₁ h₂
    simp only [frame, decide_eq_true_eq] at h₁ h₂ ⊢
    exact h₁.trans h₂
  · intro m i j h
    simp only [frame, decide_eq_true_eq] at h
    exact congrArg (fun k => lookup k (s.rows m)) h
  · intro j
    let u : Update R S sums arity := ⟨s.scalar, s.summary, fun _ _ => none⟩
    have h := namedTotal_le (amount j) (s.rows (sumMap j)) (batch b u (sumMap j))
      (s.unique (sumMap j)) (unique_batch b u (sumMap j))
    rw [namedTotal_slots s b u (sumMap j) (amount j) input] at h
    change slotTotal (amount j) ((frame s b input).entry (sumMap j)) (b.reps (sumMap j)) ≤ s.summary j
    rw [sums_hold j]
    exact h

/-- P4 observes partial maps extensionally; storage row order is irrelevant. -/
def MergedWrite (before after : State K R S sums) (b : Binding K arity)
    (u : Update R S sums arity) : Prop :=
  after.scalar = u.scalar ∧ after.summary = u.summary ∧
    ∀ m key, lookup key (after.rows m) = lookup key (applyWrites (before.rows m) (batch b u m))

theorem step_preserves (p : S → (Fin sums → Nat) → Prop)
    (q : (m : Fin maps) → Option (R m) → Prop)
    (sumMap : Fin sums → Fin maps) (amount : (j : Fin sums) → R (sumMap j) → Nat)
    (lawful : View R S I sums arity → Update R S sums arity → Prop)
    (core : View R S I sums arity → Update R S sums arity)
    (checked : Checked p q sumMap amount lawful core)
    (before after : State K R S sums) (b : Binding K arity) (input : I)
    (laws : Laws p q before) (summaries : Summaries sumMap amount before)
    (write : MergedWrite before after b (core (frame before b input))) :
    lawful (frame before b input) (core (frame before b input)) ∧
      Laws p q after ∧ Summaries sumMap amount after ∧
      (∀ m i j, b.key m i = b.key m j →
        (core (frame before b input)).entry m i = (core (frame before b input)).entry m j) := by
  obtain ⟨hl, hp, hq, ha, hd⟩ := checked _ _ (frame_representatives before b input)
    (frame_admissible p q sumMap amount before b input laws summaries)
  refine ⟨hl, ⟨?_, ?_⟩, ?_, ?_⟩
  · rw [write.1, write.2.1]
    exact hp
  · intro m key
    rw [write.2.2 m key]
    apply writes_preserve (q m) (before.rows m) (batch b (core (frame before b input)) m)
      (unique_batch b _ m) (laws.2 m)
    intro key value hm
    obtain ⟨i, _, he⟩ := List.mem_map.mp hm
    cases he
    exact hq m i
  · intro j
    rw [write.2.1]
    rw [total_ext (amount j) (after.rows (sumMap j))
      (applyWrites (before.rows (sumMap j)) (batch b (core (frame before b input)) (sumMap j)))
      (after.unique (sumMap j)) (unique_applyWrites _ _ (before.unique (sumMap j)))
      (write.2.2 (sumMap j))]
    apply summary_step (amount j) (before.rows (sumMap j))
      (batch b (core (frame before b input)) (sumMap j)) (before.unique (sumMap j))
      (unique_batch b _ _) (before.summary j) _ (summaries j)
    rw [namedTotal_slots before b _ (sumMap j) (amount j) input, writtenTotal_slots]
    exact hd j
  · intro m i j he
    exact ha m i j (by simpa [frame] using he)

/-- Every step of any supplied run, for any finite number of heterogeneous maps. -/
theorem run_preserves (p : S → (Fin sums → Nat) → Prop)
    (q : (m : Fin maps) → Option (R m) → Prop)
    (sumMap : Fin sums → Fin maps) (amount : (j : Fin sums) → R (sumMap j) → Nat)
    (arity : Nat → Fin maps → Nat)
    (lawful : (n : Nat) → View R S I sums (arity n) → Update R S sums (arity n) → Prop)
    (core : (n : Nat) → View R S I sums (arity n) → Update R S sums (arity n))
    (checked : ∀ n, Checked p q sumMap amount (lawful n) (core n))
    (run : Nat → State K R S sums) (binding : (n : Nat) → Binding K (arity n))
    (input : Nat → I)
    (empty : ∀ m, (run 0).rows m = []) (zero : ∀ j, (run 0).summary j = 0)
    (genesis : p (run 0).scalar (run 0).summary ∧ ∀ m, q m none)
    (writes : ∀ n, MergedWrite (run n) (run (n + 1)) (binding n)
      (core n (frame (run n) (binding n) (input n)))) :
    ∀ n, Laws p q (run n) ∧ Summaries sumMap amount (run n) ∧
      lawful n (frame (run n) (binding n) (input n))
        (core n (frame (run n) (binding n) (input n))) := by
  have invariant : ∀ n, Laws p q (run n) ∧ Summaries sumMap amount (run n) := by
    intro n
    induction n with
    | zero =>
      refine ⟨⟨genesis.1, ?_⟩, ?_⟩
      · intro m k
        simp only [empty m, lookup]
        exact genesis.2 m
      · intro j
        simp only [zero j, empty (sumMap j), total]
    | succ n ih =>
      have h := step_preserves p q sumMap amount (lawful n) (core n) (checked n)
        (run n) (run (n + 1)) (binding n) (input n) ih.1 ih.2 (writes n)
      exact ⟨h.2.1, h.2.2.1⟩
  intro n
  have h := step_preserves p q sumMap amount (lawful n) (core n) (checked n)
    (run n) (run (n + 1)) (binding n) (input n) (invariant n).1 (invariant n).2 (writes n)
  exact ⟨(invariant n).1, (invariant n).2, h.1⟩

/-- P2 as an explicit correspondence premise for an actual decision function. -/
def P2 (core : View R S I sums arity → Update R S sums arity)
    (actual : State K R S sums → Binding K arity → I → Update R S sums arity) : Prop :=
  ∀ state bindings input, actual state bindings input = core (frame state bindings input)

/-- P3 explicitly identifies the declared state laws with the uniform form. -/
def P3 (stateLaw : State K R S sums → Prop)
    (p : S → (Fin sums → Nat) → Prop) (q : (m : Fin maps) → Option (R m) → Prop) : Prop :=
  ∀ state, stateLaw state ↔ Laws p q state

/-- P4 is exact merged named publication. Capacity is unrestricted by the
mathematical theorem, so restricting runs to declared capacities is immediate. -/
def P4 (before after : State K R S sums) (bindings : Binding K arity)
    (candidate : Update R S sums arity) : Prop := MergedWrite before after bindings candidate

/-- C1 at the model boundary: actual decisions factor through the checked
view, declared laws have the uniform form, and publication is exactly P4.
The renaming consequence P1 follows from P2 and frame_renaming. -/
theorem collections_reduction (p : S → (Fin sums → Nat) → Prop)
    (q : (m : Fin maps) → Option (R m) → Prop)
    (sumMap : Fin sums → Fin maps) (amount : (j : Fin sums) → R (sumMap j) → Nat)
    (arity : Nat → Fin maps → Nat)
    (lawful : (n : Nat) → View R S I sums (arity n) → Update R S sums (arity n) → Prop)
    (core : (n : Nat) → View R S I sums (arity n) → Update R S sums (arity n))
    (actual : (n : Nat) → State K R S sums → Binding K (arity n) → I → Update R S sums (arity n))
    (checked : ∀ n, Checked p q sumMap amount (lawful n) (core n))
    (p2 : ∀ n, P2 (core n) (actual n))
    (stateLaw : State K R S sums → Prop) (p3 : P3 stateLaw p q)
    (run : Nat → State K R S sums) (bindings : (n : Nat) → Binding K (arity n))
    (input : Nat → I)
    (empty : ∀ m, (run 0).rows m = []) (zero : ∀ j, (run 0).summary j = 0)
    (genesis : p (run 0).scalar (run 0).summary ∧ ∀ m, q m none)
    (p4 : ∀ n, P4 (run n) (run (n + 1)) (bindings n)
      (actual n (run n) (bindings n) (input n))) :
    ∀ n, stateLaw (run n) ∧ Summaries sumMap amount (run n) ∧
      lawful n (frame (run n) (bindings n) (input n))
        (actual n (run n) (bindings n) (input n)) ∧
      (∀ m i j, (bindings n).key m i = (bindings n).key m j →
        (actual n (run n) (bindings n) (input n)).entry m i =
          (actual n (run n) (bindings n) (input n)).entry m j) := by
  have writes : ∀ n, MergedWrite (run n) (run (n + 1)) (bindings n)
      (core n (frame (run n) (bindings n) (input n))) := by
    intro n
    have h := p4 n
    rw [P4, p2 n] at h
    exact h
  have result := run_preserves p q sumMap amount arity lawful core checked run bindings input
    empty zero genesis writes
  intro n
  refine ⟨(p3 _).mpr (result n).1, (result n).2.1, ?_, ?_⟩
  · rw [p2 n]
    exact (result n).2.2
  · rw [p2 n]
    exact (step_preserves p q sumMap amount (lawful n) (core n) (checked n)
      (run n) (run (n + 1)) (bindings n) (input n) (result n).1 (result n).2.1
      (writes n)).2.2.2

#print axioms frame_representatives
#print axioms frame_renaming
#print axioms decision_renaming
#print axioms commit_named
#print axioms commit_unnamed
#print axioms frame_admissible
#print axioms step_preserves
#print axioms run_preserves
#print axioms collections_reduction
end CollectionsReduction
