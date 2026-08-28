# Contributing to INIMOA

Thank you for helping build INIMOA. Contributions are licensed under the [GNU Affero General Public License v3.0](LICENSE.txt).

## Before coding

1. Open or confirm an issue describing the problem and expected behavior.
2. Identify whether the change belongs to Macro upstream, INIMOA, or both.
3. Keep the change within the existing domain/port/adapter architecture and Macro-native UI patterns.
4. Do not include private planning documents, local AI-agent configuration, credentials, or generated build output.

## Pull requests

Use a concise Conventional Commit-style title:

```text
feat(projects): enforce dependency readiness
fix(teams): prevent role restoration after rejoin
```

The pull request should explain what changed, why it belongs in INIMOA, how it was verified, and what remains unverified. Link the issue and include migration or rollback notes when persistence changes.

## Development checks

- Follow [docs/STYLE_GUIDE.md](docs/STYLE_GUIDE.md).
- Use [docs/RUNNING_LOCALLY.md](docs/RUNNING_LOCALLY.md) for setup.
- Format and lint the affected Rust or TypeScript code.
- Run focused tests for every changed crate or package.
- Refresh checked-in SQLx metadata when static queries change.
- Verify permission-denied, failure, and rollback behavior for sensitive operations.

Do not claim completion from a build alone. Report the exact test and runtime boundary that was verified.

## AI-assisted work

AI tools are allowed, but contributors remain responsible for every submitted line. Do not commit assistant instructions, session transcripts, prompts, private development plans, or output that has not been reviewed and tested.

## License

By submitting a contribution, you agree that it may be distributed under the [AGPL-3.0](LICENSE.txt) with the rest of the project.
