# Issue tracker: GitHub

Issues and specs for this repo live as GitHub issues. Use the `gh` CLI for all operations.

## Conventions

- **Create an issue**: `gh issue create --title "..." --body "..."`. Use a heredoc for multi-line bodies.
- **Read an issue**: `gh issue view <number> --comments`, filtering comments by `jq` and also fetching labels.
- **List issues**: `gh issue list --state open --json number,title,body,labels,comments --jq '[.[] | {number, title, body, labels: [.labels[].name], comments: [.comments[].body]}]'` with appropriate `--label` and `--state` filters.
- **Comment on an issue**: `gh issue comment <number> --body "..."`
- **Apply / remove labels**: `gh issue edit <number> --add-label "..."` / `--remove-label "..."`
- **Close**: `gh issue close <number> --comment "..."`

Infer the repo from `git remote -v`; `gh` does this automatically when run inside a clone.

## Pull requests as a triage surface

**PRs as a request surface: no.** `/triage` reads this flag: external pull requests are not
triaged as feature requests.

GitHub shares one number space across issues and PRs, so a bare `#42` may be either: resolve with `gh pr view 42` and fall back to `gh issue view 42`.

## Labels

Issues and pull requests are labelled by kind: `bug`, `enhancement`, `documentation`,
`refactor`, and `good first issue` for small, self-contained ones. The repository also has
`accessibility`, `duplicate`, `help wanted`, `invalid`, `question` and `wontfix`.

Of the triage labels the skills name, only `wontfix` exists. There are no `needs-triage`,
`needs-info`, `ready-for-agent` or `ready-for-human` labels, and none is to be created: where a
skill would apply one, say in a comment on the issue what it needs instead.

## When a skill says "publish to the issue tracker"

Create a GitHub issue.

## When a skill says "fetch the relevant ticket"

Run `gh issue view <number> --comments`.

## Security problems

Never describe a security problem, as [SECURITY.md](../../SECURITY.md) defines it, in a public
issue, comment or PR description. Report it privately as SECURITY.md describes, and keep public
commit messages and CHANGELOG entries neutral.
