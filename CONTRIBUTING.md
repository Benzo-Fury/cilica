# Contributing to Cilica

Thanks for helping. Bug reports, ideas and pull requests are all welcome.

## Before you open a pull request

1. Read [CLAUDE.md](CLAUDE.md) for commands and conventions, and [docs/spec.md](docs/spec.md) for how the app should behave.
2. Run the checks: `bun run typecheck`, `bun run --cwd apps/desktop lint`, `bun run --cwd apps/desktop test`, and `cargo test` for any crate you changed.
3. Keep each pull request to one change, and say in its description what it does and why.

## Contributor licence agreement

Before your first pull request can be merged, you need to sign the [Cilica Contributor License Agreement](licenses/CLA.md). The CLA Assistant bot will post a link on the pull request: open it, sign in with GitHub and accept. You only do this once.

**Why there is one.** Cilica is open source under the AGPL, and it is also sold under paid licences to larger organisations. That pays for its development. The agreement lets the maintainer include your work in both. In return, it promises that any version including your contribution will also stay available under its open source licence. You keep the copyright in your work.

If you contribute as part of your job, check with your employer first. They may need to sign for you, as section 7 of the agreement explains.

## Third-party code

If your change includes code you didn't write, say where it came from and what licence it uses. Only include code under permissive licences such as MIT, BSD, Apache 2.0, ISC or Zlib. Code under the GPL, AGPL or LGPL can't be included, because Cilica is also offered under commercial licences.
