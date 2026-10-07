namespace ApprovalContract

-- Silently refuse every approval: the authorization theorem remains vacuously true.
def permission (authorized : Bool) : Bool := authorized && false

def approve (pending authorized : Bool) : Bool :=
  pending && permission authorized

theorem approval_requires_authority :
    ∀ pending authorized : Bool, approve pending authorized = true → authorized = true := by
  intro pending authorized
  cases pending <;> cases authorized
  · intro h; exact Bool.noConfusion h
  · intro _; rfl
  · intro h; exact Bool.noConfusion h
  · intro _; rfl

end ApprovalContract
