#!/usr/bin/env python3
"""Validate the edited learner vocabulary and export the two runtime files."""

import argparse
import csv
from pathlib import Path
import re
import sys


ROOT = Path(__file__).resolve().parents[1]
DATA = ROOT / "data"
LEVELS = {"A1", "A2", "B1", "B2"}
PATTERNS = {
    "en": re.compile(r"[A-Za-z]+(?:[ '-][A-Za-z]+)*"),
    "ru": re.compile(r"[А-Яа-яЁё]+(?:[ -][А-Яа-яЁё]+)*"),
}


def read_words():
    with (DATA / "words.tsv").open(encoding="utf-8", newline="") as source:
        reader = csv.DictReader(source, delimiter="\t")
        if reader.fieldnames != ["en", "ru", "level", "source"]:
            raise ValueError("words.tsv: expected en, ru, level, source columns")
        rows = list(reader)
    if not rows:
        raise ValueError("words.tsv: vocabulary is empty")

    seen = set()
    for line, row in enumerate(rows, start=2):
        if set(row) != {"en", "ru", "level", "source"}:
            raise ValueError(f"words.tsv:{line}: invalid column count")
        for lang, pattern in PATTERNS.items():
            text = row[lang]
            if text is None or not 1 <= len(text) <= 24 or not pattern.fullmatch(text):
                raise ValueError(f"words.tsv:{line}: invalid {lang} entry {text!r}")
        if row["en"] in seen:
            raise ValueError(f"words.tsv:{line}: duplicate English entry {row['en']!r}")
        seen.add(row["en"])
        if row["source"] == "Oxford":
            if row["level"] not in LEVELS:
                raise ValueError(f"words.tsv:{line}: expected Oxford level A1–B2")
        elif row["source"] == "Everyday":
            if row["level"] != "":
                raise ValueError(f"words.tsv:{line}: everyday additions have no CEFR label")
        else:
            raise ValueError(f"words.tsv:{line}: unknown source {row['source']!r}")
    return rows


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="check exports without writing")
    args = parser.parse_args()
    try:
        rows = read_words()
        outputs = {
            DATA / f"words_{lang}.txt": "".join(row[lang] + "\n" for row in rows).encode("utf-8")
            for lang in ("en", "ru")
        }
        if args.check:
            for path, expected in outputs.items():
                if path.read_bytes() != expected:
                    raise ValueError(f"{path.name} differs from words.tsv; run scripts/build_words.py")
        else:
            for path, content in outputs.items():
                path.write_bytes(content)
    except (OSError, ValueError, csv.Error) as error:
        print(error, file=sys.stderr)
        return 1
    print(f"{'Checked' if args.check else 'Exported'} {len(rows):,} aligned learner word pairs.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
