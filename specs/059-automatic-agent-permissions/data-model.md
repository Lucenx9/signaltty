# Data model

GuiPreference adds `automatic_agent_permissions: bool`, default false. JSON parsing accepts a boolean only. Existing preference fields are retained.
Pane.argv and agent.resume_argv already persist native launch/resume arguments. No new pane or task field, transition or event is introduced. Automatic mode is conveyed by an allowlisted native argument prefix. The parent pane's recorded launch arguments are the source for default same-provider workers.
