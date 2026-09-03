# INIMAOS baseline and delivery boundary

## Status

- Recorded: 2026-09-03, Asia/Seoul.
- Canonical source: `Inima-solution/INIMOA` commit
  `b325df522ce63d9b1dc48b3f0590d7a9edb63aaf`.
- Macro comparison baseline: tag-derived description
  `v2026.8.26.0-1-g8ee95d53b`, commit
  `8ee95d53bd8dc777141b8a37473699ef20b704d9`.
- Evidence state: local source and runtime evidence exists; staging,
  production, release, and disaster-recovery evidence does not.
- Delivery owner, reviewer, and rollback owner: `Ahn-Hyun`.

This file records facts and required controls. A requirement is not described
as active until the named enforcement or runtime evidence exists.

## Repository identity

| Field | Recorded value | Evidence boundary |
| --- | --- | --- |
| Company fork | `https://github.com/Inima-solution/INIMOA.git` as `origin` | Company-owned public repository |
| Upstream | `https://github.com/macro-inc/macro.git` as fetch-only `upstream` | Upstream push is disabled locally |
| Canonical commit | `b325df522ce63d9b1dc48b3f0590d7a9edb63aaf` | Merge commit for PR #1 and current `origin/main` at recording time |
| Macro comparison commit | `8ee95d53bd8dc777141b8a37473699ef20b704d9` | Pinned historical comparison point, not current upstream HEAD |
| Current fetched upstream | `bc07a05616dfcc92a55fc26f19f749cc11809152` | `upstream/main` at recording time; no integration rehearsal is claimed |
| License | `LICENSE.txt`, GNU AGPL version 3 | Legal approval for an operated release remains separate |
| Feature branch | `gate-foundation-closure` from the canonical commit | Dedicated local branch; it is not a deployed environment |
| Preserved upstream worktree | Clean at the pinned comparison commit | It is 25 commits behind the recorded fetched upstream and has not rehearsed an update |

The private implementation plan and design contract remain local development
artifacts and are excluded by `.gitignore`. Their removal from the public tree
does not erase their Git history: commit `7e4553147` records the original
baseline and design contract, while this document is the current public-safe
delivery record.

## GitHub control state

Read-only GitHub inspection on 2026-09-03 established:

- `main` is not branch-protected and no repository ruleset exists;
- repository Actions are enabled with default read-only workflow permissions;
- PR #1 has no review or check-run evidence;
- PR #2 opened from `gate-foundation-closure`, but the upstream-generated
  conventions workflow could not provide a fork gate: its jobs target the
  unavailable `namespace-profile-linux-small` custom runner;
- the upstream conventions workflow is now disabled manually and remains
  unsuitable until a compatible runner is explicitly approved;
- no release, deployment, or GitHub environment exists for the canonical
  commit;
- merge commits, squash merges, and rebase merges are all enabled; and
- merged branches are not automatically deleted.

Therefore no required-review, required-check, deployment, or release claim is
currently valid.

This branch adds `.github/workflows/inimoa-governance-check.yml` as the only
fork-native gate. It is not generated or upstream-owned. It uses a
GitHub-hosted `ubuntu-latest` runner, declares only `contents: read`, receives
no secrets, has no environment, cache, publication, deployment, repository
mutation, browser, schedule, release, or manual-dispatch path, and has a bounded
timeout. It checks pull requests to `main` and pushes to non-`main` branches so
PR #2 can produce evidence before merge.

The enforcement sequence is:

1. Keep `.github/workflows/code_check_conventions.yml` disabled until a
   compatible custom runner is approved. Keep every other checked-in upstream
   workflow disabled until it is individually reviewed and approved for this
   fork.
2. Commit and push the fork-native governance workflow. Its expected check
   display is `INIMOA Governance Check / Governance Contract`; treat that as an
   expectation until GitHub records the first successful run.
3. Verify the branch-push and PR checks both pass on the exact commit. Do not
   manually dispatch any workflow.
4. Protect `main` with pull requests required, zero required approvals,
   conversation resolution, administrator enforcement, and force-push and
   deletion blocks.
5. Keep Code Owners documentary and do not require Code Owner approval while
   the repository has only one collaborator.
6. Leave required status checks unset until the successful fork-native run
   exposes its stable context name; copy the observed context exactly rather
   than inferring it from this document.
7. Keep the current merge methods unchanged until the owner separately chooses
   a merge strategy.

## Branch, commit, and ownership rules

- Changes target `main` through a dedicated pull-request branch.
- New branch names must not start with `codex/`.
- New commit subjects and bodies are written in English.
- `Ahn-Hyun` owns GitHub administration, review, release approval, and rollback
  until an explicit reassignment is recorded.
- A single-owner repository must not require its owner to self-approve.
- A green narrow check proves only its named boundary.

## Upstream evaluation policy

Evaluate upstream without modifying the deployed fork directly:

1. Fetch `upstream` and record its exact commit and reachable release tag.
2. Create a dedicated integration branch/worktree from the intended INIMAOS
   base; never update a deployed branch in place.
3. Review the upstream range for migrations, generated contracts, dependency
   and license changes, security fixes, and INIMAOS conflicts.
4. Reapply or merge into the integration branch and run the affected Rust,
   frontend, migration, permission, and authenticated journey checks.
5. Run the documented full release gate before promotion.
6. Promote one verified commit/artifact through environments; record both the
   Git commit and immutable artifact digest.
7. Merge only through the repository's enforced pull-request path.

This is the recorded process definition. Owner approval and the first real
upstream-update rehearsal remain open.

## Environment and secret boundaries

- Local development uses named, isolated stacks documented in
  `docs/RUNNING_LOCALLY.md`.
- Continue the existing Doppler approach for real integration secrets. Local
  `--no-doppler` values are deterministic boot stubs, not staging secrets.
- Development, staging, and production require separate database credentials,
  object-storage buckets or prefixes, service namespaces, identity callbacks,
  and encryption keys.
- Staging must contain neither demo nor production data.
- No staging environment or isolation proof currently exists, so the staging
  and production gates remain open.

## Deterministic local seed

Commit `73c863882` adds the INIMAOS persona scenario for member, manager,
approver, HR admin, payroll admin, organization admin, auditor, and agent.
Recorded local evidence verifies seven human/FusionAuth identities, one
team-owned agent bot, and all seven business-role assignments. This is local
seed evidence only and does not prove staging readiness.

## Feature-flag policy

Every new rollout flag must record:

- a stable product-scoped name;
- owner `Ahn-Hyun` or an explicitly named delegate;
- creation date and linked change;
- default and fail-closed behavior;
- environments and target cohort;
- expiry or review date;
- removal issue and disable procedure; and
- evidence that both enabled and disabled paths were tested.

Flags are temporary rollout controls, never permanent alternate
implementations. Expired flags block release until removed or explicitly
renewed with a new date and reason.

## Release evidence record

Each candidate release must record:

- source commit, upstream base/tag, branch, pull request, and reviewer;
- exact checks with timestamps, exit codes, and retained logs;
- migration set, compatibility decision, backup identifier, restore rehearsal,
  and rollback steps;
- seeded persona and authenticated browser/E2E evidence;
- artifact/image digest and target environment;
- health, routing, and primary-journey verification after deployment;
- feature flags with owner, expiry, disable, and removal details;
- monitoring links and rollback triggers; and
- approval and release time.

The release and rollback owner is `Ahn-Hyun`. This template is defined, but no
INIMAOS release record, staging promotion, backup/restore rehearsal, or rollback
execution exists yet.

## WS-00 evidence summary

| Requirement | State at recording time |
| --- | --- |
| Current remotes, fork, baseline, commit, and license | Recorded |
| Dedicated feature branch and preserved upstream worktree | Recorded; upstream rehearsal still open |
| Branch protection, reviews, checks, and merge strategy | Policy partially defined; external enforcement and merge choice open |
| Upstream evaluation process | Defined; execution open |
| Isolated development, staging, and production resources | Local only; staging and production open |
| Doppler-backed staging secrets | Approach approved; staging proof open |
| Deterministic personas | Implemented and locally verified |
| Baseline screenshots and full checks | Partial local evidence; exact baseline gate open |
| Backup and restore rehearsal | Open |
| Feature-flag rules | Defined; first release application open |
| Release template and rollback owner | Defined; execution open |

WS-00 and Gate A remain open until the missing external and runtime evidence is
recorded. This document does not convert configuration or policy into proof of
an environment that has not been provisioned and tested.
