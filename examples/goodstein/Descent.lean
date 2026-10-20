import Init

/- A general progress theorem, not a proof of Goodstein's ordinal assignment. -/
namespace TemporalProgress

def Eventually (p : Nat → Prop) : Prop := ∃ k, p k

theorem eventually_of_descent {α : Type} (before : α → α → Prop)
    (wf : WellFounded before) (rank : Nat → α) (done : Nat → Prop)
    (decreases : ∀ t, ¬ done t → before (rank (t + 1)) (rank t)) :
    Eventually done := by
  classical
  have reaches : ∀ a, ∀ t, rank t = a → ∃ k, t ≤ k ∧ done k := by
    intro a
    induction a using wf.induction with
    | h a ih =>
      intro t ht
      by_cases hd : done t
      · exact ⟨t, Nat.le_refl t, hd⟩
      · have hnext : before (rank (t + 1)) a := by
          rw [← ht]
          exact decreases t hd
        obtain ⟨k, hk, hdone⟩ := ih (rank (t + 1)) hnext (t + 1) rfl
        exact ⟨k, Nat.le_trans (Nat.le_succ t) hk, hdone⟩
  obtain ⟨k, _, hdone⟩ := reaches (rank 0) 0 rfl
  exact ⟨k, hdone⟩

-- An explicit instance with no assumed descent proof: ordinary countdown.
theorem countdown_eventually_zero (n : Nat) : Eventually (fun t => n - t = 0) := by
  exact ⟨n, Nat.sub_self n⟩

#print axioms eventually_of_descent
#print axioms countdown_eventually_zero

end TemporalProgress
