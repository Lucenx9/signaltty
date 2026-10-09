# Verify approval reconciliation

```sh
GTK_A11Y=none xvfb-run -a dbus-run-session -- cargo test -p signaltty-gui terminal::tests::approval_question_and_choices_fit_a_narrow_pane -- --exact --ignored --test-threads=1
scripts/verify.sh full
```

The native regression must remove choices when the same ID becomes read-only, restore current options, emit their current IDs, and retain the existing VTE.
