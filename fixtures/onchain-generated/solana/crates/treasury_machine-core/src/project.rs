use crate::{Command, Context, Core, Decision, State};

/// Fail-closed starter. Replace only this implementation and keep it pure.
pub struct ProjectCore;

impl Core for ProjectCore {
    fn command_admissible(_command: &Command, _context: &Context) -> bool { false }
    fn invariant(_state: &State) -> bool { false }
    fn decide(_state: &State, _command: &Command, _context: &Context) -> Decision { Decision::rejected(1) }
}
