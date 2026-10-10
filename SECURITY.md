# Security policy

ZenoFCIS is pre-release software. It includes a nominal catalog, law,
invocation, provider, interpreter, deployment, and replay-bound commit-authority
mechanism. That library mechanism does not qualify any downstream deployment
or establish a value-custody, audit-completion, or general production claim.

Please report suspected vulnerabilities privately through GitHub's security
advisory interface. Do not include secrets, private keys, or live exploit data
in a public issue.

The semantic kernel forbids unsafe Rust and is designed to remain independent
of clocks, randomness, networking, filesystems, databases, and ambient process
state. A violation of those boundaries is security relevant even when no
memory-safety defect exists.
