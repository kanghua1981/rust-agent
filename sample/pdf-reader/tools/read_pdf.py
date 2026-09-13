#!/usr/bin/env python3
"""Extract text from a PDF: marker_single first, then pdftotext.

Plugin tools receive their arguments as JSON in the AGENT_PARAMS environment
variable and print their result on stdout.
"""
import json
import os
import shutil
import subprocess
import sys


def run(cmd):
    try:
        proc = subprocess.run(cmd, capture_output=True, text=True, timeout=300)
    except (OSError, subprocess.TimeoutExpired):
        return None
    if proc.returncode == 0 and proc.stdout.strip():
        return proc.stdout
    return None


def fail(message):
    print(json.dumps({"success": False, "error": message}), file=sys.stderr)
    sys.exit(1)


def main():
    params = json.loads(os.environ.get("AGENT_PARAMS", "{}"))
    path = params.get("path")
    if not path:
        fail("path is required")
    path = os.path.abspath(path)
    if not os.path.isfile(path):
        fail("not a file: %s" % path)

    start = params.get("start_page")
    end = params.get("end_page")
    max_chars = int(params.get("max_chars") or 20000)

    text = None
    if shutil.which("marker_single"):
        # marker writes Markdown next to the PDF and prints the output path;
        # it is only worth the dependency for math-heavy documents.
        text = run(["marker_single", path, "--output_format", "markdown"])

    if text is None:
        converter = shutil.which("pdftotext")
        if not converter:
            fail("no PDF converter found: install marker-pdf (pip) or poppler-utils (pdftotext)")
        cmd = [converter, "-layout"]
        if start:
            cmd += ["-f", str(start)]
        if end:
            cmd += ["-l", str(end)]
        cmd += [path, "-"]
        text = run(cmd)

    if text is None or not text.strip():
        fail("no text extracted (scanned PDF? try OCR, e.g. ocrmypdf)")

    text = text.strip()
    if len(text) > max_chars:
        text = text[:max_chars] + "\n\n[... truncated at %d chars]" % max_chars
    print(text)


if __name__ == "__main__":
    main()
