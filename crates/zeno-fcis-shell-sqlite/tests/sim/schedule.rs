#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Step {
    pub(crate) action: u8,
    pub(crate) fault: u8,
    pub(crate) choice: u64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct Schedule {
    pub(crate) seed: u64,
    pub(crate) steps: Vec<Step>,
}
/// SplitMix64, Steele/Vigna reference mixer. Seed zero starts with e220a8397b1dcdaf.
pub(crate) struct Random(pub(crate) u64);
impl Random {
    pub(crate) fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }
}
impl Schedule {
    pub(crate) fn seeded(seed: u64, length: usize) -> Self {
        let mut random = Random(seed);
        // Start with an observable commit/export. Then emphasize transport
        // retries while retaining every storage and refusal action. Repeated
        // full audits otherwise dominate this bounded debug-build campaign.
        const ACTIONS: [u8; 32] = [
            0, 1, 2, 3, 4, 5, 5, 5, 5, 5, 6, 7, 8, 9, 10, 11, 12, 12, 12, 12, 12, 12, 12, 12, 12,
            12, 12, 12, 12, 12, 12, 12,
        ];
        let steps = (0..length)
            .map(|index| {
                let selection = random.next();
                let fault = (random.next() % 9) as u8;
                let choice = random.next();
                Step {
                    action: match index {
                        0 => 0,
                        1 => 4,
                        _ => ACTIONS[selection as usize % ACTIONS.len()],
                    },
                    fault: if index == 0 { 0 } else { fault },
                    choice,
                }
            })
            .collect();
        Self { seed, steps }
    }
    // Restricted JSON schema: [seed, [[action, fault, choice], ...]].
    pub(crate) fn json(&self) -> String {
        format!(
            "[{},[{}]]\n",
            self.seed,
            self.steps
                .iter()
                .map(|s| format!("[{},{},{}]", s.action, s.fault, s.choice))
                .collect::<Vec<_>>()
                .join(",")
        )
    }
    pub(crate) fn parse(text: &str) -> Self {
        struct Parser<'a>(&'a str);
        impl Parser<'_> {
            fn take(&mut self, token: &str) {
                self.0 = self.0.trim_start();
                self.0 = self
                    .0
                    .strip_prefix(token)
                    .unwrap_or_else(|| panic!("schedule JSON token {token}"));
            }
            fn number(&mut self) -> u64 {
                self.0 = self.0.trim_start();
                let end = self
                    .0
                    .find(|c: char| !c.is_ascii_digit())
                    .unwrap_or(self.0.len());
                let n = super::ok(self.0[..end].parse());
                self.0 = &self.0[end..];
                n
            }
        }
        let mut p = Parser(text);
        p.take("[");
        let seed = p.number();
        p.take(",");
        p.take("[");
        let mut steps = Vec::new();
        while !p.0.trim_start().starts_with(']') {
            p.take("[");
            let action = super::ok(u8::try_from(p.number()));
            p.take(",");
            let fault = super::ok(u8::try_from(p.number()));
            p.take(",");
            let choice = p.number();
            p.take("]");
            assert!(action < 13 && fault < 9);
            steps.push(Step {
                action,
                fault,
                choice,
            });
            if !p.0.trim_start().starts_with(']') {
                p.take(",");
            }
        }
        p.take("]");
        p.take("]");
        assert!(p.0.trim().is_empty());
        Self { seed, steps }
    }
}
pub(crate) fn minimize(mut steps: Vec<Step>, mut fails: impl FnMut(&[Step]) -> bool) -> Vec<Step> {
    let mut granularity = 2;
    while !steps.is_empty() {
        let width = steps.len().div_ceil(granularity);
        let mut reduced = false;
        for start in (0..steps.len()).step_by(width) {
            let candidate: Vec<_> = steps
                .iter()
                .enumerate()
                .filter(|(i, _)| *i < start || *i >= start + width)
                .map(|(_, s)| *s)
                .collect();
            if fails(&candidate) {
                steps = candidate;
                granularity = 2;
                reduced = true;
                break;
            }
        }
        if !reduced {
            if granularity >= steps.len() {
                break;
            }
            granularity = (granularity * 2).min(steps.len());
        }
    }
    steps
}
