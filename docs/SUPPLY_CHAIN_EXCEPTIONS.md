# Supply-chain exceptions

Every entry in this file is narrow, version-specific, and release-blocking when its stated conditions stop being true. Exceptions do not authorize production deployment.

## RUSTSEC-2026-0173: `proc-macro-error2` is unmaintained

Status: accepted informational exception for ZenoFCIS 1.0.0-rc.3.

Exact dependency path:

```text
proc-macro-error2 2.0.1
-> hax-lib-macros 0.3.7
-> hax-lib 0.3.7
-> libcrux-secrets 0.0.6
-> libcrux-traits 0.0.8
-> libcrux-sha2 0.0.8
-> zeno-fcis-crypto 1.0.0-rc.3
```

Disposition:

- the advisory is an unmaintained notice with no CVSS score, vulnerability, patched version, or unaffected version;
- `libcrux-sha2 = 0.0.8` is the latest published release as of 2026-07-25;
- the dependency is supplied by the optional independently checked libcrux SHA-256 provider;
- ZenoFCIS never exposes or invokes `proc-macro-error2` as protocol behavior;
- fixed-vector and cross-provider SHA-256 parity tests remain mandatory;
- `cargo audit` ignores exactly `RUSTSEC-2026-0173` and continues to deny every other warning.

Removal trigger: a reviewed libcrux SHA-2 release removes the dependency or the advisory gains a security impact. The exception must then be removed or reconsidered before release.

Review owner: ZenoFCIS release maintainer.

Next review: before every release candidate and no later than 2026-10-25.
