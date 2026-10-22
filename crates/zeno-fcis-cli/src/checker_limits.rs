//! Resource carry and independent kernel readback for explicitly selected systemd.
use std::path::{Path, PathBuf};

pub(in crate::checker_process) fn decimal(text: &str) -> Result<u64, String> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err("malformed-number".into());
    }
    text.parse().map_err(|_| "malformed-number".into())
}

fn limit(text: &str) -> Result<Option<u64>, String> {
    match text {
        "unlimited" => Ok(None),
        _ => decimal(text).map(Some),
    }
}

fn stat_field(text: &str, index: usize) -> Result<&str, String> {
    text.rsplit_once(')')
        .and_then(|(_, tail)| tail.split_whitespace().nth(index))
        .ok_or_else(|| "stat-malformed".into())
}

const RLIMITS: [(&str, &str, &str); 16] = [
    ("Max cpu time", "seconds", "LimitCPU"),
    ("Max file size", "bytes", "LimitFSIZE"),
    ("Max data size", "bytes", "LimitDATA"),
    ("Max stack size", "bytes", "LimitSTACK"),
    ("Max core file size", "bytes", "LimitCORE"),
    ("Max resident set", "bytes", "LimitRSS"),
    ("Max processes", "processes", "LimitNPROC"),
    ("Max open files", "files", "LimitNOFILE"),
    ("Max locked memory", "bytes", "LimitMEMLOCK"),
    ("Max address space", "bytes", "LimitAS"),
    ("Max file locks", "locks", "LimitLOCKS"),
    ("Max pending signals", "signals", "LimitSIGPENDING"),
    ("Max msgqueue size", "bytes", "LimitMSGQUEUE"),
    ("Max nice priority", "", "LimitNICE"),
    ("Max realtime priority", "", "LimitRTPRIO"),
    ("Max realtime timeout", "us", "LimitRTTIME"),
];
type Limits = [(Option<u64>, Option<u64>); 16];
fn parse_limits(text: &str) -> Result<Limits, String> {
    let mut lines = text.lines();
    if lines
        .next()
        .map(|s| s.split_whitespace().collect::<Vec<_>>())
        != Some(vec!["Limit", "Soft", "Limit", "Hard", "Limit", "Units"])
    {
        return Err("rlimits-malformed".into());
    }
    let mut out = [(None, None); 16];
    let mut seen = [false; 16];
    for line in lines {
        let (index, rest) = RLIMITS
            .iter()
            .enumerate()
            .find_map(|(i, (name, _, _))| {
                line.strip_prefix(name)
                    .filter(|s| s.starts_with(char::is_whitespace))
                    .map(|s| (i, s))
            })
            .ok_or("rlimits-malformed")?;
        if seen[index] {
            return Err("rlimits-malformed".into());
        }
        let fields: Vec<_> = rest.split_whitespace().collect();
        let unit = RLIMITS[index].1;
        if fields.len() != if unit.is_empty() { 2 } else { 3 }
            || (!unit.is_empty() && fields[2] != unit)
        {
            return Err("rlimits-malformed".into());
        }
        let parse = |s| limit(s).map_err(|_| "rlimits-malformed".to_owned());
        let pair = (parse(fields[0])?, parse(fields[1])?);
        if pair
            .1
            .is_some_and(|hard| pair.0.is_none_or(|soft| soft > hard))
        {
            return Err("rlimits-malformed".into());
        }
        out[index] = pair;
        seen[index] = true;
    }
    if seen.contains(&false) {
        return Err("rlimits-malformed".into());
    }
    Ok(out)
}

/// systemd.exec uses bytes/counts, CPU seconds, RTTIME microseconds and raw
/// unsigned NICE limits. Never convert an unrepresentable finite value to infinity.
pub(in crate::checker_process) fn limit_properties(caller: &str) -> Result<Vec<String>, String> {
    let limits = parse_limits(caller)?;
    RLIMITS
        .iter()
        .zip(limits)
        .map(|((_, _, property), (soft, hard))| {
            let value = |v: Option<u64>, side: &str| -> Result<String, String> {
                let representable = match (*property, v) {
                    ("LimitNICE", v) => v.is_some_and(|v| v <= 40),
                    // systemd parses CPU seconds as usec and rounds up to seconds;
                    // RTTIME's integer time parser accepts signed 64-bit usec.
                    ("LimitCPU", Some(v)) => v
                        .checked_mul(1_000_000)
                        .and_then(|us| us.checked_add(999_999))
                        .is_some(),
                    ("LimitRTTIME", Some(v)) => v <= i64::MAX as u64,
                    (_, Some(v)) => v != u64::MAX,
                    (_, None) => true,
                };
                if !representable {
                    return Err(format!("rlimit-unrepresentable:{property}:{side}"));
                }
                Ok(v.map_or_else(|| "infinity".into(), |v| v.to_string()))
            };
            Ok(format!(
                "--property={property}={}:{}",
                value(soft, "soft")?,
                value(hard, "hard")?
            ))
        })
        .collect()
}

/// Actual startup limits must be no looser than the carried caller snapshot.
pub(in crate::checker_process) fn verify_service_limits(
    caller: &str,
    actual: &str,
) -> Result<(), String> {
    let caller = parse_limits(caller)?;
    let actual = parse_limits(actual).map_err(|_| "rlimits-readback-malformed")?;
    for (((_, _, property), a), b) in RLIMITS.iter().zip(caller).zip(actual) {
        for (side, caller, service) in [("soft", a.0, b.0), ("hard", a.1, b.1)] {
            if caller.is_some_and(|v| service.is_none_or(|s| s > v)) {
                return Err(format!("rlimits-readback-looser:{property}:{side}"));
            }
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::checker_process) struct Scheduler {
    pub policy: u64,
    pub priority: u64,
}
impl Scheduler {
    pub(in crate::checker_process) fn name(self) -> &'static str {
        match self.policy {
            0 => "other",
            1 => "fifo",
            2 => "rr",
            3 => "batch",
            5 => "idle",
            _ => "unknown",
        }
    }
    pub(in crate::checker_process) fn parse(policy: &str, priority: &str) -> Result<Self, String> {
        let policy = decimal(policy).map_err(|_| "cpu-scheduler-malformed")?;
        let priority = decimal(priority).map_err(|_| "cpu-scheduler-malformed")?;
        if !matches!(policy, 0 | 1 | 2 | 3 | 5)
            || (matches!(policy, 1 | 2) && !(1..=99).contains(&priority))
            || (!matches!(policy, 1 | 2) && priority != 0)
        {
            return Err("cpu-scheduler-unsupported".into());
        }
        Ok(Self { policy, priority })
    }
    pub(in crate::checker_process) fn read(text: &str) -> Result<Self, String> {
        Self::parse(stat_field(text, 38)?, stat_field(text, 37)?)
    }
    pub(in crate::checker_process) fn verify(self, actual: Self) -> Result<(), String> {
        if self != actual {
            return Err("cpu-scheduler-readback-mismatch".into());
        }
        Ok(())
    }
    pub(in crate::checker_process) fn properties(self) -> [String; 2] {
        [
            format!("--property=CPUSchedulingPolicy={}", self.name()),
            format!("--property=CPUSchedulingPriority={}", self.priority),
        ]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::checker_process) struct CpuQuota {
    pub quota: u64,
    pub period: u64,
}
impl CpuQuota {
    pub(in crate::checker_process) fn parse(text: &str) -> Result<(Option<u64>, u64), String> {
        let fields: Vec<_> = text.split_whitespace().collect();
        if fields.len() != 2 {
            return Err("caller-cpu-quota-malformed".into());
        }
        let period = decimal(fields[1]).map_err(|_| "caller-cpu-quota-malformed")?;
        if !(1000..=1_000_000).contains(&period) {
            return Err("caller-cpu-quota-malformed".into());
        }
        let quota = match fields[0] {
            "max" => None,
            s => Some(decimal(s).map_err(|_| "caller-cpu-quota-malformed")?),
        };
        if quota.is_some_and(|q| q < 1000) {
            return Err("caller-cpu-quota-malformed".into());
        }
        if quota.is_some_and(|q| u128::from(q) * 100 < u128::from(period)) {
            return Err("caller-cpu-quota-below-one-percent".into());
        }
        Ok((quota, period))
    }
    pub(in crate::checker_process) fn properties(self) -> Result<[String; 2], String> {
        Self::parse(&format!("{} {}", self.quota, self.period))?;
        let scaled = u128::from(self.quota) * 100_000_000;
        if scaled % u128::from(self.period) != 0 {
            return Err("caller-cpu-quota-unrepresentable".into());
        }
        let scaled = scaled / u128::from(self.period);
        let mut percent = format!("{}.{:06}", scaled / 1_000_000, scaled % 1_000_000);
        while percent.ends_with('0') {
            percent.pop();
        }
        if percent.ends_with('.') {
            percent.pop();
        }
        Ok([
            format!("--property=CPUQuota={percent}%"),
            format!("--property=CPUQuotaPeriodSec={}us", self.period),
        ])
    }
    pub(in crate::checker_process) fn verify(self, text: &str) -> Result<(), String> {
        let (q, p) = Self::parse(text)?;
        if q != Some(self.quota) || p != self.period {
            return Err("cpu-quota-readback-mismatch".into());
        }
        Ok(())
    }
}

pub(in crate::checker_process) fn chain_quota(
    root: &Path,
    mut path: PathBuf,
    workers: u64,
) -> Result<CpuQuota, String> {
    let mut tightest: Option<CpuQuota> = None;
    let mut observed_period = None;
    loop {
        if !path.starts_with(root)
            || path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
        {
            return Err("caller-cpu-quota-group-outside-root".into());
        }
        if !path.join("cgroup.procs").is_file() {
            return Err("caller-cpu-quota-unavailable:not-a-cgroup".into());
        }
        match std::fs::read_to_string(path.join("cpu.max")) {
            Ok(text) => {
                let (quota, period) = CpuQuota::parse(&text)?;
                observed_period = Some(period);
                if let Some(quota) = quota {
                    let next = CpuQuota { quota, period };
                    if tightest.is_none_or(|old| {
                        u128::from(next.quota) * u128::from(old.period)
                            < u128::from(old.quota) * u128::from(next.period)
                    }) {
                        tightest = Some(next);
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(_) => return Err("caller-cpu-quota-unavailable:unreadable".into()),
        }
        if path == root {
            break;
        }
        if !path.pop() {
            return Err("caller-cpu-quota-group-outside-root".into());
        }
    }
    let period = observed_period.ok_or("caller-cpu-quota-unobserved")?;
    let mut result = tightest.unwrap_or(CpuQuota {
        quota: workers.clamp(1, 8) * period,
        period,
    });
    result.quota = result.quota.min(workers.clamp(1, 8) * result.period);
    result.properties()?;
    Ok(result)
}

#[cfg(test)]
pub(super) fn limit_fixture() -> String {
    "Limit Soft Limit Hard Limit Units\n\
Max cpu time unlimited unlimited seconds\n\
Max file size unlimited unlimited bytes\n\
Max data size unlimited unlimited bytes\n\
Max stack size unlimited unlimited bytes\n\
Max core file size unlimited unlimited bytes\n\
Max resident set unlimited unlimited bytes\n\
Max processes unlimited unlimited processes\n\
Max open files unlimited unlimited files\n\
Max locked memory unlimited unlimited bytes\n\
Max address space unlimited unlimited bytes\n\
Max file locks unlimited unlimited locks\n\
Max pending signals unlimited unlimited signals\n\
Max msgqueue size unlimited unlimited bytes\n\
Max nice priority 40 40\n\
Max realtime priority unlimited unlimited\n\
Max realtime timeout unlimited unlimited us\n"
        .to_owned()
}

#[cfg(test)]
#[path = "checker_limits_tests.rs"]
mod tests;
