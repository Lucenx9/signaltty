# Validate CI and PR handling

Run actionlint1.7.12 from the checkout. The known Ubuntu26.04 label is recognized;
a fixture with a misspelled event or github context property must fail.

```sh
actionlint
scripts/agent.sh context --task workflow --feature 062-ci-pr-workflow --json
scripts/agent.sh check --phase implementation --feature 062-ci-pr-workflow
scripts/verify.sh full
scripts/agent.sh check --phase delivery --feature 062-ci-pr-workflow --evidence target/verification/full-RUN
```

After opening the PR, inspect its CI event and latest revision. A contribution
branch push does not create automatic CI; PR opened/synchronize/reopened does.
Manual dispatch can be used after this workflow has reached main. See docs19
for failure recovery, comments, authorization and revision-bound merging.
