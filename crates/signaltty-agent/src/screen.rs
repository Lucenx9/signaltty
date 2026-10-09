//! Screen-detection rules (`[[screen]]` in a manifest): herdr-style
//! regexes over the pane title or the bottom of the visible screen. A rule
//! matches when any `regex`, every `all` and no `not` regex matches.
//! ADR-0006's last-resort layer; the server only consults them for panes
//! that have seen no hook. Pure: compiled once at manifest load. See
//! docs/07, ADR-0024.

use regex::Regex;
use serde::Deserialize;

/// One `[[screen]]` table as written. Unknown fields ignored.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ScreenRuleSpec {
    pub id: String,
    pub state: String,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub lines: Option<usize>,
    #[serde(default)]
    pub regex: Vec<String>,
    #[serde(default)]
    pub all: Vec<String>,
    #[serde(default)]
    pub not: Vec<String>,
    #[serde(default)]
    pub priority: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenState {
    Working,
    Blocked,
    Idle,
    /// Wins but changes nothing (herdr `skip_state_update`).
    Hold,
}

impl ScreenState {
    pub fn as_str(self) -> &'static str {
        match self {
            ScreenState::Working => "working",
            ScreenState::Blocked => "blocked",
            ScreenState::Idle => "idle",
            ScreenState::Hold => "hold",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Base {
    Screen,
    PromptBox,
    AbovePromptBox,
    AfterLastRule,
    AfterLastPrompt,
    BeforeCurrentPrompt,
    WithoutCurrentPrompt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Region {
    Title,
    /// A slice of the screen, then its last (or, for `top`, first) `lines`
    /// non-empty lines.
    Lines(Base, Option<usize>, bool),
}

#[derive(Debug, Clone)]
pub struct ScreenRule {
    pub id: String,
    pub state: ScreenState,
    pub priority: i32,
    region: Region,
    /// The region as written (`bottom(12)`, `screen(5)`, `title`, …).
    label: String,
    regexes: Vec<Regex>,
    all: Vec<Regex>,
    not: Vec<Regex>,
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
            "hold" => ScreenState::Hold,
            other => return Err(at(format!("bad state '{other}'"))),
        };
        let name = spec.region.as_deref().unwrap_or("bottom");
        let (base, lines) = match name {
            "title" if spec.lines.is_some() => {
                return Err(at("lines does not apply to region title".into()))
            }
            "title" => (None, None),
            "bottom" => (
                Some(Base::Screen),
                Some(spec.lines.unwrap_or(DEFAULT_LINES)),
            ),
            "screen" => (Some(Base::Screen), spec.lines),
            "prompt_box" => (Some(Base::PromptBox), spec.lines),
            "above_prompt_box" => (Some(Base::AbovePromptBox), spec.lines),
            "after_last_rule" => (Some(Base::AfterLastRule), spec.lines),
            "top" => (
                Some(Base::Screen),
                Some(spec.lines.unwrap_or(DEFAULT_LINES)),
            ),
            "after_last_prompt" => (Some(Base::AfterLastPrompt), spec.lines),
            "before_current_prompt" => (Some(Base::BeforeCurrentPrompt), spec.lines),
            "without_current_prompt" => (Some(Base::WithoutCurrentPrompt), spec.lines),
            other => return Err(at(format!("bad region '{other}'"))),
        };
        if lines.is_some_and(|n| !(1..=MAX_LINES).contains(&n)) {
            return Err(at(format!("lines must be 1..={MAX_LINES}")));
        }
        let region = base.map_or(Region::Title, |b| Region::Lines(b, lines, name == "top"));
        if spec.regex.is_empty() && spec.all.is_empty() {
            return Err(at("needs at least one regex or all".into()));
        }
        let compile = |list: &[String]| {
            list.iter()
                .map(|r| Regex::new(r).map_err(|e| at(format!("bad regex: {e}"))))
                .collect::<Result<Vec<_>, _>>()
        };
        Ok(ScreenRule {
            id: spec.id.clone(),
            state,
            priority: spec.priority,
            region,
            label: lines.map_or(name.to_string(), |n| format!("{name}({n})")),
            regexes: compile(&spec.regex)?,
            all: compile(&spec.all)?,
            not: compile(&spec.not)?,
        })
    }

    /// The region as written in the manifest, with its line limit, for
    /// `pane.explain`.
    pub fn region_label(&self) -> &str {
        &self.label
    }

    pub fn matches(&self, title: &str, screen: &str) -> bool {
        let sliced;
        let text = match self.region {
            Region::Title => title,
            Region::Lines(base, limit, from_top) => {
                let raw: Vec<&str> = screen.lines().collect();
                let lines: Vec<&str> = slice(base, &raw)
                    .iter()
                    .map(|l| l.trim_end())
                    .filter(|l| !l.is_empty())
                    .collect();
                let n = limit.unwrap_or(lines.len()).min(lines.len());
                let kept = if from_top {
                    &lines[..n]
                } else {
                    &lines[lines.len() - n..]
                };
                sliced = kept.join("\n");
                &sliced
            }
        };
        (self.regexes.is_empty() || self.regexes.iter().any(|r| r.is_match(text)))
            && self.all.iter().all(|r| r.is_match(text))
            && !self.not.iter().any(|r| r.is_match(text))
    }
}

/// herdr's horizontal rule: starts with `─`, and either is only dashes or
/// has at least three before any trailing text.
fn is_rule(line: &str) -> bool {
    let trimmed = line.trim();
    let dashes = trimmed.chars().take_while(|&c| c == '─').count();
    dashes > 0 && (dashes >= 3 || trimmed.trim_start_matches('─').trim().is_empty())
}

/// The raw screen lines a region reads (herdr semantics: the prompt box
/// opens at the second-last rule and ends at the next one; no box means an
/// empty box and a whole-screen "above").
fn slice<'a>(base: Base, lines: &'a [&'a str]) -> &'a [&'a str] {
    let rules: Vec<usize> = (0..lines.len()).filter(|&i| is_rule(lines[i])).collect();
    let top = rules.len().checked_sub(2).map(|i| rules[i]);
    match base {
        Base::Screen => lines,
        Base::PromptBox => top.map_or(&[], |t| {
            let end = rules
                .iter()
                .copied()
                .find(|&i| i > t)
                .unwrap_or(lines.len());
            &lines[t + 1..end]
        }),
        Base::AbovePromptBox => top.map_or(lines, |t| &lines[..t]),
        Base::AfterLastRule => rules.last().map_or(lines, |&last| &lines[last + 1..]),
        Base::AfterLastPrompt => match lines.iter().rposition(|l| is_prompt(l)) {
            Some(i) => &lines[i + 1..],
            None => lines,
        },
        Base::BeforeCurrentPrompt => current_prompt(lines).map_or(lines, |i| &lines[..i]),
        Base::WithoutCurrentPrompt => match current_prompt(lines) {
            Some(_) => &[],
            None => lines,
        },
    }
}

/// herdr's Codex prompt line: `›` alone or `› ` followed by input.
fn is_prompt(line: &str) -> bool {
    line == "›" || line.starts_with("› ")
}

/// The last prompt line, unless a Codex block line (`•`, `■`, `✗`, `✓`)
/// follows it, which means that prompt was already answered.
fn current_prompt(lines: &[&str]) -> Option<usize> {
    let i = lines.iter().rposition(|l| is_prompt(l))?;
    let block = |l: &&str| l.starts_with(['•', '■', '✗', '✓']);
    (!lines[i + 1..].iter().any(block)).then_some(i)
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
            ..Default::default()
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
            ..Default::default()
        })
        .unwrap_err()
    }

    #[test]
    fn invalid_rules_are_rejected() {
        assert!(compile_err("busy", None, None, &["x"]).contains("state"));
        assert!(compile_err("working", Some("middle"), None, &["x"]).contains("region"));
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

    fn from_toml(text: &str) -> Result<ScreenRule, String> {
        ScreenRule::compile(&toml::from_str::<ScreenRuleSpec>(text).unwrap())
    }

    #[test]
    fn all_and_not_gate_the_any_list() {
        let r = from_toml(
            "id = 'a'\nstate = 'blocked'\nall = ['esc dismiss', 'enter (?:confirm|submit)']\nnot = ['done']",
        )
        .unwrap();
        assert!(r.matches("", "esc dismiss\nenter submit"));
        assert!(!r.matches("", "esc dismiss"));
        assert!(!r.matches("", "esc dismiss\nenter submit\ndone"));
        let r = from_toml("id = 'b'\nstate = 'working'\nregex = ['x', 'y']\nall = ['z']").unwrap();
        assert!(r.matches("", "y z"));
        assert!(!r.matches("", "y"));
        assert!(!r.matches("", "z"));
        assert!(from_toml("id = 'c'\nstate = 'idle'\nnot = ['x']")
            .unwrap_err()
            .contains("regex"));
        assert!(from_toml("id = 'd'\nstate = 'idle'\nall = ['(']")
            .unwrap_err()
            .contains("regex"));
        assert!(
            from_toml("id = 'e'\nstate = 'idle'\nregex = ['x']\nnot = ['(']")
                .unwrap_err()
                .contains("regex")
        );
    }

    const BOXED: &str = "old output\n────────\nDo you want to proceed?\n✻ Waiting for 2 background agents to finish\n────────────\n❯ fix it\n────────────\n  ? for shortcuts\n";

    fn region_hits(region: &str, lines: Option<usize>, re: &str) -> bool {
        let mut text = format!("id = 'r'\nstate = 'idle'\nregion = '{region}'\nregex = ['{re}']");
        if let Some(n) = lines {
            text.push_str(&format!("\nlines = {n}"));
        }
        from_toml(&text).unwrap().matches("", BOXED)
    }

    #[test]
    fn prompt_aware_regions_follow_herdr() {
        // prompt_box: between the second-last rule and the next one.
        assert!(region_hits("prompt_box", None, "^❯ fix it$"));
        assert!(!region_hits("prompt_box", None, "shortcuts|proceed"));
        // above_prompt_box: everything above the box's top border.
        assert!(region_hits("above_prompt_box", None, "old output"));
        assert!(!region_hits("above_prompt_box", None, "fix it"));
        assert!(region_hits(
            "above_prompt_box",
            Some(1),
            "^✻ Waiting for 2 background agents to finish$"
        ));
        assert!(!region_hits("above_prompt_box", Some(1), "proceed"));
        // after_last_rule: below the last horizontal rule only.
        assert!(region_hits("after_last_rule", None, "shortcuts"));
        assert!(!region_hits("after_last_rule", None, "fix it"));
        // screen: everything; lines keeps the last non-empty lines.
        assert!(region_hits("screen", None, "old output"));
        assert!(!region_hits("screen", Some(2), "fix it"));
        assert!(region_hits("screen", Some(3), "fix it"));
    }

    #[test]
    fn regions_without_rules_fall_back_like_herdr() {
        let plain = "no rules here\n❯ hi\n";
        let hit = |region: &str, re: &str| {
            from_toml(&format!(
                "id = 'r'\nstate = 'idle'\nregion = '{region}'\nregex = ['{re}']"
            ))
            .unwrap()
            .matches("", plain)
        };
        assert!(!hit("prompt_box", "hi"));
        assert!(hit("above_prompt_box", "no rules"));
        assert!(hit("after_last_rule", "no rules"));
        // "──x" is not a rule (fewer than three dashes before text).
        assert!(!super::is_rule("──x"));
        assert!(super::is_rule("───── suffix"));
        assert!(super::is_rule("  ──  "));
        assert!(!super::is_rule("x───"));
    }

    fn hits(region: &str, lines: Option<usize>, re: &str, screen: &str) -> bool {
        let mut text = format!("id = 'r'\nstate = 'idle'\nregion = '{region}'\nregex = ['{re}']");
        if let Some(n) = lines {
            text.push_str(&format!("\nlines = {n}"));
        }
        from_toml(&text).unwrap().matches("", screen)
    }

    #[test]
    fn codex_prompt_regions_follow_herdr() {
        // An answered prompt (a block after it) and a current prompt at the end.
        let s = "› first ask\n• answer\nhint above\n› \n";
        assert!(hits("after_last_prompt", None, "^$|^", s));
        assert!(!hits("after_last_prompt", None, "hint|answer", s));
        assert!(hits("before_current_prompt", None, "hint above", s));
        assert!(!hits("before_current_prompt", None, "^›[ ]?$", s));
        assert!(!hits("without_current_prompt", None, "answer", s));
        // A block after the last prompt: no current prompt.
        let s = "› ask\n• Working (3s • esc to interrupt)\n";
        assert!(hits("after_last_prompt", None, "Working", s));
        assert!(hits("before_current_prompt", None, "› ask", s));
        assert!(hits("without_current_prompt", None, "Working", s));
        // No prompt at all: whole screen everywhere.
        let s = "Allow command? [y/n]\n";
        for region in [
            "after_last_prompt",
            "before_current_prompt",
            "without_current_prompt",
        ] {
            assert!(hits(region, None, "Allow command", s), "{region}");
        }
        // "›x" is not a prompt line; "›" alone is.
        assert!(super::is_prompt("›"));
        assert!(super::is_prompt("› fix it"));
        assert!(!super::is_prompt("›x"));
        assert!(!super::is_prompt(" › fix"));
    }

    #[test]
    fn top_region_keeps_the_first_lines() {
        let s = "\nfirst\nsecond\nthird\n";
        assert!(hits("top", Some(2), "second", s));
        assert!(!hits("top", Some(2), "third", s));
        assert!(hits("top", Some(1), r"\Afirst\z", s));
        let r = from_toml("id = 't'\nstate = 'idle'\nregion = 'top'\nregex = ['x']").unwrap();
        assert_eq!(r.region_label(), "top(12)");
        let r =
            from_toml("id = 'p'\nstate = 'idle'\nregion = 'before_current_prompt'\nregex = ['x']")
                .unwrap();
        assert_eq!(r.region_label(), "before_current_prompt");
    }

    #[test]
    fn hold_state_and_region_labels() {
        let r =
            from_toml("id = 'h'\nstate = 'hold'\nregion = 'prompt_box'\nregex = ['x']").unwrap();
        assert_eq!((r.state, r.state.as_str()), (ScreenState::Hold, "hold"));
        assert_eq!(r.region_label(), "prompt_box");
        let r = from_toml(
            "id = 'a'\nstate = 'idle'\nregion = 'above_prompt_box'\nlines = 1\nregex = ['x']",
        )
        .unwrap();
        assert_eq!(r.region_label(), "above_prompt_box(1)");
        let r = from_toml("id = 'b'\nstate = 'idle'\nregex = ['x']").unwrap();
        assert_eq!(r.region_label(), "bottom(12)");
        // Labels keep the region name written in the manifest.
        let r = from_toml("id = 's'\nstate = 'idle'\nregion = 'screen'\nlines = 12\nregex = ['x']")
            .unwrap();
        assert_eq!(r.region_label(), "screen(12)");
        let r = from_toml("id = 't'\nstate = 'idle'\nregion = 'screen'\nregex = ['x']").unwrap();
        assert_eq!(r.region_label(), "screen");
        let r = from_toml("id = 'u'\nstate = 'idle'\nregion = 'title'\nregex = ['x']").unwrap();
        assert_eq!(r.region_label(), "title");
        assert!(
            from_toml("id = 'c'\nstate = 'idle'\nregion = 'screen'\nlines = 0\nregex = ['x']")
                .is_err()
        );
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
