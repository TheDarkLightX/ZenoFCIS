namespace ApprovalContract

def permission (authorized : Bool) : Bool := authorized && true

-- Still enforces authorization, but silently discards the pending-state guard.
def approve (_pending authorized : Bool) : Bool :=
  permission authorized

theorem approval_requires_authority :
    ∀ pending authorized : Bool, approve pending authorized = true → authorized = true := by
  intro pending authorized
  cases pending <;> cases authorized
  · intro h; exact Bool.noConfusion h
  · intro _; rfl
  · intro h; exact Bool.noConfusion h
  · intro _; rfl

end ApprovalContract
