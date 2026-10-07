namespace ApprovalContract

def permission (authorized : Bool) : Bool := authorized && true

def approve (pending authorized : Bool) : Bool :=
  pending && permission authorized

theorem approval_requires_authority :
    ∀ pending authorized : Bool, approve pending authorized = true → authorized = true := by
  intro pending authorized h
  cases authorized with
  | false =>
    cases pending <;> exact Bool.noConfusion h
  | true => rfl

end ApprovalContract
