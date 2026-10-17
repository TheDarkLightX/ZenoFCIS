//! Operational admission only. These values never grant checker authority.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub(crate) enum Profile {
    #[serde(rename = "finite-data/1")]
    FiniteData,
    #[serde(rename = "python-reference/1")]
    PythonReference,
    #[serde(rename = "group-resources/1")]
    GroupResources,
}

impl Profile {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::FiniteData => "finite-data/1",
            Self::PythonReference => "python-reference/1",
            Self::GroupResources => "group-resources/1",
        }
    }

    // The bytes are the frozen profile contract, independent of Rust layout.
    pub(crate) const fn contract(self) -> &'static str {
        match self {
            Self::FiniteData => {
                "finite-data/1;trusted-bounded-data;wall<=25s;bounded-intake-work-output-storage;minimal-env;fd-allowlist;owned-worker-cleanup;cpu<=25s;fsize<=16777216B;nofile<=64;core=0;linux-as<=67108864B;files=absent;network=absent;swap=absent;hostile-escaped-descendants=absent;macos-hard-memory=absent"
            }
            Self::PythonReference => {
                "python-reference/1;wall<=3s;leaf-as<=50331648B;leaf-cpu<=2s;leaf-fsize<=65536B;leaf-nofile<=32;leaf-core=0;leaf-minimal-env;leaf-fd-allowlist;leaf-scratch-only-content;leaf-network-denied;leaf-no-fork-exec;leaf-no-cross-process;landlock>=3;seccomp-default-deny;no-new-privileges;non-dumpable;owned-leaf-cleanup;qualified-linux-x86_64-only"
            }
            Self::GroupResources => {
                "group-resources/1;group-memory<=67108864B;group-swap=0;group-tasks<=64;inherited-cpu-quota-affinity-nice;external-wall;bounded-intake-work-output-storage;cgroup-descendant-cleanup;legacy-systemd-policy"
            }
        }
    }

    pub(crate) fn hash(self) -> String {
        crate::transform::sha256_hex(self.contract().as_bytes())
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, clap::ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Backend {
    #[default]
    Auto,
    Systemd,
    Posix,
}
impl Backend {
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Systemd => "systemd",
            Self::Posix => "posix",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Control {
    Wall,
    BoundedData,
    Environment,
    FileDescriptors,
    OwnedCleanup,
    AddressSpace,
    CpuSeconds,
    FileSize,
    OpenFiles,
    CoreSize,
    GroupMemory,
    GroupSwap,
    GroupTasks,
    CpuPolicy,
    TreeCleanup,
    LeafMemory,
    LeafIsolation,
    LeafWall,
    LeafCpu,
    LeafFileSize,
    LeafOpenFiles,
    LeafCoreSize,
    LeafNoNewPrivileges,
    LeafNonDumpable,
    LeafLandlock,
    LeafSeccomp,
    LeafFileContent,
    LeafNetwork,
    LeafNoDescendants,
    LeafCrossProcess,
    Files,
    Network,
}
impl Control {
    pub(crate) fn name(self) -> String {
        // This encoding is shared by typed refusals and evidence.
        serde_json::to_value(self)
            .ok()
            .and_then(|v| v.as_str().map(str::to_owned))
            .unwrap_or_else(|| "unknown-control".into())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Scope {
    Invocation,
    TrustedWorker,
    Group,
    PythonLeaf,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum State {
    Holds,
    Weaker,
    Absent,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum Unit {
    Bytes,
    Seconds,
    Count,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Quantity {
    pub(crate) value: u64,
    pub(crate) unit: Unit,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Guarantee {
    pub(crate) control: Control,
    pub(crate) state: State,
    pub(crate) scope: Scope,
    #[serde(deserialize_with = "required_nullable")]
    pub(crate) requested: Option<Quantity>,
    #[serde(deserialize_with = "required_nullable")]
    pub(crate) effective: Option<Quantity>,
}

fn requirement(control: Control, scope: Scope, limit: Option<(u64, Unit)>) -> Guarantee {
    let quantity = limit.map(|(value, unit)| Quantity { value, unit });
    Guarantee {
        control,
        state: State::Holds,
        scope,
        requested: quantity.clone(),
        effective: quantity,
    }
}

pub(crate) fn requirements(profile: Profile, linux: bool) -> Vec<Guarantee> {
    use Control::*;
    use Scope::*;
    use Unit::*;
    let mut out = vec![
        requirement(
            Wall,
            Invocation,
            if profile == Profile::FiniteData {
                Some((25, Seconds))
            } else {
                None
            },
        ),
        requirement(BoundedData, Invocation, None),
    ];
    match profile {
        Profile::FiniteData => {
            out.extend(
                [Environment, FileDescriptors, OwnedCleanup]
                    .map(|c| requirement(c, TrustedWorker, None)),
            );
            out.extend([
                requirement(CpuSeconds, TrustedWorker, Some((25, Seconds))),
                requirement(FileSize, TrustedWorker, Some((16 * 1024 * 1024, Bytes))),
                requirement(OpenFiles, TrustedWorker, Some((64, Count))),
                requirement(CoreSize, TrustedWorker, Some((0, Bytes))),
            ]);
            if linux {
                out.push(requirement(
                    AddressSpace,
                    TrustedWorker,
                    Some((64 * 1024 * 1024, Bytes)),
                ));
            }
        }
        Profile::PythonReference => {
            out.extend(
                [Environment, FileDescriptors, OwnedCleanup, LeafIsolation]
                    .map(|c| requirement(c, PythonLeaf, None)),
            );
            out.extend([
                requirement(LeafMemory, PythonLeaf, Some((48 * 1024 * 1024, Bytes))),
                requirement(LeafWall, PythonLeaf, Some((3, Seconds))),
                requirement(LeafCpu, PythonLeaf, Some((2, Seconds))),
                requirement(LeafFileSize, PythonLeaf, Some((65536, Bytes))),
                requirement(LeafOpenFiles, PythonLeaf, Some((32, Count))),
                requirement(LeafCoreSize, PythonLeaf, Some((0, Bytes))),
            ]);
            out.extend(
                [
                    LeafNoNewPrivileges,
                    LeafNonDumpable,
                    LeafLandlock,
                    LeafSeccomp,
                    LeafFileContent,
                    LeafNetwork,
                    LeafNoDescendants,
                    LeafCrossProcess,
                ]
                .map(|c| requirement(c, PythonLeaf, None)),
            );
        }
        Profile::GroupResources => {
            out.extend([
                requirement(GroupMemory, Group, Some((64 * 1024 * 1024, Bytes))),
                requirement(GroupSwap, Group, Some((0, Bytes))),
                requirement(GroupTasks, Group, Some((64, Count))),
                requirement(CpuPolicy, Group, None),
                requirement(TreeCleanup, Group, None),
            ]);
        }
    }
    out
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Unavailable {
    pub(crate) backend: Backend,
    pub(crate) missing: String,
}
impl std::fmt::Display for Unavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "containment-unavailable:{}:{}",
            self.backend.name(),
            self.missing
        )
    }
}

/// Unknown, duplicate, wrong-scope, wrong-unit and unbounded controls all fail.
/// No mutation, environment or backend probing occurs at this decision boundary.
pub(crate) fn compare(
    profile: Profile,
    backend: Backend,
    linux: bool,
    actual: &[Guarantee],
) -> Result<(), Unavailable> {
    for needed in requirements(profile, linux) {
        let mut matches = actual.iter().filter(|g| g.control == needed.control);
        let found = matches.next();
        let valid = matches.next().is_none()
            && found.is_some_and(|g| {
                g.state == State::Holds
                    && g.scope == needed.scope
                    && g.requested == needed.requested
                    && match (&needed.effective, &g.effective) {
                        (Some(n), Some(a)) => n.unit == a.unit && a.value <= n.value,
                        (Some(_), None) => false,
                        (None, _) => true,
                    }
            });
        if !valid {
            return Err(Unavailable {
                backend,
                missing: needed.control.name(),
            });
        }
    }
    Ok(())
}

fn required_nullable<'de, D: serde::Deserializer<'de>, T: serde::Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    Option::<T>::deserialize(deserializer)
}

pub(crate) const EVIDENCE_SCHEMA: &str = "zeno-fcis/containment-run/1";
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Termination {
    Running,
    Exited,
    Stopped,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Cleanup {
    Pending,
    OwnedWorkersReaped,
    Unknown,
}

/// Supervisor-owned provenance. All fields are required, including explicit
/// unknown identities/limits; it is never inserted in transform-receipt/1.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RunEvidence {
    pub(crate) schema: String,
    pub(crate) backend: Backend,
    pub(crate) os: String,
    pub(crate) architecture: String,
    pub(crate) required_profile: Profile,
    pub(crate) profile_sha256: String,
    pub(crate) policy_build_sha256: String,
    pub(crate) invocation_sha256: String,
    #[serde(deserialize_with = "required_nullable")]
    pub(crate) subject_sha256: Option<String>,
    #[serde(deserialize_with = "required_nullable")]
    pub(crate) semantic_receipt_sha256: Option<String>,
    pub(crate) guarantees: Vec<Guarantee>,
    pub(crate) worker_pid: u32,
    pub(crate) termination: Termination,
    #[serde(deserialize_with = "required_nullable")]
    pub(crate) exit_code: Option<u8>,
    #[serde(deserialize_with = "required_nullable")]
    pub(crate) signal: Option<u8>,
    #[serde(deserialize_with = "required_nullable")]
    pub(crate) termination_reason: Option<String>,
    pub(crate) cleanup: Cleanup,
    pub(crate) absence_verified: bool,
}

impl RunEvidence {
    pub(crate) fn validate_posix_identity(
        &self,
        build: &str,
        invocation: &str,
        os: &str,
        architecture: &str,
    ) -> Result<(), String> {
        if self.schema != EVIDENCE_SCHEMA
            || self.backend != Backend::Posix
            || self.required_profile != Profile::FiniteData
            || self.profile_sha256 != Profile::FiniteData.hash()
            || self.policy_build_sha256 != build
            || self.invocation_sha256 != invocation
            || self.os != os
            || self.architecture != architecture
        {
            return Err("guardian-evidence-binding".into());
        }
        Ok(())
    }

    /// Compare only captured, immutable observations. Transport and process
    /// ownership are the caller shell's separate obligations.
    pub(crate) fn validate_posix_completion(
        &self,
        build: &str,
        invocation: &str,
        os: &str,
        architecture: &str,
        code: u8,
    ) -> Result<(), String> {
        self.validate_posix_identity(build, invocation, os, architecture)?;
        if self.worker_pid == 0
            || self.termination != Termination::Exited
            || self.exit_code != Some(code)
            || self.signal.is_some()
            || self.termination_reason.is_some()
            || self.cleanup != Cleanup::OwnedWorkersReaped
            || !self.absence_verified
        {
            return Err("guardian-evidence-binding".into());
        }
        compare(Profile::FiniteData, Backend::Posix, true, &self.guarantees)
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn comparison_refuses_each_missing_wrong_scope_and_weaker_control_without_mutation() {
        for p in [
            Profile::FiniteData,
            Profile::PythonReference,
            Profile::GroupResources,
        ] {
            let good = requirements(p, true);
            assert_eq!(compare(p, Backend::Posix, true, &good), Ok(()));
            let before = good.clone();
            for i in 0..good.len() {
                for mutation in 0..6 {
                    let mut bad = good.clone();
                    match mutation {
                        0 => {
                            bad.remove(i);
                        }
                        1 => bad[i].state = State::Absent,
                        2 => bad[i].state = State::Weaker,
                        3 => {
                            bad[i].scope = if bad[i].scope == Scope::Group {
                                Scope::Invocation
                            } else {
                                Scope::Group
                            }
                        }
                        4 => bad.push(bad[i].clone()),
                        _ => {
                            if let Some(q) = &mut bad[i].effective {
                                q.value += 1;
                            } else {
                                bad[i].state = State::Absent;
                            }
                        }
                    }
                    assert!(
                        compare(p, Backend::Posix, true, &bad).is_err(),
                        "{p:?} {i} {mutation}"
                    );
                }
            }
            assert_eq!(good, before);
        }
    }
    #[test]
    fn profiles_do_not_confuse_per_process_memory_with_group_or_python_isolation() {
        let finite = requirements(Profile::FiniteData, true);
        for p in [Profile::PythonReference, Profile::GroupResources] {
            assert!(compare(p, Backend::Posix, true, &finite).is_err());
        }
        let mac = requirements(Profile::FiniteData, false);
        assert!(mac.iter().all(|g| g.control != Control::AddressSpace));
        assert!(compare(Profile::FiniteData, Backend::Posix, true, &mac).is_err());
        for p in [
            Profile::FiniteData,
            Profile::PythonReference,
            Profile::GroupResources,
        ] {
            assert_eq!(p.hash().len(), 64);
            assert!(p.contract().starts_with(p.name()));
        }
    }

    #[test]
    fn versioned_profile_contract_hashes_are_frozen() {
        assert_eq!(
            Profile::FiniteData.hash(),
            "fe782e7d687ea20c945e18f08886dc90a412f76cfeaf26da2787ade9efc0cc29"
        );
        assert_eq!(
            Profile::PythonReference.hash(),
            "e6657ec1b129c68986daa54e734cbf07f0a36342851f61f8904072c933db5e2f"
        );
        assert_eq!(
            Profile::GroupResources.hash(),
            "10a39c722e0191c35ceeb9ab0929ba950279f5fb0633f6ee0473ed78a9b1f884"
        );
    }

    fn completed() -> RunEvidence {
        RunEvidence {
            schema: EVIDENCE_SCHEMA.into(),
            backend: Backend::Posix,
            os: "linux".into(),
            architecture: "x86_64".into(),
            required_profile: Profile::FiniteData,
            profile_sha256: Profile::FiniteData.hash(),
            policy_build_sha256: "build".into(),
            invocation_sha256: "invocation".into(),
            subject_sha256: None,
            semantic_receipt_sha256: None,
            guarantees: requirements(Profile::FiniteData, true),
            worker_pid: 1,
            termination: Termination::Exited,
            exit_code: Some(0),
            signal: None,
            termination_reason: None,
            cleanup: Cleanup::OwnedWorkersReaped,
            absence_verified: true,
        }
    }
    #[test]
    fn run_evidence_schema_requires_explicit_fields_and_rejects_unknowns() {
        let e = completed();
        let value = serde_json::to_value(&e).unwrap_or_else(|e| panic!("{e}"));
        let fields: Vec<_> = value
            .as_object()
            .unwrap_or_else(|| panic!("object"))
            .keys()
            .cloned()
            .collect();
        for field in fields {
            let mut missing = value.clone();
            missing
                .as_object_mut()
                .unwrap_or_else(|| panic!("object"))
                .remove(&field);
            assert!(
                serde_json::from_value::<RunEvidence>(missing).is_err(),
                "missing {field}"
            );
        }
        let mut extra = value.clone();
        extra["contained"] = serde_json::json!(true);
        assert!(serde_json::from_value::<RunEvidence>(extra).is_err());
        let mut missing = value;
        missing["guarantees"][0]
            .as_object_mut()
            .unwrap_or_else(|| panic!("object"))
            .remove("effective");
        assert!(serde_json::from_value::<RunEvidence>(missing).is_err());
    }
    #[test]
    fn completion_binding_rejects_corrupt_backend_profile_identity_exit_and_cleanup() {
        let original = completed();
        let valid = |e: &RunEvidence| {
            e.validate_posix_completion("build", "invocation", "linux", "x86_64", 0)
        };
        assert_eq!(valid(&original), Ok(()));
        for i in 0..16 {
            let mut e = original.clone();
            match i {
                0 => e.schema.push('x'),
                1 => e.backend = Backend::Systemd,
                2 => e.required_profile = Profile::PythonReference,
                3 => e.profile_sha256.push('x'),
                4 => e.policy_build_sha256.push('x'),
                5 => e.invocation_sha256.push('x'),
                6 => e.os = "macos".into(),
                7 => e.architecture = "unknown".into(),
                8 => e.worker_pid = 0,
                9 => e.termination = Termination::Running,
                10 => e.exit_code = None,
                11 => e.signal = Some(9),
                12 => e.termination_reason = Some("unknown".into()),
                13 => e.cleanup = Cleanup::Unknown,
                14 => e.absence_verified = false,
                _ => e.guarantees.clear(),
            }
            assert!(valid(&e).is_err(), "mutant {i}");
        }
        assert_eq!(
            valid(&original),
            Ok(()),
            "scratch mutations cannot alias the input"
        );
    }
}
