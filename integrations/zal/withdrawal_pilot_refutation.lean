/-- Planted-defect control: replaces the liveness section of a model whose
alarm budget is resettable. Request lane A, then tick with the alarm forever:
the run is fair, yet lane A is never paid, so the exported claim fails. This
file is expected to fail on the real contract's model. -/
def frozenRun : Nat → Act
  | 0 => .request .a
  | _ + 1 => .tick true

theorem frozen_fair : Fair frozenRun := fun n => ⟨n + 1, by omega, rfl⟩

def frozen : Ctrl := ⟨.pending, .empty, .p2, false, .a⟩

theorem frozen_state : ∀ n, ctrlAt frozenRun (n + 2) = frozen
  | 0 => by decide +kernel
  | n + 1 => by
    show ctrlAt frozenRun (n + 2 + 1) = frozen
    rw [ctrlAt_succ, frozen_state n]
    rfl

theorem frozen_unpaid : ∀ m, (obsAt frozenRun m).ca = false
  | 0 => by decide +kernel
  | 1 => by decide +kernel
  | n + 2 => by
    show (dstep (ctrlAt frozenRun (n + 2)) (frozenRun (n + 2))).2.ca = false
    rw [frozen_state n]
    rfl

theorem withdrawal_freezes :
    ¬ ∀ act, Fair act → ∀ preds : String → List Int → Prop, claim_2 (observeRun act) preds := by
  intro h
  have hc := h frozenRun frozen_fair (fun _ _ => True)
  unfold claim_2 at hc
  rcases (hc.2 0 (Nat.zero_le 0)).1 with hn | ⟨t, _, hp⟩
  · exact hn (by simp [observeRun, bit_one]; decide +kernel)
  · have hp' : (obsAt frozenRun t).ca = true := by simpa [observeRun, bit_one] using hp
    rw [frozen_unpaid t] at hp'
    exact absurd hp' (by decide)

end WithdrawalPilot

#print axioms WithdrawalPilot.withdrawal_freezes

end ZenoFCIS
