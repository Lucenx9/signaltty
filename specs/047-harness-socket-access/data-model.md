# Data model

No persisted workspace changes. Existing integration Report adds bridge file and
configuration result. Bridge context is an immutable explicit socket path and pane
id from its process environment; absent context produces tool errors. Registrations
contain command/args and provider-specific environment forwarding. No pane ids or
socket paths are persisted in provider configuration.
