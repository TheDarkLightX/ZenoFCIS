use super::*;
use crate::checker_process::{test_err, test_ok};

#[test]
fn every_carried_limit_accepts_equal_and_tighter_readback_but_rejects_looser() {
    let fixture = limit_fixture();
    for line in fixture.lines().skip(1) {
        let pair = |value| {
            line.replace("unlimited unlimited", value)
                .replace("40 40", value)
        };
        let caller = fixture.replace(line, &pair("1 2"));
        assert_eq!(test_ok(limit_properties(&caller)).len(), 16);
        assert_eq!(verify_service_limits(&caller, &caller), Ok(()));
        for changed in ["1 3", "2 2", "1 unlimited", "unlimited unlimited"] {
            assert!(
                verify_service_limits(&caller, &fixture.replace(line, &pair(changed))).is_err(),
                "{line}: {changed}"
            );
        }
        assert_eq!(
            verify_service_limits(&caller, &fixture.replace(line, &pair("0 1"))),
            Ok(())
        );
    }
}

#[test]
fn malformed_unknown_missing_or_duplicate_limit_values_refuse() {
    let base = limit_fixture();
    for bad in [
        String::new(),
        "malformed".into(),
        base.replacen("Max open files", "Unknown limit", 1),
        base.replace("Max open files unlimited unlimited files\n", ""),
        format!("{base}Max cpu time unlimited unlimited seconds\n"),
        base.replacen("unlimited unlimited", "2 1", 1),
        base.replacen("unlimited unlimited", "max unlimited", 1),
        base.replacen("seconds", "bytes", 1),
    ] {
        assert!(limit_properties(&bad).is_err());
        assert!(verify_service_limits(&base, &bad).is_err());
        assert!(verify_service_limits(&bad, &base).is_err());
    }
}

#[test]
fn ordinary_caller_unit_defaults_admit_without_manager_comparison() {
    let caller = limit_fixture().replace(
        "Max open files unlimited unlimited files",
        "Max open files 1024 1048576 files",
    );
    assert!(
        test_ok(limit_properties(&caller)).contains(&"--property=LimitNOFILE=1024:1048576".into())
    );
    assert_eq!(verify_service_limits(&caller, &caller), Ok(()));
    let raised = caller.replace("Max open files 1024", "Max open files 1048576");
    assert_eq!(
        verify_service_limits(&caller, &raised),
        Err("rlimits-readback-looser:LimitNOFILE:soft".into())
    );
}

#[test]
fn unrepresentable_values_refuse_with_property_and_side() {
    for (line, property, value) in [
        (
            "Max cpu time unlimited unlimited seconds",
            "LimitCPU",
            "18446744073709",
        ),
        (
            "Max realtime timeout unlimited unlimited us",
            "LimitRTTIME",
            "9223372036854775808",
        ),
        ("Max nice priority 40 40", "LimitNICE", "41"),
        ("Max nice priority 40 40", "LimitNICE", "unlimited"),
        (
            "Max file size unlimited unlimited bytes",
            "LimitFSIZE",
            "18446744073709551615",
        ),
    ] {
        let old = if property == "LimitNICE" {
            "40 40"
        } else {
            "unlimited unlimited"
        };
        for (pair, side) in [
            (format!("{value} {value}"), "soft"),
            (format!("0 {value}"), "hard"),
        ] {
            let input = limit_fixture().replace(line, &line.replace(old, &pair));
            assert_eq!(
                test_err(limit_properties(&input)),
                format!("rlimit-unrepresentable:{property}:{side}")
            );
        }
    }
}

#[test]
fn scheduler_policy_priority_carry_and_readback_are_exact() {
    for (policy, priority, name) in [
        ("0", "0", "other"),
        ("1", "7", "fifo"),
        ("2", "9", "rr"),
        ("3", "0", "batch"),
        ("5", "0", "idle"),
    ] {
        let expected = test_ok(Scheduler::parse(policy, priority));
        assert_eq!(
            expected.properties(),
            [
                format!("--property=CPUSchedulingPolicy={name}"),
                format!("--property=CPUSchedulingPriority={priority}")
            ]
        );
        assert_eq!(expected.verify(expected), Ok(()));
        let other = Scheduler {
            policy: if policy == "0" { 5 } else { 0 },
            priority: 0,
        };
        assert!(expected.verify(other).is_err());
    }
    for (policy, priority) in [
        ("6", "0"),
        ("1", "0"),
        ("5", "1"),
        ("0", "bad"),
        ("bad", "0"),
    ] {
        assert!(Scheduler::parse(policy, priority).is_err());
    }
    assert!(Scheduler::read("malformed").is_err());
}

struct Groups(PathBuf);
impl Groups {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "zeno-quota-test-{}-{}",
            std::process::id(),
            crate::TEMP_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        test_ok(std::fs::create_dir(&path));
        test_ok(std::fs::write(path.join("cgroup.procs"), ""));
        Self(path)
    }
}
impl Drop for Groups {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn quota_requires_observation_and_reads_the_root_and_binding_period() {
    let group = Groups::new();
    assert_eq!(
        test_err(chain_quota(&group.0, group.0.clone(), 4)),
        "caller-cpu-quota-unobserved"
    );
    test_ok(std::fs::write(group.0.join("cpu.max"), "50000 100000"));
    assert_eq!(
        test_ok(chain_quota(&group.0, group.0.clone(), 4)),
        CpuQuota {
            quota: 50000,
            period: 100000
        }
    );
    let leaf = group.0.join("leaf");
    test_ok(std::fs::create_dir(&leaf));
    test_ok(std::fs::write(leaf.join("cgroup.procs"), ""));
    assert_eq!(
        test_ok(chain_quota(&group.0, leaf.clone(), 4)),
        CpuQuota {
            quota: 50000,
            period: 100000
        }
    );
    test_ok(std::fs::write(leaf.join("cpu.max"), "15000 50000"));
    let expected = test_ok(chain_quota(&group.0, leaf, 4));
    assert_eq!(
        expected,
        CpuQuota {
            quota: 15000,
            period: 50000
        }
    );
    assert_eq!(
        test_ok(expected.properties()),
        [
            "--property=CPUQuota=30%",
            "--property=CPUQuotaPeriodSec=50000us"
        ]
    );
    assert_eq!(expected.verify("15000 50000"), Ok(()));
    for bad in ["15000 100000", "30000 50000", "max 50000", "bad"] {
        assert!(expected.verify(bad).is_err());
    }
}

#[test]
fn all_sixteen_limit_properties_preserve_exact_values_and_infinity() {
    let caller = limit_fixture()
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
        test_ok(limit_properties(&caller)),
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
    assert_eq!(verify_service_limits(&caller, &caller), Ok(()));
}
