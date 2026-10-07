//! Retained independent bounded decision table for the explicit legacy oracle.
use durable_counter::legacy::{authority, bindings::GeneratedProject, generated::*, profile};
use zeno_fcis_core::Decision;
use zeno_fcis_crypto::RustCryptoSha256;
use zeno_fcis_schema::ValidationLimits;

#[test]
fn every_bounded_input_obeys_the_independent_decision_table() {
    let authority = authority().unwrap();
    let project = GeneratedProject::try_new::<RustCryptoSha256>().unwrap();
    for count in 0..=3 {
        for failures in 0..=3 {
            for command in [CounterCommand::Increment, CounterCommand::RecordFailure] {
                for allowed in [false, true] {
                    let pre = project
                        .admit_root::<RustCryptoSha256>(
                            &CounterState {
                                count: CounterValue(count),
                                failures: CounterValue(failures),
                            },
                            ValidationLimits::default(),
                        )
                        .unwrap();
                    let cmd = project
                        .admit_command::<RustCryptoSha256>(&command, ValidationLimits::default())
                        .unwrap();
                    let ctx = project
                        .admit_context::<RustCryptoSha256>(
                            &CounterContext(allowed),
                            ValidationLimits::default(),
                        )
                        .unwrap();
                    let binding = profile::digest(
                        "example/test/input",
                        format!("{count},{failures},{command:?},{allowed}").as_bytes(),
                    );
                    let invocation = authority
                        .admit_invocation(
                            pre,
                            cmd.admitted().clone(),
                            ctx.admitted().clone(),
                            binding,
                            binding,
                            binding,
                        )
                        .unwrap();
                    let selected = match command {
                        CounterCommand::Increment => count,
                        CounterCommand::RecordFailure => failures,
                    };
                    let decision = authority.execute(invocation).unwrap();
                    match decision {
                        Decision::Reject(reject) => {
                            assert!(!allowed || selected == 3);
                            assert_eq!(
                                reject.reason().rejection().reason_id().get(),
                                if allowed { 201 } else { 200 }
                            );
                        }
                        Decision::Accept(_) => {
                            assert!(allowed && count < 3);
                            assert!(matches!(command, CounterCommand::Increment));
                        }
                        Decision::CommittedFailure(failure) => {
                            assert!(allowed && failures < 3);
                            assert!(matches!(command, CounterCommand::RecordFailure));
                            assert_eq!(failure.reason().get(), 202);
                        }
                    }
                }
            }
        }
    }
}
