# Progress contract

OSC 9;4;<state>[;<value>] terminates with BEL or ST. State is one decimal digit
0..4; value is absent/empty or -1/0..100 in decimal. Retain the payload after `9;`
verbatim for declarative rules (e.g. `4;1;-1`, `4;0;0`, `4;1;`). Invalid reserved
progress is ignored. Ordinary OSC 9 message handling is unchanged.
`region = "osc_progress"` accepts regex/all/not, rejects `lines`, and reads only
retained progress; absent evidence cannot match.
`pane.explain` adds `osc_progress` (string, empty if absent), and evaluates the
same evidence as classification. This is diagnostic evidence, not a lifecycle authority.
