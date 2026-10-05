#!/usr/bin/env python3
"""Render this packet's Markdown manuscript to self-contained LaTeX.

Document formatting only. No runtime implementation, external converter,
network, compiler, model call or proof execution is performed.
"""
from pathlib import Path
import hashlib
import re

BASE = Path(__file__).resolve().parent
SOURCE = BASE / "paper.md"
TARGET = BASE / "paper.tex"


def escape(text):
    substitutions = {
        "\\": r"\textbackslash{}", "&": r"\&", "%": r"\%", "$": r"\$",
        "#": r"\#", "_": r"\_", "{": r"\{", "}": r"\}",
        "~": r"\textasciitilde{}", "^": r"\textasciicircum{}",
        "—": "---", "–": "--", "−": r"\ensuremath{-}",
        "→": r"\ensuremath{\rightarrow}", "≤": r"\ensuremath{\leq}",
        "≥": r"\ensuremath{\geq}", "×": r"\ensuremath{\times}",
        "‘": "`", "’": "'", "“": "``", "”": "''",
    }
    return "".join(substitutions.get(c, c) for c in text)


TOKEN = re.compile(r"\[[^\]]+\]\([^)]+\)|`[^`]+`|\*\*[^*]+\*\*|\*[^*]+\*")


def inline(text):
    parts = []
    pos = 0
    for match in TOKEN.finditer(text):
        parts.append(escape(text[pos:match.start()]))
        token = match.group()
        if token.startswith("["):
            link = re.fullmatch(r"\[([^\]]+)\]\(([^)]+)\)", token)
            label, url = link.groups()
            if "{" in url or "}" in url or "\\" in url:
                raise ValueError("Unsupported link target")
            parts.append(r"\href{\detokenize{" + url + "}}{" + escape(label) + "}")
        elif token.startswith("`"):
            parts.append(r"\texttt{" + escape(token[1:-1]) + "}")
        elif token.startswith("**"):
            parts.append(r"\textbf{" + escape(token[2:-2]) + "}")
        else:
            parts.append(r"\emph{" + escape(token[1:-1]) + "}")
        pos = match.end()
    parts.append(escape(text[pos:]))
    return "".join(parts)


def render(source_bytes):
    lines = source_bytes.decode("utf-8").splitlines()
    assert lines[0].startswith("# ")
    assert lines[2] == "**Dana Edwards**"
    assert lines[4] == "Draft — October 4, 2026"
    title = lines[0][2:]
    digest = hashlib.sha256(source_bytes).hexdigest()
    out = [
        "% Generated from paper.md; editing requires regeneration.",
        "% paper.md SHA256: " + digest,
        r"\documentclass[11pt]{article}",
        r"\usepackage[T1]{fontenc}",
        r"\usepackage[utf8]{inputenc}",
        r"\usepackage[margin=1in]{geometry}",
        r"\usepackage{amsmath,amssymb,array,booktabs,longtable}",
        r"\usepackage[hidelinks,breaklinks=true]{hyperref}",
        r"\setlength{\emergencystretch}{2em}",
        r"\setlength{\parskip}{0.2em}",
        r"\setcounter{secnumdepth}{0}",
        r"\hypersetup{pdftitle={" + escape(title) +
        r"},pdfauthor={Dana Edwards},pdfsubject={Research design draft; scoped symbolic evidence; proposed neural loop},pdfkeywords={ZenoFCIS, neurosymbolic software factory, translation validation}}",
        r"\title{" + escape(title) + "}",
        r"\author{Dana Edwards}",
        r"\date{Draft---October 4, 2026}",
        r"\begin{document}", r"\maketitle", "",
    ]
    i, abstract = 6, False
    while i < len(lines):
        line = lines[i]
        if not line:
            i += 1
            continue
        if line.startswith("## "):
            if abstract:
                out.extend([r"\end{abstract}", ""])
                abstract = False
            heading = line[3:]
            if heading == "Abstract":
                out.append(r"\begin{abstract}")
                abstract = True
            else:
                out.extend([r"\section{" + inline(heading) + "}", ""])
            i += 1
            continue
        if line.startswith("```"):
            out.extend([r"\begin{small}", r"\begin{verbatim}"])
            i += 1
            while i < len(lines) and not lines[i].startswith("```"):
                if any(ord(c) > 127 for c in lines[i]):
                    raise ValueError("Use ASCII inside manuscript code blocks")
                out.append(lines[i])
                i += 1
            if i == len(lines):
                raise ValueError("Unclosed code block")
            out.extend([r"\end{verbatim}", r"\end{small}", ""])
            i += 1
            continue
        if line.startswith("|"):
            rows = []
            while i < len(lines) and lines[i].startswith("|"):
                rows.append([cell.strip() for cell in lines[i].strip("|").split("|")])
                i += 1
            assert len(rows) >= 3 and all(len(row) == 3 for row in rows)
            assert all(re.fullmatch(r":?-+:?", cell) for cell in rows[1])
            out.extend([
                r"\begingroup\small\setlength{\tabcolsep}{4pt}",
                r"\begin{longtable}{@{}>{\raggedright\arraybackslash}p{0.23\textwidth}>{\raggedright\arraybackslash}p{0.31\textwidth}>{\raggedright\arraybackslash}p{0.38\textwidth}@{}}",
                r"\toprule",
                " & ".join(r"\textbf{" + inline(c) + "}" for c in rows[0]) + " " + chr(92) * 2,
                r"\midrule\endhead",
            ])
            out.extend(" & ".join(inline(c) for c in row) + " " + chr(92) * 2 for row in rows[2:])
            out.extend([r"\bottomrule", r"\end{longtable}", r"\endgroup", ""])
            continue
        paragraph = [line]
        i += 1
        while i < len(lines) and lines[i] and not lines[i].startswith(("## ", "```", "|")):
            paragraph.append(lines[i])
            i += 1
        out.extend([inline(" ".join(paragraph)), ""])
    if abstract:
        out.append(r"\end{abstract}")
    out.extend([r"\end{document}", ""])
    return "\n".join(out)


if __name__ == "__main__":
    source = SOURCE.read_bytes()
    rendered = render(source)
    TARGET.write_text(rendered, encoding="utf-8")
    print("Rendered paper.tex from complete paper.md:", len(source.splitlines()), "lines;")
    print("Markdown SHA256:", hashlib.sha256(source).hexdigest())
