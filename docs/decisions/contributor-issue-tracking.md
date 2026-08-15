# Contributor Issue Tracking

Decision: GitHub issues are the project's issue tracker for contributors. The project's agent
instructions do not require any other tracking tool, and no tracker beyond git and GitHub is a
prerequisite for opening a pull request.

Rationale: Issues need to be visible to anyone who clones the repository and usable without
installing anything, and they need to sit where pull requests can be gated and approved.
Maintainers are free to run additional tooling locally — `.beads/` is committed because the
project's git hooks live there — but a contributor must never need it.
