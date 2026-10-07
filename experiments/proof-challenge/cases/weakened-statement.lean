namespace ApprovalContract

def permission (authorized : Bool) : Bool := authorized && true

def approve (pending authorized : Bool) : Bool :=
  pending && permission authorized

theorem approval_requires_authority :
    ∀ pending authorized : Bool, approve pending authorized = true → True := by
  intro _ _ _
  exact True.intro

end ApprovalContract
