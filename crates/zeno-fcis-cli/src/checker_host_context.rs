//! Owned host observations and pure validation. Proc/cgroup acquisition stays
//! in the shell; no observation is an equivalence or publication capability.
use std::path::{Path, PathBuf};

pub(in crate::checker_process) const NAMESPACES: [&str; 7] =
    ["mnt", "user", "pid", "net", "ipc", "uts", "cgroup"];
pub(in crate::checker_process) const RESIDUALS: [&str; 2] = [
    "concurrent-checker-services-are-not-charged-to-callers-aggregate-cgroup-budget",
    "privileged-same-namespace-landlock-with-NoNewPrivs-zero-is-not-observable",
];

#[derive(Clone, Debug)]
pub(in crate::checker_process) struct Observations {
    pub manager_pid: String,
    pub manager_stat: (String, String),
    pub namespaces: [(String, String); 7],
    pub labels: (String, String),
    pub caller_limits: String,
    pub cgroup_path: String,
    pub memory: Vec<Option<String>>,
    pub swap: Vec<Option<String>>,
    pub tasks: Vec<Option<String>>,
    pub scheduler_stat: String,
}

pub(in crate::checker_process) fn decimal(text: &str) -> Result<u64, String> {
    if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err("malformed-number".into());
    }
    text.parse().map_err(|_| "malformed-number".into())
}

pub(in crate::checker_process) fn manager_pid(text: &str) -> Result<u32, String> {
    let fields: Vec<_> = text.split_whitespace().collect();
    if fields.len() != 2 || fields[0] != "u" {
        return Err("manager-pid-malformed".into());
    }
    decimal(fields[1])
        .ok()
        .and_then(|v| u32::try_from(v).ok())
        .filter(|v| *v > 0)
        .ok_or_else(|| "manager-pid-malformed".into())
}

fn stat_field(text: &str, index: usize) -> Result<&str, String> {
    text.rsplit_once(')')
        .and_then(|(_, tail)| tail.split_whitespace().nth(index))
        .ok_or_else(|| "stat-malformed".into())
}

pub(in crate::checker_process) fn stable_manager(stats: &(String, String)) -> Result<(), String> {
    let first = decimal(stat_field(&stats.0, 19)?)?;
    let last = decimal(stat_field(&stats.1, 19)?)?;
    if first == 0 || first != last {
        return Err("manager-identity-changed".into());
    }
    Ok(())
}

fn namespace_id(name: &str, value: &str) -> Result<u64, String> {
    value
        .strip_prefix(&format!("{name}:["))
        .and_then(|s| s.strip_suffix(']'))
        .and_then(|s| decimal(s).ok())
        .filter(|v| *v != 0)
        .ok_or_else(|| format!("namespace-{name}-malformed"))
}

pub(in crate::checker_process) fn namespace_equal(
    name: &str,
    pair: &(String, String),
) -> Result<(), String> {
    let caller = namespace_id(name, &pair.0)?;
    let manager = namespace_id(name, &pair.1)?;
    if caller != manager {
        return Err(format!("namespace-{name}-mismatch"));
    }
    Ok(())
}

pub(in crate::checker_process) fn labels_equal(pair: &(String, String)) -> Result<(), String> {
    let valid = |s: &str| {
        let s = s.strip_suffix('\n').unwrap_or(s);
        !s.is_empty() && s.len() <= 4096 && !s.chars().any(char::is_control)
    };
    if !valid(&pair.0) || !valid(&pair.1) {
        return Err("lsm-label-malformed".into());
    }
    if pair.0.strip_suffix('\n').unwrap_or(&pair.0) != pair.1.strip_suffix('\n').unwrap_or(&pair.1)
    {
        return Err("lsm-label-mismatch".into());
    }
    Ok(())
}

pub(in crate::checker_process) fn cgroup_root(path: &str, namespace: &str) -> Result<(), String> {
    if !path.starts_with('/')
        || path.contains('\n')
        || path.contains('\0')
        || path
            .split('/')
            .skip(1)
            .any(|s| s == "." || s == ".." || (s.is_empty() && path != "/"))
    {
        return Err("cgroup-path-malformed".into());
    }
    let ns = namespace_id("cgroup", namespace)?;
    // Linux UAPI CGROUP_NS_INIT_INO (include/uapi/linux/nsfs.h). Namespace
    // equality with a container's PID 1 would not establish initial membership.
    if path == "/" && ns != 0xeffffffb {
        return Err("cgroup-namespace-root".into());
    }
    Ok(())
}

fn limit(text: &str) -> Result<Option<u64>, String> {
    match text {
        "unlimited" => Ok(None),
        _ => decimal(text).map(Some),
    }
}

pub(in crate::checker_process) fn budget(
    values: &[Option<String>],
    required: u64,
    name: &str,
) -> Result<(), String> {
    let mut observed = false;
    for text in values {
        let text = text.as_ref().ok_or_else(|| format!("{name}-unobserved"))?;
        observed = true;
        let value = match text.trim() {
            "max" => None,
            s => Some(decimal(s).map_err(|_| format!("{name}-malformed"))?),
        };
        if value.is_some_and(|v| v < required) {
            return Err(format!("{name}-tighter-than-unit"));
        }
    }
    if !observed {
        return Err(format!("{name}-unobserved"));
    }
    Ok(())
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

/// Startup readback is decisive; the manager process is not the unit default.
/// Explicit selection retains the owner's override of host-policy conditions.
pub(in crate::checker_process) fn verify_service_limits(
    mode: &str,
    caller: Option<&str>,
    actual: &str,
) -> Result<(), String> {
    match mode {
        "explicit" => return Ok(()),
        "automatic" => (),
        _ => return Err("unknown-host-policy-mode".into()),
    }
    let caller = parse_limits(caller.ok_or("missing-caller-rlimits")?)?;
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

pub(in crate::checker_process) fn acquire(
    manager_response: String,
    proc_root: &Path,
    root: &Path,
    path: PathBuf,
) -> Result<Observations, String> {
    let pid = manager_pid(&manager_response)?;
    let manager = proc_root.join(pid.to_string());
    let caller = proc_root.join("self");
    let read = |p: &Path, reason: &str| std::fs::read_to_string(p).map_err(|_| reason.to_owned());
    let before = read(&manager.join("stat"), "manager-identity-unreadable")?;
    let mut namespaces: [(String, String); 7] =
        std::array::from_fn(|_| (String::new(), String::new()));
    for (i, name) in NAMESPACES.iter().enumerate() {
        let link = |p: &Path| {
            std::fs::read_link(p)
                .ok()
                .and_then(|p| p.to_str().map(str::to_owned))
                .ok_or_else(|| format!("namespace-{name}-unreadable"))
        };
        namespaces[i] = (
            link(&caller.join("ns").join(name))?,
            link(&manager.join("ns").join(name))?,
        );
    }
    let labels = (
        read(&caller.join("attr/current"), "lsm-label-unreadable")?,
        read(&manager.join("attr/current"), "lsm-label-unreadable")?,
    );
    let caller_limits = read(&caller.join("limits"), "rlimits-unreadable")?;
    let scheduler_stat = read(&caller.join("stat"), "cpu-scheduler-unreadable")?;
    let cgroup_path = path
        .strip_prefix(root)
        .map_err(|_| "cgroup-path-malformed")?;
    let cgroup_path = format!("/{}", cgroup_path.to_str().ok_or("cgroup-path-malformed")?);
    let mut memory = Vec::new();
    let mut swap = Vec::new();
    let mut tasks = Vec::new();
    let mut at = path;
    loop {
        if !at.starts_with(root) || !at.join("cgroup.procs").is_file() {
            return Err("cgroup-ancestors-unavailable".into());
        }
        for (name, out) in [
            ("memory.max", &mut memory),
            ("memory.swap.max", &mut swap),
            ("pids.max", &mut tasks),
        ] {
            match std::fs::read_to_string(at.join(name)) {
                Ok(text) => out.push(Some(text)),
                Err(e)
                    if e.kind() == std::io::ErrorKind::NotFound
                        && at == root
                        && namespace_id("cgroup", &namespaces[6].0)? == 0xeffffffb => {}
                Err(_) => return Err(format!("{name}-unreadable")),
            }
        }
        if at == root {
            break;
        }
        if !at.pop() {
            return Err("cgroup-ancestors-unavailable".into());
        }
    }
    let after = read(&manager.join("stat"), "manager-identity-unreadable")?;
    Ok(Observations {
        manager_pid: manager_response,
        manager_stat: (before, after),
        namespaces,
        labels,
        caller_limits,
        cgroup_path,
        memory,
        swap,
        tasks,
        scheduler_stat,
    })
}

#[cfg(test)]
pub(in crate::checker_process) fn fixture() -> Observations {
    let stat = |policy| {
        let mut fields = vec!["0".to_owned(); 50];
        fields[0] = "S".into();
        fields[19] = "123".into();
        fields[38] = policy;
        format!("42 (manager) {}", fields.join(" "))
    };
    let limits = "Limit Soft Limit Hard Limit Units\n\
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
        .to_owned();
    Observations {
        manager_pid: "u 42\n".into(),
        manager_stat: (stat("0".into()), stat("0".into())),
        namespaces: std::array::from_fn(|i| {
            let v = format!(
                "{}:[{}]",
                NAMESPACES[i],
                if i == 6 {
                    0xeffffffb_u64
                } else {
                    100 + i as u64
                }
            );
            (v.clone(), v)
        }),
        labels: ("unconfined\n".into(), "unconfined\n".into()),
        caller_limits: limits,
        cgroup_path: "/caller.scope".into(),
        memory: vec![Some("67108864".into()), Some("max".into())],
        swap: vec![Some("0".into())],
        tasks: vec![Some("64".into())],
        scheduler_stat: stat("0".into()),
    }
}
