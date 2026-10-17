//! Shell preparation for automatic systemd admission. The status parser is
//! pure; the pipeline invokes one-shot shell acquisitions and owns its
//! diagnostic results. It supplies no semantic or publication authority.

#[derive(Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
pub(super) struct Evidence {
    pub(super) qualified: bool,
    checks: Vec<Check>,
}

#[derive(Debug, Eq, PartialEq, serde::Serialize, serde::Deserialize)]
struct Check {
    check: String,
    passed: bool,
    detail: String,
}

#[derive(Debug)]
pub(super) struct Refusal {
    pub(super) failed_check: String,
    pub(super) evidence: Evidence,
}

/// Only the two kernel fields named by the policy are interpreted. Each must
/// occur exactly once, have a colon, and contain one known decimal value.
fn status_field(text: &str, field: &str, name: &str, maximum: u8) -> Result<u8, String> {
    let mut value = None;
    for line in text.lines() {
        let (key, rest) = line.split_once(':').unwrap_or((line, ""));
        if key.trim() != field {
            if line.split_whitespace().next() == Some(field) {
                return Err(format!("{name}-malformed"));
            }
            continue;
        }
        if key != field || !line.contains(':') {
            return Err(format!("{name}-malformed"));
        }
        if value.is_some() {
            return Err(format!("{name}-duplicate"));
        }
        let number = match rest.trim() {
            "0" => 0,
            "1" => 1,
            "2" if maximum == 2 => 2,
            _ => return Err(format!("{name}-malformed")),
        };
        value = Some(number);
    }
    value.ok_or_else(|| format!("{name}-missing"))
}

fn clear_status_field(text: &str, field: &str, name: &str, maximum: u8) -> Result<u8, String> {
    let value = status_field(text, field, name, maximum)?;
    if value != 0 {
        return Err(name.to_owned());
    }
    Ok(value)
}

/// Fresh exclusive scratch, dropped after returning owned evidence. The
/// closures run once, in refusal order, and are never retained or exposed.
pub(super) fn qualify(
    manager: impl FnOnce() -> Result<(), String>,
    status: impl FnOnce() -> std::io::Result<String>,
    scheduling: impl FnOnce() -> Result<(String, i64), String>,
    quota: impl FnOnce() -> Result<u64, String>,
) -> Result<Evidence, Refusal> {
    fn record<T>(
        checks: &mut Vec<Check>,
        name: &str,
        result: Result<T, String>,
        describe: impl FnOnce(&T) -> String,
    ) -> Result<T, String> {
        let detail = match &result {
            Ok(value) => describe(value),
            Err(error) => error.clone(),
        };
        checks.push(Check {
            check: name.to_owned(),
            passed: result.is_ok(),
            detail,
        });
        result
    }

    let mut checks = Vec::new();
    let result = (|| {
        record(
            &mut checks,
            "systemd-user-manager",
            manager().map_err(|_| "systemd-user-manager".to_owned()),
            |_| "available".to_owned(),
        )?;
        let text = record(
            &mut checks,
            "status",
            status().map_err(|_| "status-unreadable".to_owned()),
            |_| "readable".to_owned(),
        )?;
        record(
            &mut checks,
            "no-new-privs",
            clear_status_field(&text, "NoNewPrivs", "no-new-privs", 1),
            u8::to_string,
        )?;
        record(
            &mut checks,
            "seccomp",
            clear_status_field(&text, "Seccomp", "seccomp", 2),
            u8::to_string,
        )?;
        record(
            &mut checks,
            "cpu-scheduling",
            scheduling().map_err(|_| "cpu-scheduling".to_owned()),
            |(cpus, nice)| format!("affinity={cpus};nice={nice}"),
        )?;
        record(
            &mut checks,
            "cpu-quota",
            quota().map_err(|_| "cpu-quota".to_owned()),
            u64::to_string,
        )?;
        Ok(())
    })();
    let evidence = Evidence {
        qualified: result.is_ok(),
        checks,
    };
    match result {
        Ok(()) => Ok(evidence),
        Err(failed_check) => Err(Refusal {
            failed_check,
            evidence,
        }),
    }
}

/// Operational evidence wraps the result; canonical semantic receipts and the
/// frozen containment-run/1 schema are untouched. Worker claims are replaced.
pub(super) fn label_output(
    bytes: &[u8],
    profile: super::Profile,
    evidence: &Evidence,
) -> Result<Vec<u8>, String> {
    let mut result: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|_| "containment-host-policy-output-not-json".to_owned())?;
    if !result.is_object() {
        return Err("containment-host-policy-output-not-object".to_owned());
    }
    result["backend"] = serde_json::json!("systemd");
    result["required_profile"] = serde_json::json!(profile);
    result["containment"] = serde_json::json!({
        "schema":"zeno-fcis/containment-result/1",
        "backend":"systemd", "required_profile":profile,
        "evidence":"inline-host-policy-checks", "host_policy":evidence
    });
    let mut bytes = serde_json::to_vec(&result).map_err(|e| e.to_string())?;
    bytes.push(b'\n');
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::super::{test_err, test_ok};
    use super::*;

    const CLEAR: &str = "Name:\tchecker\nNoNewPrivs:\t0\nSeccomp:\t0\nSeccomp_filters:\t0\n";

    fn evaluate(status: &str) -> Result<Evidence, Refusal> {
        qualify(
            || Ok(()),
            || Ok(status.to_owned()),
            || Ok(("0-3".to_owned(), 5)),
            || Ok(200),
        )
    }

    fn refused(result: Result<Evidence, Refusal>, expected: &str) {
        let refusal = test_err(result);
        assert_eq!(refusal.failed_check, expected);
        assert!(!refusal.evidence.qualified);
        assert!(refusal.evidence.checks.last().is_some_and(|c| !c.passed));
    }

    #[test]
    fn host_policy_records_each_successful_check() {
        let evidence = test_ok(evaluate(CLEAR));
        assert!(evidence.qualified);
        assert_eq!(evidence.checks.len(), 6);
        assert!(evidence.checks.iter().all(|c| c.passed));
        assert_eq!(evidence.checks[2].detail, "0");
        assert_eq!(evidence.checks[3].detail, "0");
        assert_eq!(evidence.checks[4].detail, "affinity=0-3;nice=5");
        assert_eq!(evidence.checks[5].detail, "200");
    }

    #[test]
    fn host_policy_requires_the_read_only_manager_probe() {
        refused(
            qualify(
                || Err("bus denied".to_owned()),
                || panic!("status read after manager refusal"),
                || panic!("scheduling read after refusal"),
                || panic!("quota read after refusal"),
            ),
            "systemd-user-manager",
        );
    }

    #[test]
    fn host_policy_requires_readable_status() {
        refused(
            qualify(
                || Ok(()),
                || Err(std::io::Error::other("denied")),
                || panic!("scheduling read after status refusal"),
                || panic!("quota read after refusal"),
            ),
            "status-unreadable",
        );
    }

    #[test]
    fn host_policy_refuses_no_new_privileges() {
        refused(evaluate("NoNewPrivs:\t1\nSeccomp:\t0\n"), "no-new-privs");
    }

    #[test]
    fn host_policy_refuses_both_seccomp_modes() {
        for mode in [1, 2] {
            refused(
                evaluate(&format!("NoNewPrivs:\t0\nSeccomp:\t{mode}\n")),
                "seccomp",
            );
        }
    }

    #[test]
    fn host_policy_refuses_missing_status_fields() {
        refused(evaluate("Seccomp:\t0\n"), "no-new-privs-missing");
        refused(evaluate("NoNewPrivs:\t0\n"), "seccomp-missing");
        refused(evaluate(""), "no-new-privs-missing");
    }

    #[test]
    fn host_policy_refuses_malformed_or_duplicate_status_fields() {
        for (field, name, other) in [
            ("NoNewPrivs", "no-new-privs", "Seccomp:\t0\n"),
            ("Seccomp", "seccomp", "NoNewPrivs:\t0\n"),
        ] {
            for value in ["", "+0", "00", "-1", "3", "256", "0 0", "zero", "0\0"] {
                refused(
                    evaluate(&format!("{other}{field}:\t{value}\n")),
                    &format!("{name}-malformed"),
                );
            }
            for line in [
                format!("{field} 0"),
                format!("{field}"),
                format!(" {field}: 0"),
            ] {
                refused(
                    evaluate(&format!("{other}{line}\n")),
                    &format!("{name}-malformed"),
                );
            }
            refused(
                evaluate(&format!("{other}{field}: 0\n{field}: 0\n")),
                &format!("{name}-duplicate"),
            );
        }
        refused(
            evaluate("NoNewPrivs: 2\nSeccomp: 0\n"),
            "no-new-privs-malformed",
        );
    }

    #[test]
    fn host_policy_requires_known_cpu_scheduling() {
        refused(
            qualify(
                || Ok(()),
                || Ok(CLEAR.to_owned()),
                || Err("unreadable affinity or nice".to_owned()),
                || panic!("quota read after scheduling refusal"),
            ),
            "cpu-scheduling",
        );
    }

    #[test]
    fn host_policy_requires_known_enclosing_cpu_quota() {
        refused(
            qualify(
                || Ok(()),
                || Ok(CLEAR.to_owned()),
                || Ok(("0".to_owned(), 0)),
                || Err("unknown ancestor quota".to_owned()),
            ),
            "cpu-quota",
        );
    }

    #[test]
    fn host_policy_is_deterministic_and_does_not_retain_input_aliases() {
        let mut status = CLEAR.to_owned();
        let evidence = test_ok(evaluate(&status));
        let before = test_ok(serde_json::to_vec(&evidence));
        status.clear();
        assert_eq!(test_ok(serde_json::to_vec(&evidence)), before);
        assert_eq!(
            test_ok(serde_json::to_vec(&test_ok(evaluate(CLEAR)))),
            before
        );
    }

    #[test]
    fn host_policy_result_uses_supervisor_evidence_and_preserves_semantic_fields() {
        let evidence = test_ok(evaluate(CLEAR));
        for profile in [
            super::super::Profile::PythonReference,
            super::super::Profile::GroupResources,
        ] {
            let input = br#"{"status":"equivalent","authority":"checker","receipt":{"sha256":"unchanged"},"containment":{"host_policy":{"qualified":false}},"backend":"forged"}"#;
            let bytes = test_ok(label_output(input, profile, &evidence));
            let output: serde_json::Value = test_ok(serde_json::from_slice(&bytes));
            assert_eq!(output["status"], "equivalent");
            assert_eq!(output["authority"], "checker");
            assert_eq!(output["receipt"]["sha256"], "unchanged");
            assert_eq!(output["backend"], "systemd");
            assert_eq!(output["required_profile"], serde_json::json!(profile));
            assert_eq!(
                output["containment"]["schema"],
                "zeno-fcis/containment-result/1"
            );
            assert_eq!(
                output["containment"]["host_policy"],
                serde_json::json!(evidence)
            );
            assert_eq!(test_ok(label_output(&bytes, profile, &evidence)), bytes);
        }
        for bytes in [b"null".as_slice(), b"[]", b"garbage", b"{}\n{}"] {
            assert!(label_output(bytes, super::super::Profile::GroupResources, &evidence).is_err());
        }
    }
}
