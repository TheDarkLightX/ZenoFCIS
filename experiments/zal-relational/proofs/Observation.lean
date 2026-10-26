import Std.Tactic

namespace Zal.Observation

-- These are abstract natural-number laws. No Haskell extraction is asserted.
theorem saturation_add (a b cutoff : Nat) :
    min (a + b) cutoff = min (min a cutoff + min b cutoff) cutoff := by
  omega

theorem remainder_add (a b modulus : Nat) :
    ((a % modulus) + (b % modulus)) % modulus = (a + b) % modulus := by
  exact (Nat.add_mod a b modulus).symm

theorem exact_observation (a k : Nat) :
    min a (k + 1) = k ↔ a = k := by
  omega

theorem at_most_observation (a k : Nat) :
    min a (k + 1) ≤ k ↔ a ≤ k := by
  omega

theorem at_least_observation (a k : Nat) :
    k ≤ min a k ↔ k ≤ a := by
  omega

theorem strict_composition (a b c cap : Nat) (h : a + b + c ≤ cap) :
    a + b ≤ cap ∧ (a + b) + c = a + (b + c) := by
  omega

#print axioms saturation_add
#print axioms remainder_add
#print axioms exact_observation
#print axioms at_most_observation
#print axioms at_least_observation
#print axioms strict_composition

end Zal.Observation
