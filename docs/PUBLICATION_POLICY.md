# Public documentation policy

Publish documentation that helps users install, use, understand or assess the
released library: tutorials, API and format references, architecture, explicit
assurance boundaries, supported examples and curated release records.

Keep private project details, Library and Space records, research drafts,
working papers, internal roadmaps, agent handoffs, coordination notes and raw
research logs outside the public repository. Local availability is not
publication permission. Do not copy packet names, private links, unpublished
results or their hashes into a public status page.

Before publishing a document:

1. State its purpose for library users and inspect the entire document.
2. Check that its sources, examples, links and attachments are public.
3. Review claims against the implemented feature and its qualified scope.
4. Add the approved document to `release/public-documents.txt` and run
   `python3 tools/check_public_docs.py` plus the documentation link check.
5. Use the bounded release privacy scanner with a local private-marker file
   when project-specific exclusions are needed. Never publish that marker file.

A new research publication needs explicit owner approval and separate curation.
It is not automatically included because it helped with implementation.
Release source bundles, attached reports, CI artifacts and PR descriptions need
the same review as repository files. Archive signatures must not be silently
changed after withdrawing an asset.

The checks detect known omissions and unapproved paths. They do not establish
that every confidential detail has been identified; content review remains
necessary. Keep private recovery copies outside publishing checkouts, and
re-clone after a privacy history rewrite to avoid restoring old documents.
