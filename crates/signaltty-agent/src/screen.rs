//! Screen-detection rules (`[[screen]]` in a manifest): herdr-style
//! regexes over the pane title or the bottom of the visible screen.
//! ADR-0006's last-resort layer; the server only consults them for panes
//! that have seen no hook. Pure: compiled once at manifest load. See
//! docs/07, ADR-0024.

use regex::Regex;
use serde::Deserialize;

/// One `[[screen]]` table as written. Unknown fields ignored.
#[derive(Debug, Clone, Deserialize)]
pub struct ScreenRuleSpec {
    pub id: String,
    pub state: String,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub lines: Option<usize>,
    pub regex: Vec<String>,
    #[serde(default)]
    pub priority: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenState {
    Working,
    Blocked,
    Idle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Region {
    Title,
    Bottom(usize),
}

#[derive(Debug, Clone)]
pub struct ScreenRule {
    pub id: String,
    pub state: ScreenState,
    pub priority: i32,
    region: Region,
    regexes: Vec<Regex>,
}

const DEFAULT_LINES: usize = 12;
const MAX_LINES: usize = 200;

impl ScreenRule {
    pub fn compile(spec: &ScreenRuleSpec) -> Result<ScreenRule, String> {
        let at = |msg: String| format!("[[screen]] '{}': {msg}", spec.id);
        let state = match spec.state.as_str() {
            "working" => ScreenState::Working,
            "blocked" => ScreenState::Blocked,
            "idle" => ScreenState::Idle,
            other => return Err(at(format!("bad state '{other}'"))),
        };
        let region = match (spec.region.as_deref().unwrap_or("bottom"), spec.lines) {
            ("title", None) => Region::Title,
            ("title", Some(_)) => return Err(at("lines only applies to region bottom".into())),
            ("bottom", lines) => {
                let n = lines.unwrap_or(DEFAULT_LINES);
                if !(1..=MAX_LINES).contains(&n) {
                    return Err(at(format!("lines must be 1..={MAX_LINES}")));
                }
                Region::Bottom(n)
            }
            (other, _) => return Err(at(format!("bad region '{other}'"))),
        };
        if spec.regex.is_empty() {
            return Err(at("needs at least one regex".into()));
        }
        let regexes = spec
            .regex
            .iter()
            .map(|r| Regex::new(r).map_err(|e| at(format!("bad regex: {e}"))))
            .collect::<Result<_, _>>()?;
        Ok(ScreenRule {
            id: spec.id.clone(),
            state,
            priority: spec.priority,
            region,
            regexes,
        })
    }

    fn matches(&self, title: &str, screen: &str) -> bool {
        let bottom;
        let text = match self.region {
            Region::Title => title,
            Region::Bottom(n) => {
                let lines: Vec<&str> = screen
                    .lines()
                    .map(str::trim_end)
                    .filter(|l| !l.is_empty())
                    .collect();
                bottom = lines[lines.len().saturating_sub(n)..].join("\n");
                &bottom
            }
        };
        self.regexes.iter().any(|r| r.is_match(text))
    }
}

/// Highest-priority matching rule; ties keep the first in iteration order.
pub fn classify<'a>(
    rules: impl IntoIterator<Item = &'a ScreenRule>,
    title: &str,
    screen: &str,
) -> Option<&'a ScreenRule> {
    let mut best: Option<&ScreenRule> = None;
    for rule in rules {
        if best.is_none_or(|b| rule.priority > b.priority) && rule.matches(title, screen) {
            best = Some(rule);
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rule(
        id: &str,
        state: &str,
        region: Option<&str>,
        lines: Option<usize>,
        re: &[&str],
        priority: i32,
    ) -> ScreenRule {
        ScreenRule::compile(&ScreenRuleSpec {
            id: id.into(),
            state: state.into(),
            region: region.map(Into::into),
            lines,
            regex: re.iter().map(|s| (*s).to_string()).collect(),
            priority,
        })
        .unwrap()
    }

    fn compile_err(state: &str, region: Option<&str>, lines: Option<usize>, re: &[&str]) -> String {
        ScreenRule::compile(&ScreenRuleSpec {
            id: "r".into(),
            state: state.into(),
            region: region.map(Into::into),
            lines,
            regex: re.iter().map(|s| (*s).to_string()).collect(),
            priority: 0,
        })
        .unwrap_err()
    }

    #[test]
    fn invalid_rules_are_rejected() {
        assert!(compile_err("busy", None, None, &["x"]).contains("state"));
        assert!(compile_err("working", Some("top"), None, &["x"]).contains("region"));
        assert!(compile_err("working", None, Some(0), &["x"]).contains("lines"));
        assert!(compile_err("working", None, Some(201), &["x"]).contains("lines"));
        assert!(compile_err("working", Some("title"), Some(3), &["x"]).contains("lines"));
        assert!(compile_err("working", None, None, &[]).contains("regex"));
        assert!(compile_err("working", None, None, &["("]).contains("regex"));
    }

    #[test]
    fn bottom_region_sees_only_the_last_non_empty_lines() {
        let screen = "Allow? [y/n]\nold output\n\nmore output\n   \n";
        let r = rule("ask", "blocked", Some("bottom"), Some(2), &[r"Allow\?"], 0);
        assert!(!r.matches("", screen));
        let r = rule("ask", "blocked", Some("bottom"), Some(3), &[r"Allow\?"], 0);
        assert!(r.matches("", screen));
        // Default region is bottom(12); line anchors work per line.
        let r = rule("out", "working", None, None, &[r"(?m)^more output$"], 0);
        assert!(r.matches("", screen));
    }

    #[test]
    fn title_region_ignores_the_screen() {
        let r = rule("spin", "working", Some("title"), None, &["^[⠋⠙⠹] "], 0);
        assert!(r.matches("⠙ thinking", "idle prompt"));
        assert!(!r.matches("claude", "⠙ thinking"));
    }

    #[test]
    fn any_regex_matches() {
        let r = rule("two", "idle", None, None, &["^nope$", r"\$$"], 0);
        assert!(r.matches("", "user@host $ "));
    }

    #[test]
    fn highest_priority_wins_and_ties_keep_order() {
        let low = rule("low", "working", None, None, &["esc to interrupt"], 1);
        let high = rule("high", "blocked", None, None, &["Do you want"], 5);
        let tie = rule("tie", "idle", None, None, &["Do you want"], 5);
        let screen = "Do you want to proceed?\nesc to interrupt";
        let rules = [low, high, tie];
        assert_eq!(
            classify(&rules, "", screen).map(|r| r.id.as_str()),
            Some("high")
        );
        assert_eq!(
            classify(&rules, "", "esc to interrupt").map(|r| r.state),
            Some(ScreenState::Working)
        );
        assert!(classify(&rules, "", "nothing here").is_none());
    }
}
