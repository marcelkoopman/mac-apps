# Contributing

This is a private repository. Open contributions and public GitHub issues are
not accepted. For support, email **mac-apps-support@proton.me**.

## Commit messages

Commit subjects follow [Conventional Commits](https://www.conventionalcommits.org/):

    type(optional-scope)!: description

`type` is one of feat, fix, refactor, docs, test, ci, chore, build, perf, style or revert. The
scope is lower case (`a-z0-9._-`); `!` marks a breaking change. CI checks every non-merge commit
with `scripts/check_commits.sh BASE HEAD`.

Examples:

    feat(copycraft): show complexity per function
    fix(history): restore the cursor in release builds
    ci: enforce Conventional Commits
