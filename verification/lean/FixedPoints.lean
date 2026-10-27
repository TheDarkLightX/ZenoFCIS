import Std

/-! Finite Boolean fixed points and checked, solver-proposed nested strata. -/
namespace FixedPoints

abbrev Bits (n : Nat) := Fin n → Bool
def Below (a b : Bits n) : Prop := ∀ i, a i = true → b i = true
def Monotone (f : Bits n → Bits n) : Prop := ∀ a b, Below a b → Below (f a) (f b)
def bottom : Bits n := fun _ => false
def top : Bits n := fun _ => true

def iter (f : Bits n → Bits n) (seed : Bits n) : Nat → Bits n
  | 0 => seed
  | k + 1 => f (iter f seed k)

def Least (f : Bits n → Bits n) (x : Bits n) : Prop :=
  f x = x ∧ ∀ y, f y = y → Below x y
def Greatest (f : Bits n → Bits n) (x : Bits n) : Prop :=
  f x = x ∧ ∀ y, f y = y → Below y x

theorem below_refl (a : Bits n) : Below a a := fun _ h => h
theorem below_trans {a b c : Bits n} (h : Below a b) (g : Below b c) : Below a c :=
  fun i hi => g i (h i hi)
theorem below_antisymm {a b : Bits n} (h : Below a b) (g : Below b a) : a = b := by
  funext i
  cases ha : a i <;> cases hb : b i <;> simp_all [Below]
theorem bottom_below (a : Bits n) : Below bottom a := by
  intro i hi
  cases hi
theorem below_top (a : Bits n) : Below a top := fun _ _ => rfl

theorem ascending (f : Bits n → Bits n) (hm : Monotone f) :
    ∀ k, Below (iter f bottom k) (iter f bottom (k + 1)) := by
  intro k
  induction k with
  | zero => exact bottom_below _
  | succ k ih => exact hm _ _ ih

theorem descending (f : Bits n → Bits n) (hm : Monotone f) :
    ∀ k, Below (iter f top (k + 1)) (iter f top k) := by
  intro k
  induction k with
  | zero => exact below_top _
  | succ k ih => exact hm _ _ ih

/-- Any adjacent repeat suffices; minimality of the stopping index is not
needed. This includes the first repeat proposed by a solver. -/
theorem least_of_stable (f : Bits n → Bits n) (hm : Monotone f) (k : Nat)
    (stable : iter f bottom k = iter f bottom (k + 1)) : Least f (iter f bottom k) := by
  refine ⟨stable.symm, ?_⟩
  intro y hy
  have bound : ∀ j, Below (iter f bottom j) y := by
    intro j
    induction j with
    | zero => exact bottom_below _
    | succ j ih => simpa only [iter, hy] using hm _ _ ih
  exact bound k

theorem greatest_of_stable (f : Bits n → Bits n) (hm : Monotone f) (k : Nat)
    (stable : iter f top k = iter f top (k + 1)) : Greatest f (iter f top k) := by
  refine ⟨stable.symm, ?_⟩
  intro y hy
  have bound : ∀ j, Below y (iter f top j) := by
    intro j
    induction j with
    | zero => exact below_top _
    | succ j ih => simpa only [iter, hy] using hm _ _ ih
  exact bound k

/-- Number of true coordinates; no finite-set library is needed. -/
def weight : {n : Nat} → Bits n → Nat
  | 0, _ => 0
  | n + 1, a => (if a 0 then 1 else 0) + weight (fun i : Fin n => a i.succ)

theorem weight_bound (a : Bits n) : weight a ≤ n := by
  induction n with
  | zero => exact Nat.le_refl 0
  | succ n ih =>
    have ht := ih (a := fun i => a i.succ)
    simp only [weight]
    split <;> omega

/-- Rank is monotone, and equality of ranks on comparable points forces
equality of the points. Strict steps therefore consume at least one bit. -/
theorem weight_order {a b : Bits n} (h : Below a b) :
    weight a ≤ weight b ∧ (weight a = weight b → a = b) := by
  induction n with
  | zero =>
    refine ⟨Nat.le_refl 0, fun _ => ?_⟩
    funext i
    exact Fin.elim0 i
  | succ n ih =>
    have ht : Below (fun i : Fin n => a i.succ) (fun i => b i.succ) :=
      fun i hi => h i.succ hi
    have ⟨hle, heq⟩ := ih ht
    have h0 := h 0
    have join (head : a 0 = b 0)
        (tail : (fun i : Fin n => a i.succ) = (fun i => b i.succ)) : a = b := by
      funext i
      exact Fin.cases head (fun j => congrFun tail j) i
    cases ha : a 0 <;> cases hb : b 0 <;>
      simp only [weight, ha, hb, Bool.false_eq_true, if_false, if_true]
    · exact ⟨by omega, fun he => join (ha.trans hb.symm) (heq (by omega))⟩
    · exact ⟨by omega, fun he => False.elim (by omega)⟩
    · have bad : false = true := hb.symm.trans (h0 ha)
      cases bad
    · exact ⟨by omega, fun he => join (ha.trans hb.symm) (heq (by omega))⟩

theorem ascending_repeats (f : Bits n → Bits n) (hm : Monotone f) :
    ∃ k, k ≤ n ∧ iter f bottom k = iter f bottom (k + 1) := by
  suffices boundedRepeat : ∃ k : Fin (n + 1), ∀ i : Fin n,
      iter f bottom k i = iter f bottom (k + 1) i by
    obtain ⟨k, hk⟩ := boundedRepeat
    exact ⟨k, Nat.le_of_lt_succ k.isLt, funext hk⟩
  apply Decidable.byContradiction
  intro hn
  have strict : ∀ k, k ≤ n → weight (iter f bottom k) < weight (iter f bottom (k + 1)) := by
    intro k hk
    have ⟨hle, heq⟩ := weight_order (ascending f hm k)
    have ne : iter f bottom k ≠ iter f bottom (k + 1) := fun e => hn ⟨⟨k, Nat.lt_succ_of_le hk⟩, fun i => congrFun e i⟩
    have : weight (iter f bottom k) ≠ weight (iter f bottom (k + 1)) := fun e => ne (heq e)
    omega
  have grows : ∀ k, k ≤ n + 1 → k ≤ weight (iter f bottom k) := by
    intro k
    induction k with
    | zero => intro _; omega
    | succ k ih =>
      intro hk
      have hprev := ih (by omega)
      have hstep := strict k (by omega)
      omega
  have := grows (n + 1) (Nat.le_refl _)
  have := weight_bound (iter f bottom (n + 1))
  omega

theorem descending_repeats (f : Bits n → Bits n) (hm : Monotone f) :
    ∃ k, k ≤ n ∧ iter f top k = iter f top (k + 1) := by
  suffices boundedRepeat : ∃ k : Fin (n + 1), ∀ i : Fin n,
      iter f top k i = iter f top (k + 1) i by
    obtain ⟨k, hk⟩ := boundedRepeat
    exact ⟨k, Nat.le_of_lt_succ k.isLt, funext hk⟩
  apply Decidable.byContradiction
  intro hn
  have strict : ∀ k, k ≤ n → weight (iter f top (k + 1)) < weight (iter f top k) := by
    intro k hk
    have ⟨hle, heq⟩ := weight_order (descending f hm k)
    have ne : iter f top (k + 1) ≠ iter f top k := fun e => hn ⟨⟨k, Nat.lt_succ_of_le hk⟩, fun i => congrFun e.symm i⟩
    have : weight (iter f top (k + 1)) ≠ weight (iter f top k) := fun e => ne (heq e)
    omega
  have shrinks : ∀ k, k ≤ n + 1 → weight (iter f top k) + k ≤ n := by
    intro k
    induction k with
    | zero => intro _; exact weight_bound _
    | succ k ih =>
      intro hk
      have hprev := ih (by omega)
      have hstep := strict k (by omega)
      omega
  have := shrinks (n + 1) (Nat.le_refl _)
  omega

theorem stable_forever (f : Bits n → Bits n) (seed : Bits n) (k : Nat)
    (stable : iter f seed k = iter f seed (k + 1)) :
    ∀ extra, iter f seed (k + extra) = iter f seed k := by
  intro extra
  induction extra with
  | zero => rfl
  | succ extra ih =>
    change f (iter f seed (k + extra)) = iter f seed k
    rw [ih]
    exact stable.symm

def lfp (f : Bits n → Bits n) : Bits n := iter f bottom n
def gfp (f : Bits n → Bits n) : Bits n := iter f top n

theorem lfp_least (f : Bits n → Bits n) (hm : Monotone f) : Least f (lfp f) := by
  obtain ⟨k, hk, hs⟩ := ascending_repeats f hm
  have he := stable_forever f bottom k hs (n - k)
  have sum : k + (n - k) = n := by omega
  rw [sum] at he
  unfold lfp
  rw [he]
  exact least_of_stable f hm k hs

theorem gfp_greatest (f : Bits n → Bits n) (hm : Monotone f) : Greatest f (gfp f) := by
  obtain ⟨k, hk, hs⟩ := descending_repeats f hm
  have he := stable_forever f top k hs (n - k)
  have sum : k + (n - k) = n := by omega
  rw [sum] at he
  unfold gfp
  rw [he]
  exact greatest_of_stable f hm k hs

theorem least_unique (f : Bits n → Bits n) {a b : Bits n}
    (ha : Least f a) (hb : Least f b) : a = b :=
  below_antisymm (ha.2 b hb.1) (hb.2 a ha.1)

theorem greatest_unique (f : Bits n → Bits n) {a b : Bits n}
    (ha : Greatest f a) (hb : Greatest f b) : a = b :=
  below_antisymm (hb.2 a ha.1) (ha.2 b hb.1)

theorem iter_comparison (f g : Bits n → Bits n) (hm : Monotone f)
    (hfg : ∀ x, Below (f x) (g x)) (seed : Bits n) :
    ∀ k, Below (iter f seed k) (iter g seed k) := by
  intro k
  induction k with
  | zero => exact below_refl _
  | succ k ih => exact below_trans (hm _ _ ih) (hfg _)

/-- Monotonicity of the exact inner least-fixed-point operator in the outer
parameter. Monotonicity in both arguments is a substantive premise. -/
theorem inner_monotone (H : Bits n → Bits n → Bits n)
    (hy : ∀ z, Monotone (H z))
    (hz : ∀ a b, Below a b → ∀ y, Below (H a y) (H b y)) :
    Monotone (fun z => lfp (H z)) := by
  intro a b hab
  exact iter_comparison (H a) (H b) (hy a) (hz a b hab) bottom n

theorem strata_are_iterations (F : Bits n → Bits n) (z : Nat → Bits n)
    (K : Nat) (initial : z 0 = top)
    (steps : ∀ i, i ≤ K → z (i + 1) = F (z i)) :
    ∀ i, i ≤ K + 1 → z i = iter F top i := by
  intro i
  induction i with
  | zero => intro _; exact initial
  | succ i ih =>
    intro hi
    rw [steps i (by omega), ih (by omega)]
    rfl

/-- Every proposed inner chain is checked, including the last outer step.
An adjacent outer repeat then identifies the exact nested greatest/least
fixed point, independently of how the solver chose K and the inner lengths.
Index K compares z[K] with z[K+1] (K+1 evaluated outer steps). -/
theorem nested_stable (H : Bits n → Bits n → Bits n)
    (hy : ∀ z, Monotone (H z))
    (hz : ∀ a b, Below a b → ∀ y, Below (H a y) (H b y))
    (z : Nat → Bits n) (length : Nat → Nat) (K : Nat)
    (initial : z 0 = top)
    (steps : ∀ i, i ≤ K → z (i + 1) = iter (H (z i)) bottom (length i))
    (innerStable : ∀ i, i ≤ K → iter (H (z i)) bottom (length i) =
      iter (H (z i)) bottom (length i + 1))
    (outerStable : z K = z (K + 1)) :
    z K = gfp (fun outer => lfp (H outer)) ∧
      Greatest (fun outer => lfp (H outer)) (z K) := by
  let F := fun outer => lfp (H outer)
  have hm : Monotone F := inner_monotone H hy hz
  have exactSteps : ∀ i, i ≤ K → z (i + 1) = F (z i) := by
    intro i hi
    rw [steps i hi]
    exact least_unique _ (least_of_stable _ (hy _) _ (innerStable i hi))
      (lfp_least _ (hy _))
  have identify := strata_are_iterations F z K initial exactSteps
  have hK := identify K (by omega)
  have hNext := identify (K + 1) (by omega)
  have stable : iter F top K = iter F top (K + 1) := by
    rw [← hK, ← hNext]
    exact outerStable
  have hg : Greatest F (z K) := hK ▸ greatest_of_stable F hm K stable
  exact ⟨greatest_unique F hg (gfp_greatest F hm), hg⟩

/-- The identity is monotone and stable from top, but that result is not
least on one bit. Stability alone does not repair an incorrect seed. -/
theorem top_seed_counterexample :
    Monotone (fun x : Bits 1 => x) ∧
    iter (fun x : Bits 1 => x) top 0 = iter (fun x => x) top 1 ∧
    ¬ Least (fun x : Bits 1 => x) (iter (fun x => x) top 0) := by
  refine ⟨fun _ _ h => h, rfl, ?_⟩
  intro h
  have bad := h.2 bottom rfl (0 : Fin 1) rfl
  cases bad

#print axioms below_refl
#print axioms below_trans
#print axioms below_antisymm
#print axioms bottom_below
#print axioms below_top
#print axioms ascending
#print axioms descending
#print axioms least_of_stable
#print axioms greatest_of_stable
#print axioms weight_bound
#print axioms weight_order
#print axioms ascending_repeats
#print axioms descending_repeats
#print axioms stable_forever
#print axioms lfp_least
#print axioms gfp_greatest
#print axioms least_unique
#print axioms greatest_unique
#print axioms iter_comparison
#print axioms inner_monotone
#print axioms strata_are_iterations
#print axioms nested_stable
#print axioms top_seed_counterexample

end FixedPoints
