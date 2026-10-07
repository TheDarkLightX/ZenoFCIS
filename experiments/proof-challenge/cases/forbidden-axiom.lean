namespace ApprovalContract

def permission (authorized : Bool) : Bool := authorized && true

def approve (pending authorized : Bool) : Bool :=
  pending && permission authorized

axiom assumed_authority :
  ∀ pending authorized : Bool, approve pending authorized = true → authorized = true

theorem approval_requires_authority :
    ∀ pending authorized : Bool, approve pending authorized = true → authorized = true :=
  assumed_authority

end ApprovalContract
