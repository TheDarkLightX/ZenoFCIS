# Does swapping two users' swaps change either received amount?

This synthesized constant-product swap records reserve changes and each user's received amount separately. Exact commutation and tolerance one are both refuted over the declared domain. Capacity refusals and integer floor rounding remain observable.

See [two-run properties](../../../docs/TWO_RUN_PROPERTIES.md) for the exact
command, checked output, mathematical model and limits. Inputs include the
complete command and context. Results are advisory and grant no runtime or
publication authority.

## Lineage

**Background.** Goguen and Meseguer, *Security Policies and Security Models*,
IEEE Symposium on Security and Privacy, 1982; Barthe, D'Argenio and Rezk,
*Secure Information Flow by Self-Composition*, IEEE Computer Security
Foundations Workshop, 2004; Clarkson and Schneider, *Hyperproperties*,
Journal of Computer Security 18(6), 2010. These supply the noninterference,
self-composition and hyperproperty background for the checker.

**What is new here.** A finite executable example of those established methods;
comparative prior-art search pending. No novelty claim is made.

**Scope.** Exhaustively checked over the declared domains only, using one step
per side for noninterference or two for order independence. Longer
interleavings are not claimed.
