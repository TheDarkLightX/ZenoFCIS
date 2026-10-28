# Finite ZAL provenance fixtures

`app/` is the bounded two-state repair-request model from
`integrations/zal/test_app_profile.py`, lowered through the explicit app adapter.
Its original source and literal UTF-8 binding retain CRLF line endings. The
48-row reference table and generated correspondence source are retained data;
the native scaffold must place them under `provenance/zal/`, outside public
assets and automatic Cargo test discovery.

`legacy-1/` and `legacy-2/` come from the shipped `order.zal` and
`context_free2.zal` examples through the ordinary factory lowerer. They have
source maps and reference artifacts without an app binding. Their successful
native generation must remain unchanged.

The fixtures establish byte/metadata packaging cases. Coherent source-map
hashes and matching revision strings are not semantic proofs or authentication.
Pure lowering replay and actual native generation/refusal tests are separate.
