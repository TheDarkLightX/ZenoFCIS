from pathlib import Path


def replace_exact(path: Path, old: str, new: str, label: str) -> None:
    text = path.read_text(encoding="utf-8")
    if old not in text:
        if new in text:
            return
        raise SystemExit(f"missing security assembly site: {label}")
    path.write_text(text.replace(old, new, 1), encoding="utf-8")


assurance = Path("tools/check_assurance.py")
replace_exact(
    assurance,
    '    "zeno-fcis-catalog",\n    "zeno-fcis-patch",\n',
    '    "zeno-fcis-catalog",\n    "zeno-fcis-security",\n    "zeno-fcis-patch",\n',
    "security semantic boundary",
)
replace_exact(
    assurance,
    '    "zeno-fcis-catalog": 2,\n    "zeno-fcis": 3,\n',
    '    "zeno-fcis-catalog": 2,\n    "zeno-fcis-security": 2,\n    "zeno-fcis": 3,\n',
    "security dependency ring",
)

security = Path("crates/zeno-fcis-security/src/lib.rs")
replace_exact(
    security,
    '''/// Maximum capacity measurements evaluated at once.
pub const MAX_CAPACITY_EVIDENCE: usize = 4_096;
/// Maximum leakage blockers retained in one report.
''',
    '''/// Maximum capacity measurements evaluated at once.
pub const MAX_CAPACITY_EVIDENCE: usize = 4_096;
/// Maximum leakage reports evaluated by one promotion decision.
pub const MAX_LEAKAGE_REPORTS: usize = 4_096;
/// Maximum leakage blockers retained in one report.
''',
    "promotion report bound",
)
replace_exact(
    security,
    '''    pub const fn deployment_hash(&self) -> Hash32 {
        self.deployment_hash
    }
}
''',
    '''    pub const fn deployment_hash(&self) -> Hash32 {
        self.deployment_hash
    }

    /// Returns the exact threat-model commitment.
    #[must_use]
    pub const fn threat_model_hash(&self) -> Hash32 {
        self.threat_model_hash
    }
}
''',
    "promotion threat model getter",
)
replace_exact(
    security,
    '''    LeakageDeploymentMismatch {
        /// Report index.
        index: u32,
    },
    /// Required evidence is absent.
''',
    '''    LeakageDeploymentMismatch {
        /// Report index.
        index: u32,
    },
    /// Leakage-policy deployment differs from the promotion policy.
    LeakagePolicyDeploymentMismatch,
    /// Leakage-policy threat model differs from the promotion policy.
    ThreatModelMismatch,
    /// A production decision supplied no leakage comparison.
    MissingLeakageReport,
    /// Leakage-report count exceeds the deterministic evaluation bound.
    TooManyLeakageReports {
        /// Supplied report count.
        observed: u32,
        /// Maximum evaluated report count.
        maximum: u32,
    },
    /// Security-evidence count exceeds the deterministic evaluation bound.
    TooManyEvidenceItems {
        /// Supplied evidence count.
        observed: u32,
        /// Maximum evaluated evidence count.
        maximum: u32,
    },
    /// Capacity-evidence count exceeds the deterministic evaluation bound.
    TooManyCapacityItems {
        /// Supplied capacity count.
        observed: u32,
        /// Maximum evaluated capacity count.
        maximum: u32,
    },
    /// Required evidence is absent.
''',
    "promotion binding and count blockers",
)
replace_exact(
    security,
    '''            Self::CapacityConfidenceTooLow { observed, minimum } => {
                output.push(9);
                output.extend_from_slice(&observed.to_be_bytes());
                output.extend_from_slice(&minimum.to_be_bytes());
            }
        }
''',
    '''            Self::CapacityConfidenceTooLow { observed, minimum } => {
                output.push(9);
                output.extend_from_slice(&observed.to_be_bytes());
                output.extend_from_slice(&minimum.to_be_bytes());
            }
            Self::LeakagePolicyDeploymentMismatch => output.push(10),
            Self::ThreatModelMismatch => output.push(11),
            Self::MissingLeakageReport => output.push(12),
            Self::TooManyLeakageReports { observed, maximum } => {
                output.push(13);
                output.extend_from_slice(&observed.to_be_bytes());
                output.extend_from_slice(&maximum.to_be_bytes());
            }
            Self::TooManyEvidenceItems { observed, maximum } => {
                output.push(14);
                output.extend_from_slice(&observed.to_be_bytes());
                output.extend_from_slice(&maximum.to_be_bytes());
            }
            Self::TooManyCapacityItems { observed, maximum } => {
                output.push(15);
                output.extend_from_slice(&observed.to_be_bytes());
                output.extend_from_slice(&maximum.to_be_bytes());
            }
        }
''',
    "promotion blocker encoding",
)
replace_exact(
    security,
    '''    let mut blockers = Vec::new();

    for (position, report) in reports.iter().enumerate() {
''',
    '''    let mut blockers = Vec::new();

    if leakage_policy.deployment_contract_hash != policy.deployment_hash {
        blockers.push(SecurityPromotionBlocker::LeakagePolicyDeploymentMismatch);
    }
    if leakage_policy.threat_model_hash != policy.threat_model_hash {
        blockers.push(SecurityPromotionBlocker::ThreatModelMismatch);
    }
    if reports.is_empty() {
        blockers.push(SecurityPromotionBlocker::MissingLeakageReport);
    }
    if reports.len() > MAX_LEAKAGE_REPORTS {
        blockers.push(SecurityPromotionBlocker::TooManyLeakageReports {
            observed: u32::try_from(reports.len()).unwrap_or(u32::MAX),
            maximum: u32::try_from(MAX_LEAKAGE_REPORTS).unwrap_or(u32::MAX),
        });
    }
    if evidence.len() > MAX_SECURITY_EVIDENCE {
        blockers.push(SecurityPromotionBlocker::TooManyEvidenceItems {
            observed: u32::try_from(evidence.len()).unwrap_or(u32::MAX),
            maximum: u32::try_from(MAX_SECURITY_EVIDENCE).unwrap_or(u32::MAX),
        });
        evidence.truncate(MAX_SECURITY_EVIDENCE);
    }
    if capacities.len() > MAX_CAPACITY_EVIDENCE {
        blockers.push(SecurityPromotionBlocker::TooManyCapacityItems {
            observed: u32::try_from(capacities.len()).unwrap_or(u32::MAX),
            maximum: u32::try_from(MAX_CAPACITY_EVIDENCE).unwrap_or(u32::MAX),
        });
        capacities.truncate(MAX_CAPACITY_EVIDENCE);
    }

    for (position, report) in reports.iter().take(MAX_LEAKAGE_REPORTS).enumerate() {
''',
    "promotion input bindings and bounds",
)
replace_exact(
    security,
    '''    #[test]
    fn promotion_succeeds_only_with_complete_deployment_evidence() {
''',
    '''    #[test]
    fn promotion_binds_threat_model_and_input_bounds() {
        let leakage_policy = policy(RuleMode::Exact, ChannelClass::Side);
        let promotion = SecurityPromotionPolicy::try_new(
            deployment_hash(),
            hash(99),
            vec![SecurityEvidenceKind::NoninterferenceProof],
            100,
            990_000,
        )
        .unwrap_or_else(|error| panic!("promotion: {error}"));
        let item = SecurityEvidence::try_new(
            SecurityEvidenceKind::NoninterferenceProof,
            hash(50),
            hash(51),
            hash(52),
            deployment_hash(),
        )
        .unwrap_or_else(|error| panic!("evidence: {error}"));
        let result = evaluate_security_promotion(
            &promotion,
            &leakage_policy,
            &[],
            vec![item; MAX_SECURITY_EVIDENCE + 1],
            Vec::new(),
        );
        assert!(result
            .blockers()
            .iter()
            .any(|item| matches!(item, SecurityPromotionBlocker::ThreatModelMismatch)));
        assert!(result
            .blockers()
            .iter()
            .any(|item| matches!(item, SecurityPromotionBlocker::MissingLeakageReport)));
        assert!(result.blockers().iter().any(|item| matches!(
            item,
            SecurityPromotionBlocker::TooManyEvidenceItems { .. }
        )));
    }

    #[test]
    fn promotion_succeeds_only_with_complete_deployment_evidence() {
''',
    "promotion binding regression test",
)
