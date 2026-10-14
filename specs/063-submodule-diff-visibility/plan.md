# Implementation plan: Submodule diff visibility

**Branch**: `t3/repo-bug-fixes-3` | **Date**: 2026-10-14 | **Spec**: [spec.md](spec.md)

## Summary and technical context

Rust 2021 server, existing Git subprocesses, JSONL IPC and isolated TestServer
fixtures. Pin `--ignore-submodules=untracked` in each explicit numstat/patch comparison:
HEAD workspace, recorded-base task, turn trees and both selected-file paths.
No new dependency or abstraction. Retain short gitlink patches and existing limits.

## Constitution check

Spec clarified before implementation. Reproduce first using the existing real-IPC
seam and local repositories; retain that fixture as the regression. Core/GUI/state
remain untouched. No load-bearing architecture decision changes: this restores
existing explicit comparison contracts from ADR-0016 and ADR-0029. Full proof and
independent Standards/Spec reviews precede publishing. No exception needed.

## Design and ownership

- git.rs owns HEAD workspace and task numstat calls.
- turns.rs owns tree-to-tree numstat comparisons and private snapshot capture.
  After ordinary add, enumerate stage records with mode 160000 and force-add only
  those literal NUL-delimited paths in the private index. This compensates for
  Git 2.55 skipping ignore=all links even during add --all; ignored ordinary
  files remain excluded. Existing TempIndex cleanup also removes the path file.
- file_diff.rs owns HEAD/base and tree-to-tree patches.
- tests/file_diff.rs exercises existing IPC and Git boundaries with local submodules.
- docs/08-ipc.md clarifies that Git ignore settings cannot hide gitlink changes.

## Verification strategy

One local submodule advances a commit after a baseline. Assert exact removed/added
Subproject commit lines, summary membership, HEAD/turn/task agreement and stable
user index/config. Vary diff.ignoreSubmodules, local submodule ignore and .gitmodules
ignore independently. Stage final source, run full and assess delivery proof.
