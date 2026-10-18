//! Invocation-local process-shell enforcement; no decision or publication capability.
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

// Diagnostic-only runs: POSIX search stages took3..22ms idle and17..170ms
// with eight same-core burners (about8x observed wall inflation). Nine shares
// plus2s scheduling/protocol/I/O margin gives20s for a full2s CPU check.
// Real1s CPU-spin controls measured1.028..1.029s idle and9.005..9.019s
// loaded; a600ms waiting control used0 observed CPU ticks and was accepted.
// Systemd loaded-stage measurement is unavailable in the sandbox (user bus
// EPERM); this conservative sizing remains subject to host replay. The25s
// worker and30s guardian caps never move. Offline deadlines are explicitly
// wall-time policy and retain their original requested+1s outer grace.
// Sources: Linux proc_pid_stat(5), getrusage(2), getauxval(3), and systemd
// resource-control(5): CPU measures scheduled work, not waiting or I/O.
const CPU_WALL_FACTOR: u32 = 9;
const WALL_MARGIN: Duration = Duration::from_secs(2);

/// The admitted POSIX finite-data worker runs trusted code/threads only;
/// native/interpreter/tree profiles still refuse. Its process stat includes
/// all worker threads, not arbitrary live descendants. Child CPU refuses.
/// Systemd's verified cgroup cpu.stat covers the complete contained tree.
enum CpuCounter {
    Process { pid: u32, start: u64, hz: u64 },
    Cgroup(PathBuf),
}

fn process_stat(text: &str, pid: u32) -> Result<(u64, u64), String> {
    let (prefix, rest) = text.rsplit_once(')').ok_or("stage-cpu-stat")?;
    if prefix
        .split_whitespace()
        .next()
        .and_then(|s| s.parse::<u32>().ok())
        != Some(pid)
    {
        return Err("stage-cpu-identity".into());
    }
    let f: Vec<_> = rest.split_whitespace().collect();
    let number = |i: usize| {
        f.get(i)
            .and_then(|s| s.parse::<u64>().ok())
            .ok_or_else(|| "stage-cpu-stat".to_owned())
    };
    if number(2)? != u64::from(pid) {
        return Err("stage-cpu-group".into());
    }
    // FiniteData admits no child-spawning work. Waited-child counters are
    // not a substitute for live-tree accounting; refuse a violated invariant.
    if number(13)? != 0 || number(14)? != 0 {
        return Err("stage-cpu-unexpected-child".into());
    }
    let ticks = [11, 12].into_iter().try_fold(0u64, |sum, i| {
        sum.checked_add(number(i)?)
            .ok_or_else(|| "stage-cpu-overflow".to_owned())
    })?;
    Ok((number(19)?, ticks))
}

fn clock_ticks(bytes: &[u8]) -> Result<u64, String> {
    const N: usize = size_of::<usize>();
    if bytes.len() % (2 * N) != 0 {
        return Err("stage-cpu-auxv".into());
    }
    for pair in bytes.chunks_exact(2 * N) {
        let kind = usize::from_ne_bytes(pair[..N].try_into().map_err(|_| "stage-cpu-auxv")?);
        let value = usize::from_ne_bytes(pair[N..].try_into().map_err(|_| "stage-cpu-auxv")?);
        if kind == 0 {
            break;
        }
        if kind == nix::libc::AT_CLKTCK as usize && value > 0 {
            return Ok(value as u64);
        }
    }
    Err("stage-cpu-clock-ticks".into())
}

fn cgroup_usage(text: &str) -> Result<u64, String> {
    let mut values = text
        .lines()
        .filter_map(|line| line.strip_prefix("usage_usec "));
    let value = values
        .next()
        .and_then(|s| s.trim().parse().ok())
        .ok_or("stage-cpu-cgroup-stat")?;
    if values.next().is_some() {
        return Err("stage-cpu-cgroup-stat".into());
    }
    Ok(value)
}

fn process_finish_uncertainty_us(hz: u64) -> u64 {
    2 * 1_000_000u64.div_ceil(hz) + u64::from(1_000_000 % hz != 0)
}

fn check_resolution(requested_ms: u64, uncertainty_us: u64) -> Result<(), String> {
    if u128::from(requested_ms) * 1000 < u128::from(uncertainty_us) {
        Err(super::STAGE_CPU_RESOLUTION.into())
    } else {
        Ok(())
    }
}

/// Worker-side refusal only; the supervisor repeats this from its own counter.
pub(super) fn check_posix_resolution(requested_ms: u64) -> Result<(), String> {
    let hz = clock_ticks(&fs::read("/proc/self/auxv").map_err(|_| "stage-cpu-auxv")?)?;
    check_resolution(requested_ms, process_finish_uncertainty_us(hz))
}

impl CpuCounter {
    fn new(pid: Option<u32>, group: Option<&Path>) -> Result<Self, String> {
        if let Some(pid) = pid {
            let text = fs::read_to_string(format!("/proc/{pid}/stat"))
                .map_err(|_| "stage-cpu-process-unavailable")?;
            let (start, _) = process_stat(&text, pid)?;
            let hz = clock_ticks(&fs::read("/proc/self/auxv").map_err(|_| "stage-cpu-auxv")?)?;
            Ok(Self::Process { pid, start, hz })
        } else {
            Ok(Self::Cgroup(
                group.ok_or("stage-cpu-group-unavailable")?.join("cpu.stat"),
            ))
        }
    }
    // /proc exposes two individually quantized process CPU counters. On
    // FINISH charge their strict upper uncertainty; zero waited-child CPU is
    // an independently checked finite-worker invariant, not two extra clocks.
    // cpu.stat usage_usec is one counter rounded to microseconds.
    fn finish_uncertainty_us(&self) -> u64 {
        match self {
            Self::Process { hz, .. } => process_finish_uncertainty_us(*hz),
            Self::Cgroup(_) => 1,
        }
    }
    fn sample(&self) -> Result<u64, String> {
        match self {
            Self::Process { pid, start, hz } => {
                let text = fs::read_to_string(format!("/proc/{pid}/stat"))
                    .map_err(|_| "stage-cpu-process-unavailable")?;
                let (actual, ticks) = process_stat(&text, *pid)?;
                if actual != *start {
                    return Err("stage-cpu-identity".into());
                }
                u64::try_from(u128::from(ticks) * 1_000_000 / u128::from(*hz))
                    .map_err(|_| "stage-cpu-overflow".into())
            }
            Self::Cgroup(path) => {
                cgroup_usage(&fs::read_to_string(path).map_err(|_| "stage-cpu-cgroup-unavailable")?)
            }
        }
    }
}

/// Exclusively owned by one supervisor until FINISH or refusal. Kernel
/// counters alone supply operational CPU usage; operator JSON is write-only.
/// A failed/missing/regressed counter refuses, never falls back to a pass.
pub(super) struct StageBudget {
    counter: Option<CpuCounter>,
    baseline: u64,
    previous: u64,
    progress: Instant,
    max_no_progress: Duration,
    budget: u64,
    started: Instant,
    active: Instant,
    wall: Instant,
    kind: &'static str,
    requested: u64,
    stopped_by: Option<&'static str>,
}
impl StageBudget {
    pub(super) fn new(
        kind: &'static str,
        requested: u64,
        started: Instant,
        end: Instant,
        offline: bool,
        pid: Option<u32>,
        group: Option<&Path>,
    ) -> Result<Self, String> {
        let counter = if offline {
            None
        } else {
            Some(CpuCounter::new(pid, group)?)
        };
        if let Some(counter @ CpuCounter::Process { .. }) = &counter {
            check_resolution(requested, counter.finish_uncertainty_us())?;
        }
        let baseline = counter.as_ref().map_or(Ok(0), CpuCounter::sample)?;
        let active = Instant::now();
        let span = if offline {
            Duration::from_millis(requested) + Duration::from_secs(1)
        } else {
            Duration::from_millis(requested) * CPU_WALL_FACTOR + WALL_MARGIN
        };
        Ok(Self {
            counter,
            baseline,
            previous: baseline,
            progress: active,
            max_no_progress: Duration::ZERO,
            budget: requested.checked_mul(1000).ok_or("stage-cpu-overflow")?,
            started,
            active,
            wall: (active + span).min(end),
            kind,
            requested,
            stopped_by: None,
        })
    }
    pub(super) fn wall_deadline(&self) -> Instant {
        self.wall
    }
    pub(super) fn check(&mut self, now: Instant) -> Result<(), String> {
        if now >= self.wall {
            self.stopped_by = Some("wall");
            return Err("contained-deadline".into());
        }
        if let Some(counter) = &self.counter {
            let current = counter.sample()?;
            if current < self.previous {
                return Err("stage-cpu-regressed".into());
            }
            self.max_no_progress = self.max_no_progress.max(self.progress.elapsed());
            if current > self.previous {
                self.progress = Instant::now();
            }
            self.previous = current;
            if current
                .checked_sub(self.baseline)
                .ok_or("stage-cpu-regressed")?
                >= self.budget
            {
                self.stopped_by = Some("cpu");
                return Err("contained-deadline".into());
            }
        }
        // A descheduled/blocking procfs read must not admit a late result.
        if Instant::now() >= self.wall {
            self.stopped_by = Some("wall");
            return Err("contained-deadline".into());
        }
        Ok(())
    }
    /// Call only after servicing the control socket. A queued FINISH is real
    /// protocol progress, even if this supervisor was descheduled for seconds.
    /// Partial/invalid frames never reset this clock. CPU/wall checks remain
    /// independent and also run at FINISH before any result permit.
    pub(super) fn check_progress(&mut self) -> Result<(), String> {
        if self.counter.is_some()
            && Instant::now()
                >= self.progress + Duration::from_millis(self.requested) + Duration::from_secs(1)
        {
            self.stopped_by = Some("no-cpu-or-protocol-progress");
            return Err("contained-deadline".into());
        }
        Ok(())
    }
    pub(super) fn permitted(&mut self) {
        self.progress = Instant::now();
    }
    pub(super) fn finish(&mut self) -> Result<(), String> {
        self.check(Instant::now())?;
        if let Some(counter) = &self.counter {
            let charge = (self.previous - self.baseline)
                .checked_add(counter.finish_uncertainty_us())
                .ok_or("stage-cpu-overflow")?;
            // Actual CPU is strictly below this rounded upper bound. Equal
            // upper bound is safe; raw observed >=budget already refused.
            if charge > self.budget {
                self.stopped_by = Some("cpu-finish-bound");
                return Err("contained-deadline".into());
            }
        }
        Ok(())
    }
    pub(super) fn observation(&self, finished: bool) -> Value {
        json!({"kind":self.kind,"requested_ms":self.requested,
            "active_ms":self.active.duration_since(self.started).as_millis() as u64,
            "wall_deadline_ms":self.wall.duration_since(self.started).as_millis() as u64,
            "observed_ms":self.started.elapsed().as_millis() as u64,
            "last_progress_ms":self.progress.duration_since(self.started).as_millis() as u64,
            "no_cpu_or_protocol_progress_budget_ms":self.counter.as_ref().map(|_|self.requested+1000),
            "no_cpu_or_protocol_progress_deadline_ms":self.counter.as_ref().map(|_|(self.progress+Duration::from_millis(self.requested)+Duration::from_secs(1)).duration_since(self.started).as_millis() as u64),
            "max_no_cpu_progress_ms":self.max_no_progress.as_millis() as u64,
            "wall_elapsed_ms":self.active.elapsed().as_millis() as u64,
            "cpu_source":match &self.counter { Some(CpuCounter::Process {..})=>"owned-finite-worker-proc-stat",Some(CpuCounter::Cgroup(_))=>"contained-cgroup-cpu.stat",None=>"offline-wall-policy" },
            "cpu_resolution_us":match &self.counter { Some(CpuCounter::Process {hz,..})=>Some(1_000_000/hz),Some(CpuCounter::Cgroup(_))=>Some(1),None=>None },
            "cpu_finish_uncertainty_us":self.counter.as_ref().map(CpuCounter::finish_uncertainty_us),
            "cpu_budget_us":self.counter.as_ref().map(|_|self.budget),
            "cpu_baseline_us":self.counter.as_ref().map(|_|self.baseline),
            "cpu_observed_us":self.counter.as_ref().map(|_|self.previous-self.baseline),
            "finished":finished,"stopped_by":self.stopped_by})
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn posix_small_cpu_requests_refuse_below_the_exact_finish_boundary() {
        let uncertainty = process_finish_uncertainty_us(100);
        assert_eq!(uncertainty, 20_000);
        for requested in [18, 19] {
            assert_eq!(
                check_resolution(requested, uncertainty),
                Err(super::super::STAGE_CPU_RESOLUTION.into())
            );
        }
        // The strict upper bound may equal the budget. Admission is not a
        // FINISH permit: any observed tick still makes this request refuse.
        assert_eq!(check_resolution(20, uncertainty), Ok(()));
        assert_eq!(check_resolution(21, uncertainty), Ok(()));
        assert_eq!(check_resolution(u64::MAX, uncertainty), Ok(()));
        assert!(10_000 + uncertainty > 20_000);
        assert_eq!(process_finish_uncertainty_us(1000), 2000);
        assert_eq!(check_resolution(2, 2000), Ok(()));
    }
    #[test]
    fn supervisor_independently_refuses_small_posix_stages_at_admission() {
        use std::os::unix::process::CommandExt;
        let mut child = std::process::Command::new("sleep")
            .arg("10")
            .process_group(0)
            .spawn()
            .unwrap();
        let now = Instant::now();
        let results: Vec<_> = [18, 19, 20]
            .into_iter()
            .map(|requested| {
                StageBudget::new(
                    "check",
                    requested,
                    now,
                    now + Duration::from_secs(25),
                    false,
                    Some(child.id()),
                    None,
                )
                .map(|s| s.observation(false))
            })
            .collect();
        child.kill().unwrap();
        child.wait().unwrap();
        assert_eq!(results[0], Err(super::super::STAGE_CPU_RESOLUTION.into()));
        assert_eq!(results[1], Err(super::super::STAGE_CPU_RESOLUTION.into()));
        let boundary = results[2].as_ref().unwrap();
        assert_eq!(boundary["cpu_finish_uncertainty_us"], 20_000);
        assert_eq!(boundary["cpu_budget_us"], 20_000);
        assert_eq!(
            boundary["finished"], false,
            "admission cannot credit a verdict"
        );
    }
    #[test]
    fn cpu_stat_is_exact_and_missing_duplicate_or_overflowed_usage_refuses() {
        assert_eq!(
            cgroup_usage("usage_usec 123\nuser_usec 99\nsystem_usec 24\n"),
            Ok(123)
        );
        for text in [
            "",
            "user_usec 12",
            "usage_usec -1",
            "usage_usec 18446744073709551616",
            "usage_usec 1\nusage_usec 2",
        ] {
            assert!(cgroup_usage(text).is_err());
        }
        let mut fields = vec!["0"; 21];
        fields[2] = "42";
        fields[11] = "3";
        fields[12] = "4";
        fields[13] = "0";
        fields[14] = "0";
        fields[19] = "789";
        let stat = format!("42 (space ) name) {}", fields.join(" "));
        assert_eq!(process_stat(&stat, 42), Ok((789, 7)));
        assert!(process_stat(&stat, 43).is_err());
        assert_eq!(
            process_stat(&stat.replace(" 3 4 0 0 ", " 3 4 1 0 "), 42),
            Err("stage-cpu-unexpected-child".into())
        );
        assert_eq!(
            CpuCounter::Process {
                pid: 42,
                start: 789,
                hz: 100
            }
            .finish_uncertainty_us(),
            20_000
        );
        assert!(
            process_stat(
                &stat.replace(" 3 4 0 0 ", " 18446744073709551615 4 0 0 "),
                42
            )
            .is_err()
        );
        assert!(clock_ticks(&[]).is_err());
        assert!(clock_ticks(&[0]).is_err());
        assert!(clock_ticks(&fs::read("/proc/self/auxv").unwrap()).unwrap() > 0);
    }
    #[test]
    fn queued_finish_is_progress_but_idle_without_a_frame_refuses() {
        let path = std::env::temp_dir().join(format!(
            "zeno-stage-progress-{}-{}",
            std::process::id(),
            crate::TEMP_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::write(path.join("cpu.stat"), "usage_usec 1000\n").unwrap();
        let now = Instant::now();
        let mut stage = StageBudget::new(
            "search",
            1000,
            now,
            now + Duration::from_secs(25),
            false,
            None,
            Some(&path),
        )
        .unwrap();
        stage.progress = now - Duration::from_secs(3);
        assert_eq!(stage.check_progress(), Err("contained-deadline".into()));
        assert_eq!(
            stage.observation(false)["stopped_by"],
            "no-cpu-or-protocol-progress"
        );
        assert!(
            stage.finish().is_ok(),
            "a queued valid FINISH can end idle, but must still pay independent CPU and wall checks"
        );
        fs::write(path.join("cpu.stat"), "usage_usec 1001000\n").unwrap();
        assert_eq!(stage.finish(), Err("contained-deadline".into()));
        assert_eq!(stage.observation(false)["stopped_by"], "cpu");
        fs::remove_dir_all(path).unwrap();
    }
    #[test]
    fn cpu_budget_finish_regression_missing_counter_and_wall_deadline_refuse() {
        let path = std::env::temp_dir().join(format!(
            "zeno-stage-cpu-{}-{}",
            std::process::id(),
            crate::TEMP_SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        let file = path.join("cpu.stat");
        fs::write(&file, "usage_usec 1000\n").unwrap();
        let now = Instant::now();
        let mut stage = StageBudget::new(
            "check",
            100,
            now,
            now + Duration::from_secs(25),
            false,
            None,
            Some(&path),
        )
        .unwrap();
        assert_eq!(
            stage.wall_deadline().duration_since(stage.active),
            Duration::from_millis(2900)
        );
        fs::write(&file, "usage_usec 100999\n").unwrap();
        assert!(stage.check(Instant::now()).is_ok());
        assert_eq!(stage.finish(), Ok(())); // Strict upper bound equals budget; actual CPU remains below it.
        fs::write(&file, "usage_usec 101000\n").unwrap();
        assert_eq!(
            stage.check(Instant::now()),
            Err("contained-deadline".into())
        );
        assert_eq!(stage.observation(false)["stopped_by"], "cpu");
        fs::write(&file, "usage_usec 1000\n").unwrap();
        assert_eq!(
            stage.check(Instant::now()),
            Err("stage-cpu-regressed".into())
        );
        fs::remove_file(&file).unwrap();
        assert_eq!(
            stage.check(Instant::now()),
            Err("stage-cpu-cgroup-unavailable".into())
        );
        assert_eq!(
            stage.check(stage.wall_deadline()),
            Err("contained-deadline".into())
        );
        assert_eq!(stage.observation(false)["stopped_by"], "wall");
        fs::remove_dir(&path).unwrap();
    }
}
