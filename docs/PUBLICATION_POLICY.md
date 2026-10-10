# Public documentation policy

Publish documentation that helps users install, use, understand, or assess the
released library: tutorials, API and format references, architecture, explicit
assurance boundaries, supported examples, and release records.

Keep unpublished material out of the public repository. Local availability is
not publication permission. Do not copy unpublished links, results, or hashes
into a public page.

Before publishing a document:

1. State its purpose for library users and read the entire document.
2. Confirm its sources, examples, links, and attachments are public.
3. Review each claim against the implemented feature and its qualified scope.
4. Add the approved document to `release/public-documents.txt` and run
   `python3 tools/check_public_docs.py` plus the documentation link check.
5. Use the bounded release privacy scanner with a local private-marker file
   when project-specific exclusions are needed. Never publish that marker file.

A new publication needs explicit approval and its own review. Contributing to
an implementation does not by itself qualify a document for publication.
Release bundles, attached reports, CI artifacts, and pull request descriptions
need the same review as repository files. Do not change an archive signature
after publication.

The checks detect known omissions and unapproved paths. They do not establish
that every confidential detail has been identified; content review remains
necessary. Keep private copies outside publishing checkouts.
