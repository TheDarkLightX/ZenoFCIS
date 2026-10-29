# Reading ZAL with Libertine

The offline ontology and library Lens browsers use Linux Libertine for selected
human-facing prose and headings. The controlled-English ontology source also
uses this reading face. Exact JSON, identities, formulas and source artifacts
retain their own views; choosing a typeface does not translate or change them.
The live behavioral authoring workspace is a separate interface.

The choice follows the typography in
[Endangered by the Language but Saved by the Compiler](https://doi.org/10.1145/3776682).
The published PDF embeds Libertine, Biolinum, Inconsolata and several mathematical
fonts. This change uses the original Linux Libertine O regular font, version
5.3.0, for prose. It does not reproduce the paper's complete mathematical
typesetting system.

The [font provenance](reading_assets/SOURCE.json) identifies the unmodified
505,280-byte OpenType file and its SHA-256. Its
[Open Font License](reading_assets/LICENSE.txt) accompanies the source and each
embedded CSS copy. See the [font project's website](https://linuxlibertine.org/).
No font installation, package manager or external font request is needed.

Export embeds this one font as a data URL, adding about 662 KiB plus the license
to the ontology HTML and Lens stylesheet. Existing artifact size limits remain
in force. Lens producer identity includes the font, license and provenance;
packet replay reconstructs the resulting presentation bytes. The ontology
browser permits data fonts under its content-security policy and still forbids
network connections.

`ONTOLOGY_QUALIFICATION.json` records the pre-typography PR168 source snapshot.
It is historical evidence for that subject. Typography qualification separately
checks the changed exports, real font use, reading layouts, navigation and exact
retained data. Neither a font check nor packet replay proves authored meaning,
behavioral refinement, external delivery or completion of the 2.7 release.
