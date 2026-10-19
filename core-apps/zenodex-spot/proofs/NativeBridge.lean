/- Native-to-contract bridge for the Spot round-trip theorems, in integers.

   An accepted native Spot step is stated in exact integers: both reserves,
   LP supply, and the coalition's and everyone else's spendable balances,
   aggregated for one coalition that contains the acting account (IntStep,
   IntCreate). verification/verus/spot_refinement_proof.rs proves these facts,
   field for field, from the compiled decision program run by the production
   evaluator specification. They are ExactValue.lean's local Step contracts
   with supply cleared: for x = r0/supply and y = r1/supply, swap keeps
   supply and r0*r1 cannot fall; add and burn move at most pro rata.

   ExactValue.lean states the model over core `Rat`, whose operations already
   depend on `Classical.choice` in Lean 4.30 (`#print axioms Rat.mul`). This
   file therefore re-proves the closed-strategy and creation results directly
   on integer traces, so it stays constructive: no `Classical.choice`, no
   `native_decide`, no `grind`, no Rat. `propext` and `Quot.sound` occur. -/
namespace SpotNative

/-- One native state, aggregated for a coalition. -/
structure IntState where
  r0 : Int
  r1 : Int
  supply : Int
  liquid0 : Int
  liquid1 : Int
  outside0 : Int
  outside1 : Int

/-- The pool part of a step: ExactValue's PoolStep with supply cleared. -/
inductive IntPool (a b : IntState) : Prop where
  | reject : b = a → IntPool a b
  | swap : b.supply = a.supply → a.r0 * a.r1 ≤ b.r0 * b.r1 → IntPool a b
  | add : (b.supply - a.supply) * a.r0 ≤ (b.r0 - a.r0) * a.supply →
      (b.supply - a.supply) * a.r1 ≤ (b.r1 - a.r1) * a.supply → IntPool a b
  | burn : (a.r0 - b.r0) * a.supply ≤ (a.supply - b.supply) * a.r0 →
      (a.r1 - b.r1) * a.supply ≤ (a.supply - b.supply) * a.r1 → IntPool a b

/-- The integer Step contract for outside LP shares `q` (the permanent lock
    and every non-member). -/
structure IntStep (q : Int) (a b : IntState) : Prop where
  before : 0 < a.r0 ∧ 0 < a.r1 ∧ 0 < a.supply ∧ q ≤ a.supply
  after : 0 < b.r0 ∧ 0 < b.r1 ∧ 0 < b.supply ∧ q ≤ b.supply
  pool : IntPool a b
  conservation0 : a.liquid0 + a.r0 + a.outside0 = b.liquid0 + b.r0 + b.outside0
  conservation1 : a.liquid1 + a.r1 + a.outside1 = b.liquid1 + b.r1 + b.outside1
  isolation0 : a.outside0 ≤ b.outside0
  isolation1 : a.outside1 ≤ b.outside1

/-- Pool creation from the absent pool, with the coalition's and everyone
    else's holdings before it: ExactValue's `creation_cost` premises. -/
structure IntCreate (q initial0 initial1 outside0 outside1 : Int) (a : IntState) : Prop where
  after : 0 < a.r0 ∧ 0 < a.r1 ∧ 0 < a.supply ∧ q ≤ a.supply
  conservation0 : initial0 + outside0 = a.liquid0 + a.r0 + a.outside0
  conservation1 : initial1 + outside1 = a.liquid1 + a.r1 + a.outside1
  isolation0 : outside0 ≤ a.outside0
  isolation1 : outside1 ≤ a.outside1

/-- Accepted native steps in sequence, for the same outside shares. -/
inductive IntTrace (q : Int) : IntState → IntState → Prop where
  | nil (a : IntState) : IntTrace q a a
  | snoc {a b c : IntState} : IntTrace q a b → IntStep q b c → IntTrace q a c

/-- Reserve per share does not fall when supply moves at most pro rata:
    r * s' ≤ r' * s (that is r/s ≤ r'/s'). -/
theorem share_up {r r' s s' : Int} (h : (s' - s) * r ≤ (r' - r) * s) : r * s' ≤ r' * s := by
  have e₁ : (s' - s) * r = r * s' - r * s := by
    rw [Int.sub_mul, Int.mul_comm s' r, Int.mul_comm s r]
  have e₂ : (r' - r) * s = r' * s - r * s := Int.sub_mul r' r s
  omega

theorem share_down {r r' s s' : Int} (h : (r - r') * s ≤ (s - s') * r) : r * s' ≤ r' * s := by
  have e₁ : (r - r') * s = r * s - r' * s := Int.sub_mul r r' s
  have e₂ : (s - s') * r = r * s - r * s' := by
    rw [Int.sub_mul, Int.mul_comm s r, Int.mul_comm s' r]
  omega

/-- (a*b)*(c*d) regrouped as (a*c)*(b*d). -/
theorem regroup (a b c d : Int) : a * b * (c * d) = a * c * (b * d) := by
  rw [Int.mul_assoc, Int.mul_assoc, Int.mul_left_comm b c d]

/-- Both reserves per share rise: the product per share squared rises. -/
theorem product_of_shares {r0 r1 r0' r1' s s' : Int} (h0 : r0 * s' ≤ r0' * s) (h1 : r1 * s' ≤ r1' * s)
    (p1 : 0 ≤ r1) (ps' : 0 ≤ s') (q0 : 0 ≤ r0') (ps : 0 ≤ s) :
    r0 * r1 * (s' * s') ≤ r0' * r1' * (s * s) := by
  have hm := Int.mul_le_mul h0 h1 (Int.mul_nonneg p1 ps') (Int.mul_nonneg q0 ps)
  rw [regroup r0 s' r1 s', regroup r0' s r1' s] at hm
  exact hm

/-- The invariant of ExactValue's `pool_step_product`, cross-multiplied:
    (r0/s)(r1/s) does not fall. -/
theorem step_product {q : Int} {a b : IntState} (h : IntStep q a b) :
    a.r0 * a.r1 * (b.supply * b.supply) ≤ b.r0 * b.r1 * (a.supply * a.supply) := by
  have ha := h.before
  have hb := h.after
  cases h.pool with
  | reject e => rw [e]; exact Int.le_refl _
  | swap hs hk =>
    rw [hs]
    exact Int.mul_le_mul_of_nonneg_right hk (Int.mul_nonneg (Int.le_of_lt ha.2.2.1) (Int.le_of_lt ha.2.2.1))
  | add h0 h1 =>
    exact product_of_shares (share_up h0) (share_up h1) (Int.le_of_lt ha.2.1)
      (Int.le_of_lt hb.2.2.1) (Int.le_of_lt hb.1) (Int.le_of_lt ha.2.2.1)
  | burn h0 h1 =>
    exact product_of_shares (share_down h0) (share_down h1) (Int.le_of_lt ha.2.1)
      (Int.le_of_lt hb.2.2.1) (Int.le_of_lt hb.1) (Int.le_of_lt ha.2.2.1)

/-- Chaining the cross-multiplied invariant through a middle state with s > 0. -/
theorem chain {p0 p1 p2 s0 s1 s2 : Int} (h01 : p0 * s1 ≤ p1 * s0) (h12 : p1 * s2 ≤ p2 * s1)
    (n0 : 0 ≤ s0) (n2 : 0 ≤ s2) (m1 : 0 < s1) : p0 * s2 ≤ p2 * s0 := by
  have a := Int.mul_le_mul_of_nonneg_right h01 n2
  have b := Int.mul_le_mul_of_nonneg_right h12 n0
  have e₁ : p0 * s1 * s2 = p0 * s2 * s1 := by
    rw [Int.mul_assoc, Int.mul_comm s1 s2, ← Int.mul_assoc]
  have e₂ : p1 * s0 * s2 = p1 * s2 * s0 := by
    rw [Int.mul_assoc, Int.mul_comm s0 s2, ← Int.mul_assoc]
  have e₃ : p2 * s1 * s0 = p2 * s0 * s1 := by
    rw [Int.mul_assoc, Int.mul_comm s1 s0, ← Int.mul_assoc]
  rw [e₁, e₂] at a
  rw [e₃] at b
  exact Int.le_of_mul_le_mul_right (Int.le_trans a b) m1

/-- What a trace keeps: conservation, outside holdings that never fall, and
    a reserve product per share that never falls (ExactValue's
    `trace_invariants`), with the endpoint valid whenever the trace moved. -/
theorem trace_invariants {q : Int} {a b : IntState} (h : IntTrace q a b) :
    a.liquid0 + a.r0 + a.outside0 = b.liquid0 + b.r0 + b.outside0 ∧
    a.liquid1 + a.r1 + a.outside1 = b.liquid1 + b.r1 + b.outside1 ∧
    a.outside0 ≤ b.outside0 ∧ a.outside1 ≤ b.outside1 ∧
    (b = a ∨ ((0 < a.supply) ∧ (0 < b.r0 ∧ 0 < b.r1 ∧ 0 < b.supply) ∧
      a.r0 * a.r1 * (b.supply * b.supply) ≤ b.r0 * b.r1 * (a.supply * a.supply))) := by
  induction h with
  | nil => exact ⟨rfl, rfl, Int.le_refl _, Int.le_refl _, Or.inl rfl⟩
  | snoc _ st ih =>
    rcases ih with ⟨c0, c1, i0, i1, rest⟩
    refine ⟨c0.trans st.conservation0, c1.trans st.conservation1,
      Int.le_trans i0 st.isolation0, Int.le_trans i1 st.isolation1, Or.inr ?_⟩
    have p := step_product st
    have hb := st.before
    have hc := st.after
    cases rest with
    | inl e =>
      subst e
      exact ⟨hb.2.2.1, ⟨hc.1, hc.2.1, hc.2.2.1⟩, p⟩
    | inr r =>
      rcases r with ⟨sa, _, pab⟩
      exact ⟨sa, ⟨hc.1, hc.2.1, hc.2.2.1⟩,
        chain pab p (Int.mul_nonneg (Int.le_of_lt sa) (Int.le_of_lt sa))
          (Int.mul_nonneg (Int.le_of_lt hc.2.2.1) (Int.le_of_lt hc.2.2.1))
          (Int.mul_pos hb.2.2.1 hb.2.2.1)⟩

/-- Cash dominance would shrink both reserves, one strictly. -/
theorem product_falls {a0 a1 b0 b1 : Int} (h0 : b0 ≤ a0) (h1 : b1 ≤ a1) (hs : b0 < a0 ∨ b1 < a1)
    (pa0 : 0 < a0) (pb1 : 0 < b1) : b0 * b1 < a0 * a1 := by
  cases hs with
  | inl h =>
    have x := Int.mul_lt_mul_of_pos_right h pb1
    have y := Int.mul_le_mul_of_nonneg_left h1 (Int.le_of_lt pa0)
    exact Int.lt_of_lt_of_le x y
  | inr h =>
    have x := Int.mul_le_mul_of_nonneg_right h0 (Int.le_of_lt pb1)
    have y := Int.mul_lt_mul_of_pos_left h pa0
    exact Int.lt_of_le_of_lt x y

/-- ExactValue's `closed_liquid_no_profit` on native traces: with the
    coalition's LP restored (supply back where it started, outside shares
    fixed), its spendable cash cannot weakly dominate, strictly in one asset. -/
theorem closed_no_profit {q : Int} {a b : IntState} (h : IntTrace q a b)
    (va : 0 < a.r0 ∧ 0 < a.r1 ∧ 0 < a.supply) (closed : b.supply = a.supply) :
    ¬ (a.liquid0 ≤ b.liquid0 ∧ a.liquid1 ≤ b.liquid1 ∧
      (a.liquid0 < b.liquid0 ∨ a.liquid1 < b.liquid1)) := by
  intro ⟨d0, d1, ds⟩
  rcases trace_invariants h with ⟨c0, c1, i0, i1, rest⟩
  cases rest with
  | inl e =>
    subst e
    cases ds with
    | inl x => exact Int.lt_irrefl _ x
    | inr x => exact Int.lt_irrefl _ x
  | inr r =>
    rcases r with ⟨_, ⟨_, pb1, _⟩, p⟩
    rw [closed] at p
    have pr := Int.le_of_mul_le_mul_right p (Int.mul_pos va.2.2 va.2.2)
    have r0 : b.r0 ≤ a.r0 := by omega
    have r1 : b.r1 ≤ a.r1 := by omega
    have rs : b.r0 < a.r0 ∨ b.r1 < a.r1 := by
      cases ds with
      | inl x => exact Or.inl (by omega)
      | inr x => exact Or.inr (by omega)
    exact Int.lt_irrefl _ (Int.lt_of_lt_of_le (product_falls r0 r1 rs va.1 pb1) pr)

/-- ExactValue's `creation_liquid_no_profit` on native traces, here strict in
    both assets: after creating the pool from nothing, whatever follows, the
    coalition's spendable cash stays below its pre-creation endowment, because
    the pool keeps positive reserves and outside holdings never fall. -/
theorem creation_no_profit {q i0 i1 o0 o1 : Int} {a b : IntState}
    (c : IntCreate q i0 i1 o0 o1 a) (h : IntTrace q a b) :
    b.liquid0 < i0 ∧ b.liquid1 < i1 := by
  rcases trace_invariants h with ⟨c0, c1, j0, j1, rest⟩
  have pos : 0 < b.r0 ∧ 0 < b.r1 := by
    cases rest with
    | inl e => rw [e]; exact ⟨c.after.1, c.after.2.1⟩
    | inr r => exact ⟨r.2.1.1, r.2.1.2.1⟩
  have k0 := c.conservation0
  have k1 := c.conservation1
  have l0 := c.isolation0
  have l1 := c.isolation1
  constructor <;> omega

#print axioms step_product
#print axioms trace_invariants
#print axioms closed_no_profit
#print axioms creation_no_profit

end SpotNative
