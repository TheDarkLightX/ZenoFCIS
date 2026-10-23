import Std

/-!
Model-level collections reduction. The checker supplies `checked`; the shell
supplies `MergedWrite`. Neither premise asserts that a Rust implementation
satisfies this model. Entry laws are uniform and independent of scalar state.
Preservation requires decidable key equality, already a premise of run_preserves.
This covers the checker's integer, enumeration and record key types; the added
step_preserves premise excludes only key types the checker cannot use.
Without it, an unrestricted one-role step principle implies excluded middle;
see [CollectionsRequiresEM.lean](CollectionsRequiresEM.lean).
-/
namespace Collections

structure State (Key Scalar Record : Type) where
  scalar : Scalar
  entry : Key → Option Record

/-- Raw key identities are absent; only their equality pattern is visible. -/
structure View (Scalar Record : Type) (arity : Nat) where
  scalar : Scalar
  entry : Fin arity → Option Record
  same : Fin arity → Fin arity → Prop

structure Update (Scalar Record : Type) (arity : Nat) where
  scalar : Scalar
  entry : Fin arity → Option Record

def frame (s : State K S R) (keys : Fin m → K) : View S R m :=
  ⟨s.scalar, fun i => s.entry (keys i), fun i j => keys i = keys j⟩

/-- P1: identical scalar/record observations and equality patterns give the
same view, even across different key types. P2 then follows from the type of
the decision function: it receives only this view. -/
theorem frame_renaming (s : State K S R) (t : State L S R)
    (a : Fin m → K) (b : Fin m → L)
    (hs : s.scalar = t.scalar)
    (he : ∀ i, s.entry (a i) = t.entry (b i))
    (hp : ∀ i j, a i = a j ↔ b i = b j) : frame s a = frame t b := by
  have he' : (fun i => s.entry (a i)) = (fun i => t.entry (b i)) := funext he
  have hp' : (fun i j => a i = a j) = (fun i j => b i = b j) :=
    funext (fun i => funext (fun j => propext (hp i j)))
  simp only [frame, hs, he', hp']

theorem decision_renaming (decide : View S R m → Update S R m)
    (s : State K S R) (t : State L S R) (a : Fin m → K) (b : Fin m → L)
    (hs : s.scalar = t.scalar) (he : ∀ i, s.entry (a i) = t.entry (b i))
    (hp : ∀ i j, a i = a j ↔ b i = b j) :
    decide (frame s a) = decide (frame t b) :=
  congrArg decide (frame_renaming s t a b hs he hp)

def Laws (scalarLaw : S → Prop) (entryLaw : Option R → Prop)
    (s : State K S R) : Prop := scalarLaw s.scalar ∧ ∀ k, entryLaw (s.entry k)

/-- Equality is an equivalence relation, and aliased observations agree. The
finite enumeration/normalization of these views is an external obligation. -/
def Admissible (scalarLaw : S → Prop) (entryLaw : Option R → Prop)
    (v : View S R m) : Prop :=
  scalarLaw v.scalar ∧ (∀ i, entryLaw (v.entry i)) ∧
  (∀ i, v.same i i) ∧ (∀ i j, v.same i j → v.same j i) ∧
  (∀ i j k, v.same i j → v.same j k → v.same i k) ∧
  (∀ i j, v.same i j → v.entry i = v.entry j)

theorem frame_admissible (p : S → Prop) (q : Option R → Prop)
    (s : State K S R) (keys : Fin m → K) (h : Laws p q s) :
    Admissible p q (frame s keys) := by
  refine ⟨h.1, fun i => h.2 (keys i), fun _ => rfl,
    fun _ _ e => e.symm, fun _ _ _ e f => e.trans f, ?_⟩
  intro i j e
  exact congrArg s.entry e

/-- The local checker result: lawful decision, every named post-entry obeys
the uniform law, and all writes to an aliased key agree. -/
def Checked (p : S → Prop) (q : Option R → Prop)
    (lawful : View S R m → Update S R m → Prop)
    (decide : View S R m → Update S R m) : Prop :=
  ∀ v, Admissible p q v → lawful v (decide v) ∧
    p (decide v).scalar ∧ (∀ i, q ((decide v).entry i)) ∧
    (∀ i j, v.same i j → (decide v).entry i = (decide v).entry j)

/-- P4: the shell commits the checked scalars and merged named entries, and
frames every unnamed entry. No shell implementation is assumed implicitly. -/
def MergedWrite (before after : State K S R) (keys : Fin m → K)
    (u : Update S R m) : Prop :=
  after.scalar = u.scalar ∧ (∀ i, after.entry (keys i) = u.entry i) ∧
    (∀ k, (∀ i, keys i ≠ k) → after.entry k = before.entry k)

theorem step_preserves [DecidableEq K] (p : S → Prop) (q : Option R → Prop)
    (lawful : View S R m → Update S R m → Prop)
    (decide : View S R m → Update S R m) (checked : Checked p q lawful decide)
    (before after : State K S R) (keys : Fin m → K)
    (h : Laws p q before) (write : MergedWrite before after keys (decide (frame before keys))) :
    lawful (frame before keys) (decide (frame before keys)) ∧ Laws p q after := by
  obtain ⟨hl, hs, he, _⟩ := checked _ (frame_admissible p q before keys h)
  refine ⟨hl, write.1 ▸ hs, ?_⟩
  intro k
  by_cases hn : ∃ i, keys i = k
  · obtain ⟨i, rfl⟩ := hn
    rw [write.2.1 i]
    exact he i
  · rw [write.2.2 k (fun i hi => hn ⟨i, hi⟩)]
    exact h.2 k

/-- Every step of every supplied run from empty genesis preserves all laws
at every key selection. Key equality is decidable; key cardinality, support
size and run length are unbounded. -/
theorem run_preserves [DecidableEq K] (p : S → Prop) (q : Option R → Prop)
    (arity : Nat → Nat)
    (lawful : (n : Nat) → View S R (arity n) → Update S R (arity n) → Prop)
    (decide : (n : Nat) → View S R (arity n) → Update S R (arity n))
    (checked : ∀ n, Checked p q (lawful n) (decide n))
    (run : Nat → State K S R) (keys : (n : Nat) → Fin (arity n) → K)
    (genesisScalar : p (run 0).scalar) (genesisEntry : ∀ k, (run 0).entry k = none)
    (emptyLaw : q none)
    (writes : ∀ n, MergedWrite (run n) (run (n + 1)) (keys n)
      (decide n (frame (run n) (keys n)))) :
    ∀ n, Laws p q (run n) ∧
      lawful n (frame (run n) (keys n)) (decide n (frame (run n) (keys n))) := by
  have inv : ∀ n, Laws p q (run n) := by
    intro n
    induction n with
    | zero => exact ⟨genesisScalar, fun k => genesisEntry k ▸ emptyLaw⟩
    | succ n ih =>
      exact (step_preserves p q (lawful n) (decide n) (checked n) _ _ _ ih (writes n)).2
  intro n
  exact ⟨inv n,
    (step_preserves p q (lawful n) (decide n) (checked n) _ _ _ (inv n) (writes n)).1⟩

/-! Counterexample to role-specific entry laws. Keys a and b may alias; a
keeps its old value, and a distinct b is assigned true. Later b is selected
as a. The local law on a survives every step, but fails on that later view. -/
def roleStep [DecidableEq K] (s : K → Option Bool) (a b : K) : K → Option Bool :=
  fun k => if k = a then s a else if k = b then some true else s k

def zeroEntry (r : Option Bool) : Prop := r ≠ some true

/-- The two named outputs depend only on named observations and equality;
when the keys alias, both outputs are the preserved a value. -/
theorem role_named_outputs [DecidableEq K] (s : K → Option Bool) (a b : K) :
    roleStep s a b a = s a ∧
    roleStep s a b b = (if a = b then s a else some true) := by
  by_cases hab : a = b
  · simp [roleStep, hab]
  · simp [roleStep, hab, Ne.symm hab]

theorem role_local_preserved [DecidableEq K] (s : K → Option Bool) (a b : K)
    (h : zeroEntry (s a)) : zeroEntry (roleStep s a b a) := by
  simpa [roleStep] using h

theorem role_write_frame [DecidableEq K] (s : K → Option Bool) (a b k : K)
    (ha : k ≠ a) (hb : k ≠ b) : roleStep s a b k = s k := by
  simp [roleStep, ha, hb]

theorem role_only_counterexample :
    (∀ a : Bool, zeroEntry ((fun _ : Bool => none) a)) ∧
    zeroEntry (roleStep (fun _ : Bool => none) false true false) ∧
    ¬ zeroEntry (roleStep (fun _ : Bool => none) false true true) := by
  simp [zeroEntry, roleStep]

#print axioms frame_renaming
#print axioms decision_renaming
#print axioms frame_admissible
#print axioms step_preserves
#print axioms run_preserves
#print axioms role_named_outputs
#print axioms role_local_preserved
#print axioms role_write_frame
#print axioms role_only_counterexample
end Collections
