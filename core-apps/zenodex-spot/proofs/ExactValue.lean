import Std

/- Algebraic Spot accounting model. x,y are reserves per share, not prices.
   This file proves the contract-level theorem; executable refinement is a
   separate obligation. No native decision or publication authority is created. -/
namespace SpotExact

set_option maxHeartbeats 800000

theorem product_mono (a b c d : Rat) (ha : 0 ≤ a) (hd : 0 ≤ d)
    (hx : a ≤ c) (hy : b ≤ d) : a*b ≤ c*d := by
  have h₁ := Rat.mul_le_mul_of_nonneg_left hy ha
  have h₂ := Rat.mul_le_mul_of_nonneg_right hx hd
  exact Rat.le_trans h₁ h₂

theorem product_strict (a b c d : Rat) (ha : 0 < a)
    (hd : 0 < d) (hx : c ≤ a) (hy : d ≤ b)
    (hs : c < a ∨ d < b) : c*d < a*b := by
  cases hs with
  | inl h =>
    have h₁ := Rat.mul_lt_mul_of_pos_right h hd
    have h₂ := Rat.mul_le_mul_of_nonneg_left hy (Rat.le_of_lt ha)
    grind
  | inr h =>
    have h₁ := Rat.mul_lt_mul_of_pos_left h ha
    have h₂ := Rat.mul_le_mul_of_nonneg_right hx (Rat.le_of_lt hd)
    grind

structure State where
  x : Rat
  y : Rat
  supply : Rat
  liquid0 : Rat
  liquid1 : Rat
  outside0 : Rat
  outside1 : Rat

def Valid (q : Rat) (s : State) : Prop :=
  0 < s.x ∧ 0 < s.y ∧ 0 < s.supply ∧ q ≤ s.supply

def total0 (s : State) : Rat := s.liquid0 + s.supply*s.x + s.outside0
def total1 (s : State) : Rat := s.liquid1 + s.supply*s.y + s.outside1
def wealth0 (q : Rat) (s : State) : Rat := s.liquid0 + (s.supply-q)*s.x
def wealth1 (q : Rat) (s : State) : Rat := s.liquid1 + (s.supply-q)*s.y

theorem complement0 (q : Rat) (s : State) :
    wealth0 q s + s.outside0 + q*s.x = total0 s := by
  unfold wealth0 total0
  grind
theorem complement1 (q : Rat) (s : State) :
    wealth1 q s + s.outside1 + q*s.y = total1 s := by
  unfold wealth1 total1
  grind

/- These are the per-command algebraic contracts: swap's raw k law at fixed
   supply, and pro-rata-bounded mint/burn. Rejections are literal identity. -/
inductive PoolStep (a b : State) : Prop where
  | reject : b = a → PoolStep a b
  | swap : b.supply = a.supply →
      (a.supply*a.x)*(a.supply*a.y) ≤ (b.supply*b.x)*(b.supply*b.y) → PoolStep a b
  | add (u v m : Rat) : b.supply = a.supply+m →
      b.supply*b.x = a.supply*a.x+u → b.supply*b.y = a.supply*a.y+v →
      m*a.x ≤ u → m*a.y ≤ v → PoolStep a b
  | burn (u v z : Rat) : b.supply = a.supply-z →
      b.supply*b.x = a.supply*a.x-u → b.supply*b.y = a.supply*a.y-v →
      u ≤ z*a.x → v ≤ z*a.y → PoolStep a b

theorem pool_step_product (q : Rat) (a b : State) (va : Valid q a) (vb : Valid q b)
    (h : PoolStep a b) : a.x*a.y ≤ b.x*b.y := by
  rcases va with ⟨ax, ay, as, aq⟩
  rcases vb with ⟨bx, b_y, bs, bq⟩
  cases h with
  | reject h => subst b; exact Rat.le_refl
  | swap hs hk =>
    have hp := Rat.mul_pos as as
    rw [hs] at hk
    have e₁ : (a.supply*a.x)*(a.supply*a.y) =
        (a.supply*a.supply)*(a.x*a.y) := by grind
    have e₂ : (a.supply*b.x)*(a.supply*b.y) =
        (a.supply*a.supply)*(b.x*b.y) := by grind
    rw [e₁, e₂] at hk
    exact Rat.le_of_mul_le_mul_left hk hp
  | add u v m hs hx hy hu hv =>
    have hx' : a.x ≤ b.x := by
      apply Rat.le_of_mul_le_mul_left (c := b.supply) _ bs
      calc
        b.supply*a.x = a.supply*a.x + m*a.x := by rw [hs, Rat.add_mul]
        _ ≤ a.supply*a.x + u := by grind
        _ = b.supply*b.x := hx.symm
    have hy' : a.y ≤ b.y := by
      apply Rat.le_of_mul_le_mul_left (c := b.supply) _ bs
      calc
        b.supply*a.y = a.supply*a.y + m*a.y := by rw [hs, Rat.add_mul]
        _ ≤ a.supply*a.y + v := by grind
        _ = b.supply*b.y := hy.symm
    exact product_mono _ _ _ _ (Rat.le_of_lt ax) (Rat.le_of_lt b_y) hx' hy'
  | burn u v z hs hx hy hu hv =>
    have hx' : a.x ≤ b.x := by
      apply Rat.le_of_mul_le_mul_left (c := b.supply) _ bs
      calc
        b.supply*a.x = a.supply*a.x - z*a.x := by rw [hs]; grind
        _ ≤ a.supply*a.x - u := by grind
        _ = b.supply*b.x := hx.symm
    have hy' : a.y ≤ b.y := by
      apply Rat.le_of_mul_le_mul_left (c := b.supply) _ bs
      calc
        b.supply*a.y = a.supply*a.y - z*a.y := by rw [hs]; grind
        _ ≤ a.supply*a.y - v := by grind
        _ = b.supply*b.y := hy.symm
    exact product_mono _ _ _ _ (Rat.le_of_lt ax) (Rat.le_of_lt b_y) hx' hy'

structure Step (q : Rat) (a b : State) : Prop where
  before_valid : Valid q a
  after_valid : Valid q b
  pool : PoolStep a b
  conservation0 : total0 a = total0 b
  conservation1 : total1 a = total1 b
  isolation0 : a.outside0 ≤ b.outside0
  isolation1 : a.outside1 ≤ b.outside1

inductive Trace (q : Rat) : State → State → Prop where
  | nil (a : State) : Trace q a a
  | snoc {a b c : State} : Trace q a b → Step q b c → Trace q a c

theorem trace_invariants (q : Rat) (a b : State) (h : Trace q a b) :
    a.x*a.y ≤ b.x*b.y ∧ total0 a = total0 b ∧ total1 a = total1 b ∧
    a.outside0 ≤ b.outside0 ∧ a.outside1 ≤ b.outside1 := by
  induction h with
  | nil => exact ⟨Rat.le_refl, rfl, rfl, Rat.le_refl, Rat.le_refl⟩
  | snoc h st ih =>
    have hp := pool_step_product q _ _ st.before_valid st.after_valid st.pool
    rcases ih with ⟨p, t0, t1, o0, o1⟩
    exact ⟨Rat.le_trans p hp, t0.trans st.conservation0, t1.trans st.conservation1,
      Rat.le_trans o0 st.isolation0, Rat.le_trans o1 st.isolation1⟩

theorem exact_no_free_value (q : Rat) (hq : 0 < q) (a b : State)
    (va : Valid q a) (vb : Valid q b) (h : Trace q a b) :
    ¬ (wealth0 q a ≤ wealth0 q b ∧ wealth1 q a ≤ wealth1 q b ∧
      (wealth0 q a < wealth0 q b ∨ wealth1 q a < wealth1 q b)) := by
  intro ⟨h0, h1, hs⟩
  rcases trace_invariants q a b h with ⟨hp, t0, t1, o0, o1⟩
  have ac0 := complement0 q a
  have ac1 := complement1 q a
  have bc0 := complement0 q b
  have bc1 := complement1 q b
  have hx : b.x ≤ a.x := by
    apply Rat.le_of_mul_le_mul_left (c := q) _ hq
    grind
  have hy : b.y ≤ a.y := by
    apply Rat.le_of_mul_le_mul_left (c := q) _ hq
    grind
  have hstrict : b.x < a.x ∨ b.y < a.y := by
    cases hs with
    | inl hs =>
      left
      apply Rat.lt_of_mul_lt_mul_left (c := q) _ (Rat.le_of_lt hq)
      grind
    | inr hs =>
      right
      apply Rat.lt_of_mul_lt_mul_left (c := q) _ (Rat.le_of_lt hq)
      grind
  have bad := product_strict a.x a.y b.x b.y va.1 vb.2.1 hx hy hstrict
  grind

/- The supporting line of xy >= x0*y0 at the initial reserve ratio. -/
theorem reserve_product_tangent (a b c d : Rat) (ha : 0 < a) (hb : 0 < b) (hc : 0 < c) (hd : 0 < d)
    (hp : a*b ≤ c*d) : 2*a*b ≤ b*c+a*d := by
  have hz : 0 ≤ (b*c-a*d)*(b*c-a*d) := by
    by_cases h : 0 ≤ b*c-a*d
    · exact Rat.mul_nonneg h h
    · have hn : 0 ≤ -(b*c-a*d) := by grind
      have he : (-(b*c-a*d))*(-(b*c-a*d)) = (b*c-a*d)*(b*c-a*d) := by grind
      rw [← he]
      exact Rat.mul_nonneg hn hn
  have hp' := Rat.mul_le_mul_of_nonneg_left hp (show 0 ≤ 4*a*b by
    have hh := Rat.mul_pos ha hb
    grind)
  have he : (b*c+a*d)*(b*c+a*d) =
      (b*c-a*d)*(b*c-a*d)+4*a*b*(c*d) := by grind
  have hs : (2*a*b)*(2*a*b) ≤ (b*c+a*d)*(b*c+a*d) := by
    rw [he]
    have ht : 4*a*b*(a*b) = (2*a*b)*(2*a*b) := by grind
    rw [ht] at hp'
    grind
  have hpos : 0 < 2*a*b := by
    have hh := Rat.mul_pos ha hb
    have he : 2*a*b = 2*(a*b) := by grind
    rw [he]
    exact Rat.mul_pos (by decide) hh
  have ht : 0 < b*c+a*d := by
    have hbc := Rat.mul_pos hb hc
    have had := Rat.mul_pos ha hd
    grind
  by_cases h : 2*a*b ≤ b*c+a*d
  · exact h
  have hlt : b*c+a*d < 2*a*b := by grind
  have hsq := Rat.mul_lt_mul_of_pos_left hlt ht
  have hsq2 := Rat.mul_lt_mul_of_pos_right hlt hpos
  grind


theorem weighted_exact_nonincrease (q : Rat) (hq : 0 < q) (a b : State)
    (va : Valid q a) (vb : Valid q b) (h : Trace q a b) :
    a.y*(wealth0 q b-wealth0 q a) + a.x*(wealth1 q b-wealth1 q a) ≤ 0 := by
  rcases trace_invariants q a b h with ⟨hp, t0, t1, o0, o1⟩
  have ht := reserve_product_tangent a.x a.y b.x b.y va.1 va.2.1 vb.1 vb.2.1 hp
  have c0a := complement0 q a
  have c0b := complement0 q b
  have c1a := complement1 q a
  have c1b := complement1 q b
  have d0 : wealth0 q b-wealth0 q a =
      -(b.outside0-a.outside0)-q*(b.x-a.x) := by grind
  have d1 : wealth1 q b-wealth1 q a =
      -(b.outside1-a.outside1)-q*(b.y-a.y) := by grind
  have h0 := Rat.mul_nonneg (Rat.le_of_lt va.2.1)
      (show 0 ≤ b.outside0-a.outside0 by grind)
  have h1 := Rat.mul_nonneg (Rat.le_of_lt va.1)
      (show 0 ≤ b.outside1-a.outside1 by grind)
  have hq' := Rat.mul_nonneg (Rat.le_of_lt hq)
      (show 0 ≤ a.y*b.x+a.x*b.y-2*a.x*a.y by grind)
  have he : a.y*(wealth0 q b-wealth0 q a) + a.x*(wealth1 q b-wealth1 q a) =
      -(a.y*(b.outside0-a.outside0)) - a.x*(b.outside1-a.outside1) -
        q*(a.y*b.x+a.x*b.y-2*a.x*a.y) := by rw [d0, d1]; grind
  rw [he]
  grind

theorem cumulative_floored_bound (q : Rat) (hq : 0 < q) (a b : State)
    (va : Valid q a) (vb : Valid q b) (h : Trace q a b)
    (e00 e01 e10 e11 : Rat) (h10 : 0 ≤ e10) (h11 : 0 ≤ e11) :
    a.y*((wealth0 q b-e10)-(wealth0 q a-e00)) +
      a.x*((wealth1 q b-e11)-(wealth1 q a-e01)) ≤ a.y*e00+a.x*e01 := by
  have hw := weighted_exact_nonincrease q hq a b va vb h
  have h0 := Rat.mul_nonneg (Rat.le_of_lt va.2.1) h10
  have h1 := Rat.mul_nonneg (Rat.le_of_lt va.1) h11
  have he : a.y*((wealth0 q b-e10)-(wealth0 q a-e00)) +
      a.x*((wealth1 q b-e11)-(wealth1 q a-e01)) =
      (a.y*(wealth0 q b-wealth0 q a)+a.x*(wealth1 q b-wealth1 q a)) +
        (a.y*e00+a.x*e01)-a.y*e10-a.x*e11 := by grind
  rw [he]
  grind

/- Creation is the only way to leave the absent-pool state. Its conservation
   equation and the positive outside claim make coalition wealth no larger
   than the pre-creation liquid endowment. Earlier rejects are identities. -/
theorem creation_cost (q initial0 initial1 outside0 outside1 : Rat) (a : State)
    (hq : 0 < q) (va : Valid q a)
    (c0 : initial0 + outside0 = total0 a)
    (c1 : initial1 + outside1 = total1 a)
    (o0 : outside0 ≤ a.outside0) (o1 : outside1 ≤ a.outside1) :
    wealth0 q a ≤ initial0 ∧ wealth1 q a ≤ initial1 := by
  have h0 := complement0 q a
  have h1 := complement1 q a
  have p0 := Rat.mul_pos hq va.1
  have p1 := Rat.mul_pos hq va.2.1
  grind

theorem exact_no_free_value_from_creation (q initial0 initial1 : Rat) (a b : State)
    (hq : 0 < q) (va : Valid q a) (vb : Valid q b) (h : Trace q a b)
    (cost : wealth0 q a ≤ initial0 ∧ wealth1 q a ≤ initial1) :
    ¬ (initial0 ≤ wealth0 q b ∧ initial1 ≤ wealth1 q b ∧
      (initial0 < wealth0 q b ∨ initial1 < wealth1 q b)) := by
  have hn := exact_no_free_value q hq a b va vb h
  grind

/- Exact minus separately floored claims. Liquid holdings are integral and
   cancel in the difference. Keeping each coalition member is essential. -/
def error (xs : List Rat) : Rat :=
  (xs.map fun x => x - (x.floor : Rat)).sum

def floorTotal (xs : List Rat) : Rat := (xs.map fun x => (x.floor : Rat)).sum

theorem exact_minus_floor (xs : List Rat) : xs.sum-floorTotal xs = error xs := by
  induction xs with
  | nil => simp [floorTotal, error, Rat.sub_self]
  | cons x xs ih =>
    simp only [floorTotal, error, List.map_cons, List.sum_cons] at *
    grind

theorem rounded_change_excess (liquid0 liquid1 : Int) (before after : List Rat) :
    (((liquid1 : Rat)+floorTotal after)-((liquid0 : Rat)+floorTotal before)) -
      (((liquid1 : Rat)+after.sum)-((liquid0 : Rat)+before.sum)) =
        error before-error after := by
  have hb := exact_minus_floor before
  have ha := exact_minus_floor after
  grind

theorem error_nonneg (xs : List Rat) : 0 ≤ error xs := by
  induction xs with
  | nil => simp [error]
  | cons x xs ih =>
    have hf := Rat.floor_le x
    simp only [error, List.map_cons, List.sum_cons] at *
    grind

theorem error_lt_count (xs : List Rat) (hne : xs ≠ []) :
    error xs < (xs.length : Rat) := by
  induction xs with
  | nil => contradiction
  | cons x xs ih =>
    have hf := Rat.lt_floor (x := x)
    cases xs with
    | nil =>
      simp only [error, List.map_cons, List.map_nil, List.sum_cons, List.sum_nil,
        List.length_cons, List.length_nil]
      grind
    | cons y ys =>
      have ht := ih (by simp)
      simp only [error, List.map_cons, List.sum_cons, List.length_cons] at *
      grind

theorem endpoint_dust (before after : List Rat) (hb : before ≠ [])
    :
    error before - error after < (before.length : Rat) := by
  have h₁ := error_lt_count before hb
  have h₂ := error_nonneg after
  grind

theorem endpoint_dust_two_sided (before after : List Rat) (hb : before ≠ [])
    (same : before.length = after.length) :
    -(before.length : Rat) < error before - error after ∧
    error before - error after < (before.length : Rat) := by
  have ha : after ≠ [] := by
    intro h
    subst after
    cases before <;> simp_all
  have h₁ := endpoint_dust before after hb
  have h₂ := endpoint_dust after before ha
  rw [← same] at h₂
  grind

/- If F = W-error, the discrepancy of any endpoint change is just the
   difference of two errors, regardless of the number of intervening steps. -/
theorem dust_identity (w0 w1 e0 e1 : Rat) :
    ((w1-e1)-(w0-e0))-(w1-w0) = e0-e1 := by grind

/- Signed per-operation discrepancies telescope to the endpoint error.
   This lemma does not discard negative intermediate discrepancies. -/
def signedExcess : List Rat → Rat
  | [] => 0
  | [_] => 0
  | a :: b :: xs => (a-b) + signedExcess (b :: xs)

theorem excess_telescope (e : Rat) (es : List Rat) :
    signedExcess (e :: es) = e - es.getLastD e := by
  induction es generalizing e with
  | nil => simp [signedExcess, List.getLastD, Rat.sub_self]
  | cons a xs ih =>
    cases xs with
    | nil => simp [signedExcess, List.getLastD, Rat.add_zero]
    | cons b ys =>
      rw [signedExcess, ih]
      simp only [List.getLastD_cons]
      grind

/- Closing LP positions restores supply because the outside shares q are fixed.
   This concerns cash balances, not a floored valuation of an open position. -/
theorem closed_liquid_no_profit (q : Rat) (a b : State)
    (va : Valid q a) (vb : Valid q b) (h : Trace q a b)
    (closed_supply : b.supply = a.supply) :
    ¬ (a.liquid0 ≤ b.liquid0 ∧ a.liquid1 ≤ b.liquid1 ∧
      (a.liquid0 < b.liquid0 ∨ a.liquid1 < b.liquid1)) := by
  intro ⟨h0, h1, hs⟩
  rcases trace_invariants q a b h with ⟨hp, t0, t1, o0, o1⟩
  unfold total0 at t0
  unfold total1 at t1
  rw [closed_supply] at t0 t1
  have hx : b.x ≤ a.x := by
    apply Rat.le_of_mul_le_mul_left (c := a.supply) _ va.2.2.1
    grind
  have hy : b.y ≤ a.y := by
    apply Rat.le_of_mul_le_mul_left (c := a.supply) _ va.2.2.1
    grind
  have hstrict : b.x < a.x ∨ b.y < a.y := by
    cases hs with
    | inl hs =>
      left
      apply Rat.lt_of_mul_lt_mul_left (c := a.supply) _ (Rat.le_of_lt va.2.2.1)
      grind
    | inr hs =>
      right
      apply Rat.lt_of_mul_lt_mul_left (c := a.supply) _ (Rat.le_of_lt va.2.2.1)
      grind
  have bad := product_strict a.x a.y b.x b.y va.1 vb.2.1 hx hy hstrict
  grind

/- Every account used in a finite strategy has a slot from the start, including
   initially empty accounts. n is arbitrary; it is not the native two-slot cap.
   Debt is accounting only: the native command surface has no borrowing route. -/
structure AccountPosition where
  shares : Rat
  cash0 : Rat
  cash1 : Rat
  debt0 : Rat
  debt1 : Rat

def accountSum {n : Nat} (book : Fin n → AccountPosition)
    (field : AccountPosition → Rat) : Rat := (List.ofFn fun i => field (book i)).sum

structure AccountsMatch {n : Nat} (q : Rat) (book : Fin n → AccountPosition)
    (s : State) : Prop where
  supply : s.supply = q + accountSum book AccountPosition.shares
  cash0 : s.liquid0 = accountSum book AccountPosition.cash0
  cash1 : s.liquid1 = accountSum book AccountPosition.cash1

/- Aggregate restoration suffices, so restoring each account separately is a
   special case. Equal debts make net-cash dominance equivalent to cash dominance.
   No desired wealth inequality is a premise of this theorem. -/
theorem finite_coalition_closed_no_profit {n : Nat} (q : Rat) (a b : State)
    (before after : Fin n → AccountPosition)
    (va : Valid q a) (vb : Valid q b) (h : Trace q a b)
    (ma : AccountsMatch q before a) (mb : AccountsMatch q after b)
    (shares_restored : accountSum before AccountPosition.shares =
      accountSum after AccountPosition.shares)
    (debt0_restored : accountSum before AccountPosition.debt0 =
      accountSum after AccountPosition.debt0)
    (debt1_restored : accountSum before AccountPosition.debt1 =
      accountSum after AccountPosition.debt1) :
    ¬ (accountSum before AccountPosition.cash0 - accountSum before AccountPosition.debt0 ≤
         accountSum after AccountPosition.cash0 - accountSum after AccountPosition.debt0 ∧
       accountSum before AccountPosition.cash1 - accountSum before AccountPosition.debt1 ≤
         accountSum after AccountPosition.cash1 - accountSum after AccountPosition.debt1 ∧
       (accountSum before AccountPosition.cash0 - accountSum before AccountPosition.debt0 <
          accountSum after AccountPosition.cash0 - accountSum after AccountPosition.debt0 ∨
        accountSum before AccountPosition.cash1 - accountSum before AccountPosition.debt1 <
          accountSum after AccountPosition.cash1 - accountSum after AccountPosition.debt1)) := by
  have supply_restored : b.supply = a.supply := by
    rw [ma.supply, mb.supply, shares_restored]
  have no_cash_gain := closed_liquid_no_profit q a b va vb h supply_restored
  rw [ma.cash0, mb.cash0, ma.cash1, mb.cash1] at no_cash_gain
  rw [debt0_restored, debt1_restored]
  grind

/- An absent-pool prefix pays for creation. Even without closing LP positions,
   final spendable cash cannot dominate the original cash endowment. -/
theorem creation_liquid_no_profit (q initial0 initial1 outside0 outside1 : Rat)
    (a b : State) (hq : 0 < q) (va : Valid q a) (vb : Valid q b)
    (h : Trace q a b)
    (c0 : initial0 + outside0 = total0 a) (c1 : initial1 + outside1 = total1 a)
    (o0 : outside0 ≤ a.outside0) (o1 : outside1 ≤ a.outside1) :
    ¬ (initial0 ≤ b.liquid0 ∧ initial1 ≤ b.liquid1 ∧
      (initial0 < b.liquid0 ∨ initial1 < b.liquid1)) := by
  have cost := creation_cost q initial0 initial1 outside0 outside1 a hq va c0 c1 o0 o1
  have no_gain := exact_no_free_value_from_creation q initial0 initial1 a b hq va vb h cost
  have h0 := Rat.mul_nonneg (show 0 ≤ b.supply-q by grind [Valid]) (Rat.le_of_lt vb.1)
  have h1 := Rat.mul_nonneg (show 0 ≤ b.supply-q by grind [Valid]) (Rat.le_of_lt vb.2.1)
  unfold wealth0 wealth1 at no_gain
  grind

#print axioms closed_liquid_no_profit
#print axioms finite_coalition_closed_no_profit
#print axioms creation_liquid_no_profit
#print axioms exact_no_free_value
#print axioms cumulative_floored_bound
#print axioms exact_no_free_value_from_creation
#print axioms endpoint_dust
#print axioms rounded_change_excess
#print axioms excess_telescope
end SpotExact
