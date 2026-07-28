//! A minimal pure transition using the ZenoFCIS decision algebra and logical budget.

use zeno_fcis::prelude::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Account {
    balance: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Withdraw {
    amount: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RejectReason {
    ResourceLimit,
    InsufficientFunds,
}

impl StableReason for RejectReason {
    fn code(&self) -> &'static str {
        match self {
            Self::ResourceLimit => "resource_limit",
            Self::InsufficientFunds => "insufficient_funds",
        }
    }

    fn precedence(&self) -> u16 {
        match self {
            Self::ResourceLimit => 0,
            Self::InsufficientFunds => 1,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CommittedFailureReason {}

impl StableReason for CommittedFailureReason {
    fn code(&self) -> &'static str {
        match *self {}
    }

    fn precedence(&self) -> u16 {
        match *self {}
    }
}

struct WithdrawTransition;

impl Transition for WithdrawTransition {
    type State = Account;
    type Command = Withdraw;
    type Context = ();
    type Candidate = Account;
    type Reject = RejectReason;
    type Failure = CommittedFailureReason;

    fn step(
        state: &Self::State,
        command: &Self::Command,
        _context: &Self::Context,
        limits: BudgetLimits,
    ) -> BudgetedDecision<Self::Candidate, Self::Reject, Self::Failure> {
        let mut budget = Budget::new(limits);

        if budget.charge(Resource::Read, 1).is_err() {
            return budget.finish(Decision::Reject(Rejected::new(RejectReason::ResourceLimit)));
        }
        if command.amount > state.balance {
            return budget.finish(Decision::Reject(Rejected::new(
                RejectReason::InsufficientFunds,
            )));
        }
        if budget.charge(Resource::Write, 1).is_err() {
            return budget.finish(Decision::Reject(Rejected::new(RejectReason::ResourceLimit)));
        }

        let candidate = Account {
            balance: state.balance - command.amount,
        };
        budget.finish(Decision::Accept(Accepted::new(candidate)))
    }
}

fn main() -> Result<(), &'static str> {
    let state = Account { balance: 100 };
    let limits = BudgetLimits::zero()
        .with_limit(Resource::Read, 1)
        .with_limit(Resource::Write, 1);

    let result = WithdrawTransition::step(&state, &Withdraw { amount: 40 }, &(), limits);

    match result.decision() {
        Decision::Accept(accepted) if accepted.candidate().balance == 60 => {}
        Decision::Accept(_) => return Err("unexpected accepted candidate"),
        Decision::Reject(_) => return Err("unexpected rejection"),
        Decision::CommittedFailure(_) => {
            return Err("this transition defines no committed failure");
        }
    }
    if state.balance != 100 {
        return Err("the immutable pre-state changed");
    }
    if result.used().used(Resource::Read) != 1 || result.used().used(Resource::Write) != 1 {
        return Err("unexpected logical resource report");
    }

    Ok(())
}
