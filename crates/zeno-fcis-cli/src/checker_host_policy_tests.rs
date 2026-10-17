use super::super::{test_err, test_ok};
use super::context::{self, CpuQuota, Observations, Scheduler};
use super::*;

fn evaluate(observations: Observations) -> Result<Evidence, Refusal> {
    qualify(
        || Ok(()),
        || Ok("NoNewPrivs: 0\nSeccomp: 0\n".into()),
        || Ok(("0-3".into(), 0)),
        || {
            Ok(CpuQuota {
                quota: 50000,
                period: 100000,
            })
        },
        || Ok(observations),
    )
}
fn refused(result: Result<Evidence, Refusal>, reason: &str) {
    let refusal = test_err(result);
    assert_eq!(refusal.failed_check, reason);
    assert!(!refusal.evidence.qualified);
    assert!(refusal.evidence.checks.last().is_some_and(|c| !c.passed));
}
fn field(stat: &str, index: usize, value: &str) -> String {
    let (head, tail) = test_ok(stat.rsplit_once(')').ok_or("stat fixture"));
    let mut fields: Vec<_> = tail.split_whitespace().collect();
    fields[index] = value;
    format!("{head}) {}", fields.join(" "))
}

#[test]
fn host_policy_requires_readable_observations() {
    for reason in [
        "manager-pid-unavailable",
        "namespace-mnt-unreadable",
        "lsm-label-unreadable",
        "rlimits-unreadable",
        "memory.max-unreadable",
        "cpu-scheduler-unreadable",
    ] {
        refused(
            qualify(
                || Ok(()),
                || Ok("NoNewPrivs: 0\nSeccomp: 0\n".into()),
                || Ok(("0".into(), 0)),
                || {
                    Ok(CpuQuota {
                        quota: 50000,
                        period: 100000,
                    })
                },
                || Err(reason.into()),
            ),
            reason,
        );
    }
}

#[test]
fn host_policy_requires_a_valid_manager_pid() {
    for response in [
        "",
        "42",
        "s 42",
        "u 0",
        "u -1",
        "u 4294967296",
        "u 42 extra",
        "u bad",
    ] {
        let mut obs = context::fixture();
        obs.manager_pid = response.into();
        refused(evaluate(obs), "manager-pid-malformed");
    }
}

#[test]
fn host_policy_rejects_manager_pid_reuse() {
    let mut obs = context::fixture();
    obs.manager_stat.1 = field(&obs.manager_stat.1, 19, "124");
    refused(evaluate(obs), "manager-identity-changed");
    for malformed in ["", "bad stat", "42 (manager) S"] {
        let mut obs = context::fixture();
        obs.manager_stat.0 = malformed.into();
        assert!(!test_err(evaluate(obs)).evidence.qualified);
    }
}

#[test]
fn host_policy_rejects_each_namespace_mismatch() {
    for (index, name) in context::NAMESPACES.iter().enumerate() {
        let mut obs = context::fixture();
        obs.namespaces[index].1 = format!("{name}:[9999]");
        refused(evaluate(obs), &format!("namespace-{name}-mismatch"));
    }
}

#[test]
fn host_policy_rejects_malformed_namespace_observations() {
    for (index, name) in context::NAMESPACES.iter().enumerate() {
        for malformed in [
            "",
            "other:[1]",
            "mnt:[0]",
            "1",
            "mnt:[bad]",
            "mnt:[18446744073709551616]",
        ] {
            let mut obs = context::fixture();
            obs.namespaces[index].0 = malformed.into();
            refused(evaluate(obs), &format!("namespace-{name}-malformed"));
        }
    }
}

#[test]
fn host_policy_rejects_lsm_label_mismatch() {
    let mut obs = context::fixture();
    obs.labels.0 = "restricted (enforce)\n".into();
    refused(evaluate(obs), "lsm-label-mismatch");
}

#[test]
fn host_policy_rejects_malformed_lsm_labels() {
    for bad in ["", "\n", "bad\0label", "two\nlabels"] {
        for side in [false, true] {
            let mut obs = context::fixture();
            if side {
                obs.labels.0 = bad.into();
            } else {
                obs.labels.1 = bad.into();
            }
            refused(evaluate(obs), "lsm-label-malformed");
        }
    }
}

#[test]
fn host_policy_rejects_noninitial_cgroup_namespace_root_at_fifty_percent() {
    let mut obs = context::fixture();
    obs.cgroup_path = "/".into();
    obs.namespaces[6] = ("cgroup:[9999]".into(), "cgroup:[9999]".into());
    refused(evaluate(obs), "cgroup-namespace-root");
    for path in ["", "relative", "/a/../b", "/a/./b", "/a//b", "/a\n"] {
        let mut obs = context::fixture();
        obs.cgroup_path = path.into();
        refused(evaluate(obs), "cgroup-path-malformed");
    }
}

#[test]
fn host_policy_rejects_a_cgroup_only_thirty_two_mib_scope() {
    let mut obs = context::fixture();
    obs.memory = vec![Some("max".into()), Some("33554432".into())];
    refused(evaluate(obs), "memory.max-tighter-than-unit");
}

#[test]
fn host_policy_rejects_a_cgroup_only_eight_task_scope() {
    let mut obs = context::fixture();
    obs.tasks = vec![Some("max".into()), Some("8".into())];
    refused(evaluate(obs), "pids.max-tighter-than-unit");
}

#[test]
fn host_policy_requires_observed_valid_swap_limits() {
    for values in [
        vec![],
        vec![None],
        vec![Some("-1".into())],
        vec![Some("unlimited".into())],
    ] {
        let mut obs = context::fixture();
        obs.swap = values;
        assert!(!test_err(evaluate(obs)).evidence.qualified);
    }
}

#[test]
fn host_policy_rejects_missing_or_malformed_cgroup_budgets() {
    for kind in ["memory.max", "pids.max", "memory.swap.max"] {
        for bad in [
            "",
            "unlimited",
            "-1",
            "+64",
            "64 64",
            "18446744073709551616",
        ] {
            let mut obs = context::fixture();
            let data = vec![Some(bad.into())];
            match kind {
                "memory.max" => obs.memory = data,
                "pids.max" => obs.tasks = data,
                _ => obs.swap = data,
            }
            refused(evaluate(obs), &format!("{kind}-malformed"));
        }
        let mut obs = context::fixture();
        match kind {
            "memory.max" => obs.memory = vec![None],
            "pids.max" => obs.tasks = vec![None],
            _ => obs.swap = vec![None],
        }
        refused(evaluate(obs), &format!("{kind}-unobserved"));
    }
}

#[test]
fn host_policy_carries_tighter_caller_limits_and_rejects_looser_readback() {
    let fixture = context::fixture();
    for line in fixture.caller_limits.lines().skip(1) {
        let replace_pair = |pair| {
            line.replace("unlimited unlimited", pair)
                .replace("40 40", pair)
        };
        for changed in ["1 2", "1 3"] {
            let mut obs = fixture.clone();
            obs.caller_limits = obs.caller_limits.replace(line, &replace_pair(changed));
            let caller = obs.caller_limits.clone();
            assert!(test_ok(evaluate(obs)).qualified);
            assert_eq!(test_ok(context::limit_properties(&caller)).len(), 16);
            assert_eq!(
                context::verify_service_limits("automatic", Some(&caller), &caller),
                Ok(())
            );
            // Unit defaults are looser until the carry makes them equal.
            assert!(
                context::verify_service_limits("automatic", Some(&caller), &fixture.caller_limits)
                    .is_err(),
                "{line}"
            );
        }
        let caller = fixture.caller_limits.replace(line, &replace_pair("1 2"));
        for pair in ["1 3", "2 2", "1 unlimited", "unlimited unlimited"] {
            let service = fixture.caller_limits.replace(line, &replace_pair(pair));
            assert!(
                context::verify_service_limits("automatic", Some(&caller), &service).is_err(),
                "must reject {line} carried as {pair}"
            );
        }
        let stricter = fixture.caller_limits.replace(line, &replace_pair("0 1"));
        assert_eq!(
            context::verify_service_limits("automatic", Some(&caller), &stricter),
            Ok(())
        );
    }
}

#[test]
fn host_policy_rejects_malformed_rlimits() {
    for bad in [
        "",
        "Limit Soft Limit Hard Limit Units\n",
        "unknown data",
        "Limit Soft Limit Hard Limit Units\nUnknown limit 1 2 bytes\n",
    ] {
        let mut obs = context::fixture();
        obs.caller_limits = bad.into();
        refused(evaluate(obs), "rlimits-malformed");
    }
    for replacement in ["2 1", "oops unlimited", "unlimited -1", "max unlimited"] {
        let mut obs = context::fixture();
        obs.caller_limits = obs
            .caller_limits
            .replacen("unlimited unlimited", replacement, 1);
        refused(evaluate(obs), "rlimits-malformed");
    }
    let mut obs = context::fixture();
    obs.caller_limits
        .push_str("Max cpu time unlimited unlimited seconds\n");
    refused(evaluate(obs), "rlimits-malformed");
}

#[test]
fn host_policy_carries_sched_idle_and_refuses_other_readback() {
    let mut obs = context::fixture();
    obs.scheduler_stat = field(&obs.scheduler_stat, 38, "5");
    let idle = test_ok(Scheduler::read(&obs.scheduler_stat));
    assert_eq!(
        idle.properties(),
        [
            "--property=CPUSchedulingPolicy=idle",
            "--property=CPUSchedulingPriority=0"
        ]
    );
    let result = test_ok(evaluate(obs));
    assert_eq!(
        result.checks.last().map(|c| c.detail.as_str()),
        Some("idle:0")
    );
    assert_eq!(
        idle.verify(Scheduler {
            policy: 0,
            priority: 0
        }),
        Err("cpu-scheduler-readback-mismatch".into())
    );
    assert_eq!(idle.verify(idle), Ok(()));
}

#[test]
fn host_policy_carries_scheduler_priority_and_checks_its_readback() {
    let fifo = test_ok(Scheduler::parse("1", "7"));
    assert_eq!(
        fifo.properties(),
        [
            "--property=CPUSchedulingPolicy=fifo",
            "--property=CPUSchedulingPriority=7"
        ]
    );
    assert!(
        fifo.verify(Scheduler {
            policy: 1,
            priority: 6
        })
        .is_err()
    );
    let mut obs = context::fixture();
    obs.scheduler_stat = field(&field(&obs.scheduler_stat, 38, "1"), 37, "7");
    assert!(test_ok(evaluate(obs)).qualified);
}

#[test]
fn host_policy_rejects_malformed_or_unsupported_scheduler() {
    for (policy, priority) in [
        ("bad", "0"),
        ("6", "0"),
        ("4", "0"),
        ("0", "1"),
        ("1", "0"),
        ("2", "100"),
        ("-1", "0"),
    ] {
        let mut obs = context::fixture();
        obs.scheduler_stat = field(&field(&obs.scheduler_stat, 38, policy), 37, priority);
        assert!(!test_err(evaluate(obs)).evidence.qualified);
    }
    let mut obs = context::fixture();
    obs.scheduler_stat = "missing fields".into();
    assert!(!test_err(evaluate(obs)).evidence.qualified);
}

struct QuotaFixture(std::path::PathBuf);
impl QuotaFixture {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "zeno-policy-quota-{}-{}",
            std::process::id(),
            crate::TEMP_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        test_ok(std::fs::create_dir(&p));
        test_ok(std::fs::write(p.join("cgroup.procs"), ""));
        Self(p)
    }
    fn evaluate(&self) -> Result<Evidence, Refusal> {
        qualify(
            || Ok(()),
            || Ok("NoNewPrivs: 0\nSeccomp: 0\n".into()),
            || Ok(("0".into(), 0)),
            || context::chain_quota(&self.0, self.0.clone(), 4),
            || Ok(context::fixture()),
        )
    }
}
impl Drop for QuotaFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn host_policy_reads_root_cpu_quota_instead_of_default() {
    let fixture = QuotaFixture::new();
    test_ok(std::fs::write(fixture.0.join("cpu.max"), "50000 100000"));
    let evidence = test_ok(fixture.evaluate());
    assert_eq!(evidence.checks[5].detail, "50000/100000");
}

#[test]
fn host_policy_requires_an_observed_cpu_limit() {
    let fixture = QuotaFixture::new();
    refused(fixture.evaluate(), "cpu-quota");
    for bad in [
        "",
        "max",
        "max 0",
        "0 100000",
        "50000 0",
        "50000 bad",
        "50000 100000 extra",
    ] {
        test_ok(std::fs::write(fixture.0.join("cpu.max"), bad));
        refused(fixture.evaluate(), "cpu-quota");
    }
}

#[test]
fn host_policy_preserves_tightest_cpu_ratio_and_its_period() {
    let fixture = QuotaFixture::new();
    let leaf = fixture.0.join("leaf");
    test_ok(std::fs::create_dir(&leaf));
    test_ok(std::fs::write(leaf.join("cgroup.procs"), ""));
    test_ok(std::fs::write(fixture.0.join("cpu.max"), "50000 100000"));
    test_ok(std::fs::write(leaf.join("cpu.max"), "100000 400000"));
    let quota = test_ok(context::chain_quota(&fixture.0, leaf, 4));
    assert_eq!(
        quota,
        CpuQuota {
            quota: 100000,
            period: 400000
        }
    );
    assert_eq!(
        test_ok(quota.properties()),
        [
            "--property=CPUQuota=25%",
            "--property=CPUQuotaPeriodSec=400000us"
        ]
    );
    assert!(quota.verify("100000 100000").is_err());
    assert!(quota.verify("200000 400000").is_err());
    assert_eq!(quota.verify("100000 400000"), Ok(()));
}

#[test]
fn host_policy_records_aggregate_and_privileged_landlock_residuals() {
    let evidence = test_ok(evaluate(context::fixture()));
    let result: serde_json::Value = test_ok(serde_json::from_slice(&test_ok(label_output(
        b"{}",
        super::super::Profile::GroupResources,
        &evidence,
    ))));
    assert_eq!(
        result["containment"]["residuals"],
        serde_json::json!(context::RESIDUALS)
    );
    assert!(context::RESIDUALS[0].contains("aggregate-cgroup-budget"));
    assert!(context::RESIDUALS[1].contains("landlock"));
}

#[test]
fn host_policy_observes_sched_idle_from_a_disposable_real_process() {
    let output = test_ok(std::process::Command::new("/usr/bin/python3")
        .args(["-I", "-S", "-B", "-c", "import os; os.sched_setscheduler(0, os.SCHED_IDLE, os.sched_param(0)); print(open('/proc/self/stat').read())"])
        .output());
    assert!(output.status.success(), "{:?}", output);
    let stat = test_ok(String::from_utf8(output.stdout));
    let observed = test_ok(Scheduler::read(&stat));
    assert_eq!(
        observed,
        Scheduler {
            policy: 5,
            priority: 0
        }
    );
    let mut context = context::fixture();
    context.scheduler_stat = stat;
    assert!(test_ok(evaluate(context)).qualified);
    assert_eq!(
        observed.properties()[0],
        "--property=CPUSchedulingPolicy=idle"
    );
    assert!(
        observed
            .verify(Scheduler {
                policy: 0,
                priority: 0
            })
            .is_err()
    );
}

fn acquisition_fixture(
    root: &std::path::Path,
) -> (std::path::PathBuf, std::path::PathBuf, std::path::PathBuf) {
    let obs = context::fixture();
    let proc_root = root.join("proc");
    let groups = root.join("groups");
    let leaf = groups.join("parent/leaf");
    for at in [&groups, &groups.join("parent"), &leaf] {
        test_ok(std::fs::create_dir_all(at));
        for (name, value) in [
            ("cgroup.procs", ""),
            ("memory.max", "67108864"),
            ("memory.swap.max", "0"),
            ("pids.max", "64"),
        ] {
            test_ok(std::fs::write(at.join(name), value));
        }
    }
    for (which, stat, limits, label) in [
        (
            "self",
            &obs.scheduler_stat,
            &obs.caller_limits,
            &obs.labels.0,
        ),
        ("42", &obs.manager_stat.0, &obs.caller_limits, &obs.labels.1),
    ] {
        let dir = proc_root.join(which);
        test_ok(std::fs::create_dir_all(dir.join("ns")));
        test_ok(std::fs::create_dir(dir.join("attr")));
        for (name, pair) in context::NAMESPACES.iter().zip(&obs.namespaces) {
            test_ok(std::os::unix::fs::symlink(
                &pair.0,
                dir.join("ns").join(name),
            ));
        }
        test_ok(std::fs::write(dir.join("stat"), stat));
        test_ok(std::fs::write(dir.join("limits"), limits));
        test_ok(std::fs::write(dir.join("attr/current"), label));
    }
    (proc_root, groups, leaf)
}

fn acquire_evaluate(
    proc_root: &std::path::Path,
    root: &std::path::Path,
    leaf: std::path::PathBuf,
) -> Result<Evidence, Refusal> {
    qualify(
        || Ok(()),
        || Ok("NoNewPrivs: 0\nSeccomp: 0\n".into()),
        || Ok(("0".into(), 0)),
        || {
            Ok(CpuQuota {
                quota: 50000,
                period: 100000,
            })
        },
        || context::acquire("u 42".into(), proc_root, root, leaf),
    )
}

#[test]
fn host_policy_reads_all_cgroup_resource_ancestors() {
    let fixture = QuotaFixture::new();
    let (proc_root, root, leaf) = acquisition_fixture(&fixture.0);
    assert!(test_ok(acquire_evaluate(&proc_root, &root, leaf.clone())).qualified);
    test_ok(std::fs::write(root.join("parent/memory.max"), "33554432"));
    refused(
        acquire_evaluate(&proc_root, &root, leaf.clone()),
        "memory.max-tighter-than-unit",
    );
    test_ok(std::fs::write(root.join("parent/memory.max"), "67108864"));
    test_ok(std::fs::write(root.join("parent/pids.max"), "8"));
    refused(
        acquire_evaluate(&proc_root, &root, leaf.clone()),
        "pids.max-tighter-than-unit",
    );
    test_ok(std::fs::write(root.join("parent/pids.max"), "64"));
    test_ok(std::fs::write(root.join("memory.max"), "33554432"));
    refused(
        acquire_evaluate(&proc_root, &root, leaf),
        "memory.max-tighter-than-unit",
    );
}

#[test]
fn host_policy_acquisition_refuses_unreadable_proc_and_resource_fields() {
    let fixture = QuotaFixture::new();
    let (proc_root, root, leaf) = acquisition_fixture(&fixture.0);
    for (path, reason) in [
        (leaf.join("memory.max"), "memory.max-unreadable"),
        (proc_root.join("self/attr/current"), "lsm-label-unreadable"),
        (proc_root.join("self/limits"), "rlimits-unreadable"),
        (proc_root.join("42/stat"), "manager-identity-unreadable"),
    ] {
        let bytes = test_ok(std::fs::read(&path));
        test_ok(std::fs::remove_file(&path));
        refused(acquire_evaluate(&proc_root, &root, leaf.clone()), reason);
        test_ok(std::fs::write(&path, bytes));
    }
    test_ok(std::fs::remove_file(proc_root.join("self/ns/mnt")));
    refused(
        acquire_evaluate(&proc_root, &root, leaf),
        "namespace-mnt-unreadable",
    );
}

#[test]
fn host_policy_observes_the_fixed_seven_namespace_names() {
    assert_eq!(
        context::NAMESPACES,
        ["mnt", "user", "pid", "net", "ipc", "uts", "cgroup"]
    );
    let evidence = test_ok(evaluate(context::fixture()));
    for name in ["mnt", "user", "pid", "net", "ipc", "uts", "cgroup"] {
        assert_eq!(
            evidence
                .checks
                .iter()
                .filter(|c| c.check == format!("namespace-{name}") && c.passed)
                .count(),
            1
        );
    }
}

#[test]
fn host_policy_checks_service_rlimits_on_readback() {
    let obs = context::fixture();
    let caller = obs.caller_limits.replacen(
        "Max open files unlimited unlimited files",
        "Max open files 2048 1048576 files",
        1,
    );
    let stricter = caller.replace("Max open files 2048", "Max open files 1024");
    let service = caller.replace("Max open files 2048", "Max open files 65536");
    assert_eq!(
        context::verify_service_limits("automatic", Some(&caller), &service),
        Err("rlimits-readback-looser:LimitNOFILE:soft".into())
    );
    assert_eq!(
        context::verify_service_limits("automatic", Some(&caller), &stricter),
        Ok(())
    );
    assert_eq!(
        context::verify_service_limits("automatic", Some(&caller), &caller),
        Ok(())
    );
    assert_eq!(
        context::verify_service_limits("explicit", None, &service),
        Ok(())
    );
}

#[test]
fn host_policy_refuses_unknown_or_missing_service_rlimit_readback() {
    let obs = context::fixture();
    assert!(context::verify_service_limits("automatic", None, &obs.caller_limits).is_err());
    for bad in [
        "malformed".to_owned(),
        obs.caller_limits
            .replacen("Max open files", "Unknown limit", 1),
        obs.caller_limits
            .replacen("Max open files unlimited unlimited files\n", "", 1),
    ] {
        assert_eq!(
            context::verify_service_limits("automatic", Some(&obs.caller_limits), &bad),
            Err("rlimits-readback-malformed".into())
        );
    }
    for mode in ["", "unknown", "Automatic"] {
        assert!(
            context::verify_service_limits(mode, Some(&obs.caller_limits), &obs.caller_limits)
                .is_err()
        );
    }
}

#[test]
fn host_policy_admits_ordinary_unit_defaults_despite_manager_nofile_raise() {
    let fixture = QuotaFixture::new();
    let (proc_root, root, leaf) = acquisition_fixture(&fixture.0);
    let base = context::fixture().caller_limits;
    let caller = base.replace(
        "Max open files unlimited unlimited files",
        "Max open files 1024 1048576 files",
    );
    let manager = base.replace(
        "Max open files unlimited unlimited files",
        "Max open files 1048576 1048576 files",
    );
    test_ok(std::fs::write(proc_root.join("self/limits"), &caller));
    test_ok(std::fs::write(proc_root.join("42/limits"), manager));
    assert!(test_ok(acquire_evaluate(&proc_root, &root, leaf.clone())).qualified);
    let properties = test_ok(context::limit_properties(&caller));
    assert!(properties.contains(&"--property=LimitNOFILE=1024:1048576".into()));
    let unit_default = &caller;
    assert_eq!(
        context::verify_service_limits("automatic", Some(&caller), unit_default),
        Ok(())
    );
    // The manager's process limit table is not needed, even if unreadable.
    test_ok(std::fs::remove_file(proc_root.join("42/limits")));
    assert!(test_ok(acquire_evaluate(&proc_root, &root, leaf)).qualified);
}

#[test]
fn host_policy_carries_all_sixteen_limits_exactly() {
    let caller = context::fixture()
        .caller_limits
        .replace(
            "Max cpu time unlimited unlimited seconds",
            "Max cpu time 1 2 seconds",
        )
        .replace(
            "Max stack size unlimited unlimited bytes",
            "Max stack size 8388608 unlimited bytes",
        )
        .replace(
            "Max core file size unlimited unlimited bytes",
            "Max core file size 0 unlimited bytes",
        )
        .replace(
            "Max open files unlimited unlimited files",
            "Max open files 1024 1048576 files",
        )
        .replace("Max nice priority 40 40", "Max nice priority 0 40")
        .replace(
            "Max realtime timeout unlimited unlimited us",
            "Max realtime timeout 1001 2002 us",
        );
    assert_eq!(
        test_ok(context::limit_properties(&caller)),
        [
            "--property=LimitCPU=1:2",
            "--property=LimitFSIZE=infinity:infinity",
            "--property=LimitDATA=infinity:infinity",
            "--property=LimitSTACK=8388608:infinity",
            "--property=LimitCORE=0:infinity",
            "--property=LimitRSS=infinity:infinity",
            "--property=LimitNPROC=infinity:infinity",
            "--property=LimitNOFILE=1024:1048576",
            "--property=LimitMEMLOCK=infinity:infinity",
            "--property=LimitAS=infinity:infinity",
            "--property=LimitLOCKS=infinity:infinity",
            "--property=LimitSIGPENDING=infinity:infinity",
            "--property=LimitMSGQUEUE=infinity:infinity",
            "--property=LimitNICE=0:40",
            "--property=LimitRTPRIO=infinity:infinity",
            "--property=LimitRTTIME=1001:2002"
        ]
    );
    assert_eq!(
        context::verify_service_limits("automatic", Some(&caller), &caller),
        Ok(())
    );
}

#[test]
fn host_policy_refuses_unrepresentable_limit_values_precisely() {
    for (from, to, property) in [
        (
            "Max cpu time unlimited unlimited seconds",
            "Max cpu time 18446744073709 18446744073709 seconds",
            "LimitCPU",
        ),
        (
            "Max realtime timeout unlimited unlimited us",
            "Max realtime timeout 9223372036854775808 9223372036854775808 us",
            "LimitRTTIME",
        ),
        (
            "Max nice priority 40 40",
            "Max nice priority 41 41",
            "LimitNICE",
        ),
        (
            "Max nice priority 40 40",
            "Max nice priority unlimited unlimited",
            "LimitNICE",
        ),
        (
            "Max file size unlimited unlimited bytes",
            "Max file size 18446744073709551615 18446744073709551615 bytes",
            "LimitFSIZE",
        ),
    ] {
        let mut obs = context::fixture();
        obs.caller_limits = obs.caller_limits.replace(from, to);
        refused(
            evaluate(obs.clone()),
            &format!("rlimit-unrepresentable:{property}:soft"),
        );
        let hard_only = to.replacen(
            to.split_whitespace()
                .find(|s| *s == "unlimited" || s.bytes().all(|b| b.is_ascii_digit()))
                .unwrap_or("unlimited"),
            "0",
            1,
        );
        obs.caller_limits = context::fixture().caller_limits.replace(from, &hard_only);
        refused(
            evaluate(obs),
            &format!("rlimit-unrepresentable:{property}:hard"),
        );
    }
    let base = context::fixture().caller_limits;
    let caller = base
        .replace(
            "Max cpu time unlimited unlimited seconds",
            "Max cpu time 18446744073708 18446744073708 seconds",
        )
        .replace(
            "Max realtime timeout unlimited unlimited us",
            "Max realtime timeout 9223372036854775807 9223372036854775807 us",
        )
        .replace(
            "Max file size unlimited unlimited bytes",
            "Max file size 18446744073709551614 18446744073709551614 bytes",
        );
    assert_eq!(test_ok(context::limit_properties(&caller)).len(), 16);
}
