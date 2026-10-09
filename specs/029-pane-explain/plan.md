# Plan

Expose `ScreenRule::matches` and a region label from `signaltty-agent`.
`Ctx::screen_rules` returns its source with the rules so the tick and the
explain handler share one selection. The handler reads the store and the
headless screen without writing either. Test-first: unit test for the
region label, integration tests for the four acceptance cases, then the CLI.
A Claude review agent reviews the diff; no Gemini.
