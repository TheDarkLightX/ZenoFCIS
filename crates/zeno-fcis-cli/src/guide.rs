//! Offline task routing over embedded public command cards.
use clap::Subcommand;
use serde_json::{Value, json};
use std::collections::BTreeSet;

#[derive(Subcommand)]
pub(crate) enum Command {
    /// List public cards without reading a workspace or knowledge layer.
    List,
    /// Print one complete card, including commands, verdicts and scope.
    Show { name: String },
    /// Rank cards deterministically by their declared task phrases.
    Find {
        #[arg(required = true)]
        task: Vec<String>,
    },
}

const CARDS: &[&str] = &[
    include_str!("../guide-cards/adversarial-synthesis-duel.txt"),
    include_str!("../guide-cards/check.txt"),
    include_str!("../guide-cards/contract-collections.txt"),
    include_str!("../guide-cards/contract-distribution.txt"),
    include_str!("../guide-cards/contract-gaps.txt"),
    include_str!("../guide-cards/contract-lemma.txt"),
    include_str!("../guide-cards/contract-review.txt"),
    include_str!("../guide-cards/contract-scenario.txt"),
    include_str!("../guide-cards/generate.txt"),
    include_str!("../guide-cards/knowledge.txt"),
    include_str!("../guide-cards/loop-workspace.txt"),
    include_str!("../guide-cards/new.txt"),
    include_str!("../guide-cards/program-search.txt"),
    include_str!("../guide-cards/prove.txt"),
    include_str!("../guide-cards/synth.txt"),
    include_str!("../guide-cards/trace-game.txt"),
    include_str!("../guide-cards/transform.txt"),
    include_str!("../guide-cards/zal.txt"),
];

fn metadata(text: &str) -> Option<Value> {
    serde_json::from_str(text.strip_prefix("```json\n")?.split_once("\n```")?.0).ok()
}

fn words(text: &str) -> BTreeSet<String> {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_ascii_lowercase)
        .collect()
}

fn summary(card: &Value) -> Value {
    json!({"name":card["name"], "title":card["title"], "synopsis":card["synopsis"], "use_when":card["use_when"]})
}

fn lookup(command: Command) -> Result<Value, u8> {
    let cards: Vec<_> = CARDS
        .iter()
        .filter_map(|text| metadata(text).map(|m| (*text, m)))
        .collect();
    match command {
        Command::Show { name } => {
            let Some((text, _)) = cards
                .iter()
                .find(|(_, card)| card["name"].as_str() == Some(&name))
            else {
                return Err(crate::USAGE);
            };
            Ok(json!({"schema":"zeno-fcis/guide/1", "authority":"none", "card":text}))
        }
        Command::List => Ok(
            json!({"schema":"zeno-fcis/guide/1", "authority":"none", "cards":cards.iter().map(|(_, card)| summary(card)).collect::<Vec<_>>()}),
        ),
        Command::Find { task } => {
            let query = words(&task.join(" "));
            let mut ranked = Vec::new();
            for (_, card) in &cards {
                let mut score = 0;
                if let Some(phrases) = card["use_when"].as_array() {
                    for phrase in phrases.iter().filter_map(Value::as_str) {
                        let terms = words(phrase);
                        let overlap = query.intersection(&terms).count();
                        score = score
                            .max(overlap * 2 + usize::from(!query.is_empty() && query == terms));
                    }
                }
                if score > 0 {
                    ranked.push((score, card));
                }
            }
            ranked.sort_by(|(a, x), (b, y)| {
                b.cmp(a)
                    .then_with(|| x["name"].as_str().cmp(&y["name"].as_str()))
            });
            Ok(
                json!({"schema":"zeno-fcis/guide/1", "authority":"none", "ranking":"best-use-when-token-overlap/1", "cards":ranked.into_iter().map(|(score, card)| { let mut row=summary(card); row["score"]=json!(score); row }).collect::<Vec<_>>()}),
            )
        }
    }
}

pub(crate) fn run(command: Command) -> u8 {
    let show = matches!(&command, Command::Show { .. });
    match lookup(command) {
        Ok(report) => {
            if show {
                print!("{}", report["card"].as_str().unwrap_or_default());
            } else {
                crate::print_json(&report);
            }
            crate::OK
        }
        Err(code) => {
            crate::print_json(
                &json!({"schema":"zeno-fcis/guide/1", "authority":"none", "status":"unknown-card", "next":"Run zeno-fcis guide list."}),
            );
            code
        }
    }
}

pub(crate) fn mcp_tools() -> Vec<Value> {
    [
        ("guide_list", "List embedded public checker cards without reading a project.", json!({}), json!([])),
        ("guide_show", "Read one complete public command card.", json!({"name":{"type":"string","maxLength":80}}), json!(["name"])),
        ("guide_find", "Rank public task phrases deterministically.", json!({"task":{"type":"string","maxLength":4096}}), json!(["task"])),
    ].into_iter().map(|(name, description, properties, required)| json!({
        "name":name, "description":description,
        "inputSchema":{"type":"object","additionalProperties":false,"properties":properties,"required":required},
        "annotations":{"readOnlyHint":true,"destructiveHint":false,"openWorldHint":false}
    })).collect()
}

pub(crate) fn mcp_call(
    name: &str,
    arguments: &Value,
) -> Result<Value, zeno_fcis_commons::CommonsError> {
    use zeno_fcis_commons::CommonsError;
    let text = |key: &str| arguments.get(key).and_then(Value::as_str);
    let command = match name {
        "guide_list" => Command::List,
        "guide_show" => {
            let value = text("name")
                .ok_or_else(|| CommonsError::new("Use an exact card name from guide_list."))?;
            if value.is_empty()
                || value.len() > 80
                || !value.as_bytes()[0].is_ascii_lowercase()
                || !value
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
            {
                return Err(CommonsError::new("Use an exact card name from guide_list."));
            }
            Command::Show {
                name: value.to_owned(),
            }
        }
        "guide_find" => {
            let value = text("task")
                .ok_or_else(|| CommonsError::new("Supply 1-4096 characters of task words."))?;
            if value.trim().is_empty() || value.chars().count() > 4096 {
                return Err(CommonsError::new("Supply 1-4096 characters of task words."));
            }
            Command::Find {
                task: vec![value.to_owned()],
            }
        }
        _ => return Err(CommonsError::new(format!("unknown tool {name}"))),
    };
    lookup(command)
        .map_err(|_| CommonsError::new("Unknown card or unavailable guide; run guide list."))
}

/// Card commands and examples are also the source for command help.
pub(crate) fn with_help(mut command: clap::Command) -> clap::Command {
    let cards: Vec<_> = CARDS.iter().filter_map(|text| metadata(text)).collect();
    fn visit(command: &mut clap::Command, path: &str, cards: &[Value]) {
        let mut help = String::new();
        for card in cards {
            if card["commands"]
                .as_array()
                .is_some_and(|commands| commands.iter().any(|c| c.as_str() == Some(path)))
            {
                help.push_str(&format!(
                    "\nEngine: {}\nContract: {}\nLimits: {}\n",
                    card["engine"].as_str().unwrap_or("unknown"),
                    card["outputs"],
                    card["limits"]
                ));
                if let Some(steps) = card["steps"].as_array() {
                    for step in steps.iter().chain(std::iter::once(&card["example"])) {
                        if let Some(example) = step["command"].as_str() {
                            help.push_str(&format!("Example (repository root; WORK is fresh scratch):\n  {example}\n  exit {}; expected {}\n", step["exit"], step["expected"]));
                        }
                    }
                }
                help.push_str(&format!(
                    "See also: zeno-fcis guide show {}\n",
                    card["name"].as_str().unwrap_or("unknown")
                ));
            }
        }
        if !help.is_empty() {
            *command = command.clone().after_help(help);
        }
        for child in command.get_subcommands_mut() {
            let child_path = if path.is_empty() {
                child.get_name().to_owned()
            } else {
                format!("{path} {}", child.get_name())
            };
            visit(child, &child_path, cards);
        }
    }
    visit(&mut command, "", &cards);
    command
}

pub(crate) const LOOP_HELP: &str = "{about-with-newline}\n{usage-heading} {usage}\n\nLearn from examples:\n  capture      Capture an executable reference\n  learn        Enumerate a declared example vocabulary\n\nComplete and repair:\n  open         Admit an original and open a session\n  candidate    Check a supplied program or expressions\n  sketch       Complete holes and check the candidate\n  pipeline     Compose completion, repair and optimization\n\nOptimize:\n  run          Drive a bounded proposer\n  resume       Replay the retained incumbent\n  encode       Encode canonical program bytes\n  strategy     Select on development, freeze, then evaluate\n  rules        Recheck concrete rewrite proposals\n  family       Check every declared family instance\n  transfer     Propose on another declared instance\n  meaning      Compare complete finite observations\n  evaluate     Evaluate an expression; no verdict\n\nCheck restated rules:\n  teach-back   Compare restated rules with decisions\n\nAsk where laws are open:\n  question     Ask about the largest ambiguous region\n\nArchitecture:\n  decompose    Judge a proposed component decomposition\n\nOptions:\n{options}{after-help}";

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_embedded_card_has_an_exact_name_and_contract() {
        let mut names = BTreeSet::new();
        for text in CARDS {
            let card = metadata(text).unwrap_or_else(|| panic!("embedded metadata"));
            assert!(names.insert(card["name"].as_str().unwrap_or("").to_owned()));
            for field in [
                "synopsis",
                "engine",
                "inputs",
                "outputs",
                "exit_codes",
                "verdicts",
                "limits",
                "refusals",
                "cost",
                "use_when",
                "not_for",
                "example",
                "sources",
            ] {
                assert!(!card[field].is_null(), "missing {field}");
            }
        }
        assert_eq!(names.len(), 18);
    }
}
