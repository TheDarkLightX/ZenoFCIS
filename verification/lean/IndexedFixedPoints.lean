import FixedPoints

/-! Finite indexed meets/joins, three-level fixed points, and checked strata.
The model uses total Boolean maps. It does not assign game semantics to them. -/
namespace IndexedFixedPoints
open FixedPoints

def meet (v : Fin m → Bits n) : Bits n :=
  fun s => (List.finRange m).all (fun j => v j s)
def join (v : Fin m → Bits n) : Bits n :=
  fun s => (List.finRange m).any (fun j => v j s)

theorem meet_true (v : Fin m → Bits n) (s : Fin n) :
    meet v s = true ↔ ∀ j, v j s = true := by simp [meet]
theorem join_true (v : Fin m → Bits n) (s : Fin n) :
    join v s = true ↔ ∃ j, v j s = true := by simp [join]

theorem meet_mono {v w : Fin m → Bits n} (h : ∀ j, Below (v j) (w j)) :
    Below (meet v) (meet w) := by
  intro s hs
  exact (meet_true w s).2 (fun j => h j s ((meet_true v s).1 hs j))

theorem join_mono {v w : Fin m → Bits n} (h : ∀ j, Below (v j) (w j)) :
    Below (join v) (join w) := by
  intro s hs
  obtain ⟨j, hj⟩ := (join_true v s).1 hs
  exact (join_true w s).2 ⟨j, h j s hj⟩

theorem meet_monotone (f : Fin m → Bits n → Bits n) (h : ∀ j, Monotone (f j)) :
    Monotone (fun x => meet (fun j => f j x)) :=
  fun a b hab => meet_mono (fun j => h j a b hab)
theorem join_monotone (f : Fin m → Bits n → Bits n) (h : ∀ j, Monotone (f j)) :
    Monotone (fun x => join (fun j => f j x)) :=
  fun a b hab => join_mono (fun j => h j a b hab)

/-- Surjectivity suffices, including permutations and repeated labels. Empty
families retain the usual empty meet (top) and empty join (bottom). -/
theorem meet_reindex (v : Fin m → Bits n) (r : Fin m' → Fin m)
    (onto : ∀ j, ∃ k, r k = j) : meet (fun k => v (r k)) = meet v := by
  apply below_antisymm
  · intro s hs
    apply (meet_true v s).2
    intro j
    obtain ⟨k, rfl⟩ := onto j
    exact (meet_true _ s).1 hs k
  · exact fun s hs => (meet_true _ s).2 (fun k => (meet_true v s).1 hs (r k))

theorem join_reindex (v : Fin m → Bits n) (r : Fin m' → Fin m)
    (onto : ∀ j, ∃ k, r k = j) : join (fun k => v (r k)) = join v := by
  apply below_antisymm
  · intro s hs
    obtain ⟨k, hk⟩ := (join_true _ s).1 hs
    exact (join_true v s).2 ⟨r k, hk⟩
  · intro s hs
    obtain ⟨j, hj⟩ := (join_true v s).1 hs
    obtain ⟨k, rfl⟩ := onto j
    exact (join_true _ s).2 ⟨k, hj⟩

structure Monotone3 (f : Bits n → Bits n → Bits n → Bits n) : Prop where
  in_x : ∀ z y, Monotone (f z y)
  in_y : ∀ z a b, Below a b → ∀ x, Below (f z a x) (f z b x)
  in_z : ∀ a b, Below a b → ∀ y x, Below (f a y x) (f b y x)

abbrev Family (g a n : Nat) := Fin g → Fin a → Bits n → Bits n → Bits n → Bits n
def inner (F : Family g a n) (j : Fin g) (i : Fin a) (z y : Bits n) := gfp (F j i z y)
def middle (F : Family g a n) (j : Fin g) (z y : Bits n) :=
  join (fun i => inner F j i z y)
def outer (F : Family g a n) (z : Bits n) := meet (fun j => lfp (middle F j z))
def value (F : Family g a n) := gfp (outer F)

theorem middle_monotone_y (F : Family g a n) (hm : ∀ j i, Monotone3 (F j i))
    (j : Fin g) (z : Bits n) : Monotone (middle F j z) := by
  intro u v huv
  apply join_mono
  intro i
  exact iter_comparison _ _ ((hm j i).in_x z u) ((hm j i).in_y z u v huv) top n

theorem middle_monotone_z (F : Family g a n) (hm : ∀ j i, Monotone3 (F j i))
    (j : Fin g) (u v : Bits n) (huv : Below u v) (y : Bits n) :
    Below (middle F j u y) (middle F j v y) := by
  apply join_mono
  intro i
  exact iter_comparison _ _ ((hm j i).in_x u y)
    (fun x => (hm j i).in_z u v huv y x) top n

theorem outer_monotone (F : Family g a n) (hm : ∀ j i, Monotone3 (F j i)) :
    Monotone (outer F) := by
  intro u v huv
  apply meet_mono
  intro j
  exact iter_comparison _ _ (middle_monotone_y F hm j u)
    (middle_monotone_z F hm j u v huv) bottom n

/-- Each binding has its independently defined extremal property, not just
equality to an iteration abbreviation. -/
theorem nested_extremal (F : Family g a n) (hm : ∀ j i, Monotone3 (F j i)) :
    Greatest (outer F) (value F) ∧
    (∀ j z, Least (middle F j z) (lfp (middle F j z))) ∧
    (∀ j i z y, Greatest (F j i z y) (inner F j i z y)) := by
  exact ⟨gfp_greatest _ (outer_monotone F hm),
    fun j z => lfp_least _ (middle_monotone_y F hm j z),
    fun j i z y => gfp_greatest _ ((hm j i).in_x z y)⟩

/-- Relabel either finite index set without changing the denotation. The
stronger surjective form also permits duplication, but never omission. -/
theorem value_reindex (F : Family g a n)
    (rj : Fin g' → Fin g) (ri : Fin a' → Fin a)
    (hj : ∀ j, ∃ k, rj k = j) (hi : ∀ i, ∃ k, ri k = i) :
    value (fun j i => F (rj j) (ri i)) = value F := by
  have mid : ∀ j z, middle (fun j i => F (rj j) (ri i)) j z = middle F (rj j) z := by
    intro j z
    funext y
    exact join_reindex (fun i => inner F (rj j) i z y) ri hi
  have out : outer (fun j i => F (rj j) (ri i)) = outer F := by
    funext z
    unfold outer
    simp only [mid]
    exact meet_reindex (fun j => lfp (middle F j z)) rj hj
  exact congrArg gfp out

def complement (v : Bits n) : Bits n := fun s => !(v s)
def dual (f : Bits n → Bits n) (v : Bits n) := complement (f (complement v))

theorem complement_involution (v : Bits n) : complement (complement v) = v := by
  funext s
  simp [complement]
theorem complement_meet (v : Fin m → Bits n) :
    complement (meet v) = join (fun j => complement (v j)) := by
  funext s
  exact List.not_all_eq_any_not
theorem complement_join (v : Fin m → Bits n) :
    complement (join v) = meet (fun j => complement (v j)) := by
  funext s
  exact List.not_any_eq_all_not
theorem complement_eq_iff (u v : Bits n) : complement u = complement v ↔ u = v := by
  constructor
  · intro h
    simpa only [complement_involution] using congrArg complement h
  · exact congrArg complement
theorem complement_order {u v : Bits n} (h : Below u v) :
    Below (complement v) (complement u) := by
  intro s hs
  have hh := h s
  cases hu : u s <;> cases hv : v s <;> simp_all [complement]
theorem dual_monotone (f : Bits n → Bits n) (hm : Monotone f) : Monotone (dual f) :=
  fun _ _ h => complement_order (hm _ _ (complement_order h))

/-- The generated inner tables use a least iteration of the complemented
operator. This equation identifies every round with the direct greatest chain. -/
theorem dual_iteration (f : Bits n → Bits n) :
    ∀ k, iter (dual f) bottom k = complement (iter f top k) := by
  intro k
  induction k with
  | zero => rfl
  | succ k ih => simp only [iter, ih, dual, complement_involution]

theorem complemented_lfp (f : Bits n → Bits n) : complement (lfp (dual f)) = gfp f := by
  unfold lfp gfp
  rw [dual_iteration, complement_involution]

def tableMiddle (F : Family g a n) (j : Fin g) (z y : Bits n) :=
  complement (meet (fun i => lfp (dual (F j i z y))))

theorem tableMiddle_eq (F : Family g a n) (j : Fin g) : tableMiddle F j = middle F j := by
  funext z y
  simp only [tableMiddle, complement_meet, complemented_lfp, middle, inner]

/-- L[k] is shared by the guarantee chains in outer round k, as in the
generated tables. Round zero is the all-true outer seed. -/
def strata (F : Family g a n) (L : Nat → Nat) : Nat → Bits n
  | 0 => top
  | k + 1 => complement (join (fun j =>
      complement (iter (tableMiddle F j (strata F L k)) bottom (L (k + 1)))))

theorem strata_step (F : Family g a n) (L : Nat → Nat) (k : Nat) :
    strata F L (k + 1) =
      meet (fun j => iter (tableMiddle F j (strata F L k)) bottom (L (k + 1))) := by
  change complement (join _) = _
  simp only [complement_join, complement_involution]

theorem least_of_last_repeat (f : Bits n → Bits n) (hm : Monotone f)
    (k : Nat) (positive : 0 < k)
    (stable : iter f bottom k = iter f bottom (k - 1)) : Least f (iter f bottom k) := by
  have hs : iter f bottom (k - 1) = iter f bottom ((k - 1) + 1) := by
    simpa only [Nat.sub_add_cancel positive] using stable.symm
  rw [stable]
  exact least_of_stable f hm (k - 1) hs

theorem greatest_of_last_repeat (f : Bits n → Bits n) (hm : Monotone f)
    (k : Nat) (positive : 0 < k)
    (stable : iter f top k = iter f top (k - 1)) : Greatest f (iter f top k) := by
  have hs : iter f top (k - 1) = iter f top ((k - 1) + 1) := by
    simpa only [Nat.sub_add_cancel positive] using stable.symm
  rw [stable]
  exact greatest_of_stable f hm (k - 1) hs

/-- All middle repeats are checked for rounds 1..K and every guarantee;
the outer check compares K with K-1. No claim of minimality is trusted.
The inner level is exact after n rounds by complemented_lfp. -/
theorem generator_stable (F : Family g a n) (hm : ∀ j i, Monotone3 (F j i))
    (L : Nat → Nat) (K : Nat) (positiveK : 0 < K)
    (positiveL : ∀ k, 1 ≤ k → k ≤ K → ∀ _j : Fin g, 0 < L k)
    (middleStable : ∀ k, 1 ≤ k → k ≤ K → ∀ j,
      iter (tableMiddle F j (strata F L (k - 1))) bottom (L k) =
      iter (tableMiddle F j (strata F L (k - 1))) bottom (L k - 1))
    (outerStable : strata F L K = strata F L (K - 1)) :
    strata F L K = value F ∧ Greatest (outer F) (strata F L K) := by
  have steps : ∀ k, k + 1 ≤ K → strata F L (k + 1) = outer F (strata F L k) := by
    intro k hk
    rw [strata_step]
    change meet (fun j => iter (tableMiddle F j (strata F L k)) bottom (L (k + 1))) =
      meet (fun j => lfp (middle F j (strata F L k)))
    congr 1
    funext j
    rw [tableMiddle_eq]
    apply least_unique _ _ (lfp_least _ (middle_monotone_y F hm j _))
    apply least_of_last_repeat _ (middle_monotone_y F hm j _) _ (positiveL _ (by omega) hk j)
    simpa only [Nat.add_sub_cancel, tableMiddle_eq] using middleStable (k + 1) (by omega) hk j
  have identify : ∀ k, k ≤ K → strata F L k = iter (outer F) top k := by
    intro k
    induction k with
    | zero => intro _; rfl
    | succ k ih =>
      intro hk
      rw [steps k hk, ih (by omega)]
      rfl
  have hg : Greatest (outer F) (strata F L K) := by
    rw [identify K (Nat.le_refl _)]
    apply greatest_of_last_repeat _ (outer_monotone F hm) K positiveK
    rw [← identify K (Nat.le_refl _), ← identify (K - 1) (by omega)]
    exact outerStable
  exact ⟨greatest_unique _ hg (gfp_greatest _ (outer_monotone F hm)), hg⟩

/-- A common n+1 middle length always satisfies the last-repeat check. -/
theorem full_middle_repeat (F : Family g a n) (hm : ∀ j i, Monotone3 (F j i))
    (j : Fin g) (z : Bits n) :
    iter (tableMiddle F j z) bottom (n + 1) = iter (tableMiddle F j z) bottom n := by
  rw [tableMiddle_eq]
  exact (lfp_least _ (middle_monotone_y F hm j z)).1

theorem full_strata_iterations (F : Family g a n) (hm : ∀ j i, Monotone3 (F j i)) :
    ∀ k, strata F (fun _ => n + 1) k = iter (outer F) top k := by
  intro k
  induction k with
  | zero => rfl
  | succ k ih =>
    rw [strata_step]
    change meet (fun j => iter (tableMiddle F j (strata F (fun _ => n + 1) k)) bottom (n + 1)) = _
    simp only [full_middle_repeat F hm]
    simp only [tableMiddle_eq]
    change outer F (strata F (fun _ => n + 1) k) = outer F (iter (outer F) top k)
    rw [ih]

/-- The stability premises are jointly satisfiable for every monotone family,
including empty state or index sets. This supplies a bounded certificate;
shorter first-repeat certificates are covered by generator_stable. -/
theorem bounded_certificate (F : Family g a n) (hm : ∀ j i, Monotone3 (F j i)) :
    (0 < n + 1) ∧
    (∀ k, 1 ≤ k → k ≤ n + 1 → ∀ j,
      iter (tableMiddle F j (strata F (fun _ => n + 1) (k - 1))) bottom (n + 1) =
      iter (tableMiddle F j (strata F (fun _ => n + 1) (k - 1))) bottom ((n + 1) - 1)) ∧
    strata F (fun _ => n + 1) (n + 1) = strata F (fun _ => n + 1) ((n + 1) - 1) := by
  refine ⟨by omega, ?_, ?_⟩
  · intro k _ _ j
    simpa only [Nat.add_sub_cancel] using full_middle_repeat F hm j _
  · simp only [Nat.add_sub_cancel, full_strata_iterations F hm]
    exact (gfp_greatest _ (outer_monotone F hm)).1

/-- A monotone, nonempty family where using top for the middle least chain
passes repeat checks but returns the wrong outer result. -/
def middleIdentity : Family 1 1 1 := fun _ _ _ y _ => y
theorem middle_identity_monotone : ∀ j i, Monotone3 (middleIdentity j i) := by
  intro j i
  exact ⟨fun _ _ _ _ _ => below_refl _, fun _ _ _ h _ => h,
    fun _ _ _ _ _ => below_refl _⟩
theorem wrong_middle_seed_counterexample :
    value middleIdentity = bottom ∧
    iter (middle middleIdentity 0 top) top 1 = top ∧
    top ≠ value middleIdentity := by
  refine ⟨?_, ?_, ?_⟩
  · rfl
  · rfl
  · intro h
    have := congrFun h (0 : Fin 1)
    change true = false at this
    cases this

/-- The same concrete family satisfies all correct strata premises with
K=2 and L[1]=L[2]=2, and yields bottom from the top outer seed. -/
theorem concrete_certificate :
    (∀ k, 1 ≤ k → k ≤ 2 → ∀ j : Fin 1,
      iter (tableMiddle middleIdentity j (strata middleIdentity (fun _ => 2) (k - 1))) bottom 2 =
      iter (tableMiddle middleIdentity j (strata middleIdentity (fun _ => 2) (k - 1))) bottom 1) ∧
    strata middleIdentity (fun _ => 2) 2 = strata middleIdentity (fun _ => 2) 1 ∧
    strata middleIdentity (fun _ => 2) 2 = value middleIdentity := by
  have h := bounded_certificate middleIdentity middle_identity_monotone
  exact ⟨h.2.1, h.2.2,
    (generator_stable middleIdentity middle_identity_monotone (fun _ => 2) 2
      (by decide) (by intros; exact Nat.zero_lt_succ 1) h.2.1 h.2.2).1⟩

#print axioms meet_true
#print axioms join_true
#print axioms meet_mono
#print axioms join_mono
#print axioms meet_monotone
#print axioms join_monotone
#print axioms meet_reindex
#print axioms join_reindex
#print axioms middle_monotone_y
#print axioms middle_monotone_z
#print axioms outer_monotone
#print axioms nested_extremal
#print axioms value_reindex
#print axioms complement_involution
#print axioms complement_meet
#print axioms complement_join
#print axioms complement_eq_iff
#print axioms complement_order
#print axioms dual_monotone
#print axioms dual_iteration
#print axioms complemented_lfp
#print axioms tableMiddle_eq
#print axioms strata_step
#print axioms least_of_last_repeat
#print axioms greatest_of_last_repeat
#print axioms generator_stable
#print axioms full_middle_repeat
#print axioms full_strata_iterations
#print axioms bounded_certificate
#print axioms middle_identity_monotone
#print axioms wrong_middle_seed_counterexample
#print axioms concrete_certificate

end IndexedFixedPoints
