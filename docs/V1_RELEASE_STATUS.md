# V1 release status

Assessment date: 2026-09-12. The source reviewed here is
`dfdcc04c5f6d944f9c4395b6ec119ee3a5765e49`, together with the accompanying
[parallel authorization change](PARALLEL_AUTHORIZATION_SORTING.md).
The release identity remains `1.0.0-rc.3`. This is a dated assessment, not
release approval or evidence that a later commit passed.

The shortest remaining path is a bounded qualification pass using the
[V1 release checklist](V1_RELEASE_CHECKLIST.md). Further optimization is
optional unless it fixes a release blocker.

## Confirmed status

| Area | Current evidence | Required before stable V1 |
| --- | --- | --- |
| Integration | The work is on `agent/bounded-synthesis-20260908`; no open PR was found. Remote `main` is `0123f8ffe0f29caeecc1bf6417bbf37e485bbf88` and has commits absent from this branch. | Review the complete branch against current main and validate the final integrated source. |
| CI | The recorded push of `dfdcc04` has 43 successful workflows and Miri still running; no failed conclusion was observed. | Every required workflow must pass at the final source commit. |
| Formal tools and QEMU | These workflows were absent from that push's 44-run set. They run on main pushes, matching PR changes, or explicit dispatch. | Retain exact-source results, including the existing pinned Lean translation corpus and QEMU transcript/framebuffer checks. |
| Assurance issues | 13 issues remain open, including older P0/P1 findings. Their reports name older revisions; several corresponding repairs are present in current code. | Reconcile each acceptance criterion with current source and retained tests; fix actual residual defects and record reviewed closure or justified scope. |
| API and adoption | CLI discovery/JSON, generated applications and packaged-consumer checks exist. | Independent final API/authority review and real adopter feedback confirming no required breaking change remains. |
| Packages | The manifest declares 36 public crates and two binaries; a release-candidate workflow passed on `dfdcc04`. | Independently inspect its artifacts and reproduce retained checksums on a second clean builder for the final candidate. Workflow success alone does not establish this. |
| Stable identity | Workspace and package-set versions remain RC3. | Update all internal version pins, release notes and generated package evidence together for the reviewed 1.0 candidate; then repeat the gate. |
| Publication | No tag, merge or publication was performed in this pass. | Complete the owner-controlled signed-tag, ordered crate publication, artifact verification and installed-consumer checks. |

## Issue reconciliation

The open reports are [#54](https://github.com/TheDarkLightX/ZenoFCIS/issues/54),
[#55](https://github.com/TheDarkLightX/ZenoFCIS/issues/55),
[#56](https://github.com/TheDarkLightX/ZenoFCIS/issues/56),
[#57](https://github.com/TheDarkLightX/ZenoFCIS/issues/57),
[#58](https://github.com/TheDarkLightX/ZenoFCIS/issues/58),
[#61](https://github.com/TheDarkLightX/ZenoFCIS/issues/61),
[#62](https://github.com/TheDarkLightX/ZenoFCIS/issues/62),
[#67](https://github.com/TheDarkLightX/ZenoFCIS/issues/67),
[#68](https://github.com/TheDarkLightX/ZenoFCIS/issues/68),
[#72](https://github.com/TheDarkLightX/ZenoFCIS/issues/72),
[#73](https://github.com/TheDarkLightX/ZenoFCIS/issues/73),
[#74](https://github.com/TheDarkLightX/ZenoFCIS/issues/74), and
[#76](https://github.com/TheDarkLightX/ZenoFCIS/issues/76).
This is an issue inventory, not a claim of 13 remaining vulnerabilities.

For example, the source already has privately constructed genesis and
transition authority, complete SQLite bundle/receipt/replay/outbox checking,
complete-footprint evidence, validated refinement records, catalog-derived
economic law requirements, and explicit non-executable commit evidence.
Each issue still needs its acceptance criteria checked; the existence of a
type or a passing test count alone cannot close an assurance finding.

Issue #68 asks for a mechanized end-to-end soundness theorem. It is labeled an
enhancement and must be distinguished from an observed runtime defect. Whether
that theorem is required depends on the selected release claims. A stable API
version does not establish an unbounded correctness theorem or qualify a
downstream financial deployment.

## Qualification order

1. Review the complete branch against current main, reconcile the 13 issue
   reports, and finish API/adopter feedback. Keep the reviewed V1 feature scope
   fixed unless a blocker requires a change.
2. Finish Miri and run all missing exact-source workflows. Compare the Lean
   translation corpus with the operator claims in the formal-tool reference;
   its older prose and current test coverage need reconciliation. Lean remains
   pinned at 4.30.0; use existing local runtimes without copying or upgrading.
3. Build the final package set from clean source, inspect the generated-application
   receipt, SBOM and provenance inputs, and compare retained hashes from two
   independent clean builders. Record the independent review.
4. Prepare the stable version and compatibility documentation, integrate the
   reviewed source, and rerun the complete release checklist at that exact
   commit. Internal version-pin updates do not require a Lean upgrade.
5. Complete the signed release and ordered publication ceremony, then verify
   installed crates, CLI behavior and published documentation independently.

The full prospective PR changes formal-tools, CLI and spec files, so it can
trigger both conditional workflows. Looking only at the latest three-line
patch would give the wrong conclusion about those path filters. RC feedback
can be gathered from reviewed source and package candidates; absence of a
public tag does not by itself establish absence of feedback.

The main agent verified the branch and live issue/run inventory, and corrected
these points in Opus's advisory review. No irreversible release action is
authorized by this report.
