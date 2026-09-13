---
name: pdf-reading
description: How to extract text from PDF files when the read_pdf tool is unavailable
---

# Reading PDFs

`read_pdf` comes from the `pdf-reader` plugin. If it is not installed, use
`run_command` with one of these, in order of preference:

1. **Math-heavy papers** — `marker_single paper.pdf --output_format markdown`
   (needs `pip install marker-pdf`). Produces Markdown with LaTeX.
2. **Plain text PDFs** — `pdftotext -layout paper.pdf -` (needs poppler-utils).
   Add `-f <first> -l <last>` to restrict the page range.
3. **Scanned PDFs have no text layer.** Extract nothing useful with the above;
   say so instead of guessing from an empty result, and suggest OCR.

Always read a page range first (`-f 1 -l 5`) before pulling a whole book: a
1000-page PDF will blow the context window.
