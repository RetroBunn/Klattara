"""Text normalization check: how Klattara reads numbers, dates, abbreviations
and the like, against the reference readings of Sproat and Jaitly (2016).

Their English data is Wikipedia text, one token per line with its class,
its written form and how Google's Kestrel system reads it aloud ("DATE",
"Jan. 5", "january fifth"). It is on Kaggle (richardwilliamsproat/
text-normalization-for-english-russian-and-polish). The standard test set is
the first 100,002 lines of en_with_types/output-00099-of-00100; put them in
eval/text_normalization/evaluation.tsv:

    head -100002 eval/en_with_types/output-00099-of-00100 > eval/text_normalization/evaluation.tsv

Look for errors in slices of the other files instead, so the test set stays
unseen; for example, ten files' first 100,002 lines:

    for i in 00 01 02 03 04 05 06 07 08 09; do head -100002 eval/en_with_types/output-000$i-of-00100; done > eval/text_normalization/dev10.tsv
    .venv/Scripts/python src/tools/normalization_eval.py --file eval/text_normalization/dev10.tsv

Each token that the reference reads differently from how it is written is
checked: Klattara transcribes the sentence as written, and again with that
token replaced by its reference reading in words. When the two give the same
phonemes, stress aside, Klattara read the token as the reference does.
Neighboring changed tokens are replaced together, since a range like
"33 - 38" is read as one ("thirty three to thirty eight"), and replacing
only part of it ("thirty three - 38") would read differently from both. Results are grouped by the token's class. The reference is
one system's choices, with some British habits, so treat the numbers as a
guide to what to look at; "o" for zero in numbers counts as "zero". Where
Klattara reads a neighbor by what follows it, as "p." is "page" before a
number, replacing the number with words changes that neighbor too, and the
number is counted wrong.

Usage, from the project folder:
    .venv/Scripts/python src/tools/normalization_eval.py [--file path] [--errors N] [--class CLASS]
"""

import argparse
import collections
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DEFAULT = ROOT / "eval" / "text_normalization" / "evaluation.tsv"
NO_SPACE_BEFORE = {",", ".", ":", ";", "?", "!", ")", "]", "'s", "%"}
NO_SPACE_AFTER = {"(", "["}


def sentences(path):
    """Each sentence as a list of (class, written, spoken) tokens."""
    current = []
    with open(path, encoding="utf-8") as f:
        for line in f:
            fields = line.rstrip("\n").split("\t")
            if fields[0] == "<eos>":
                if current:
                    yield current
                current = []
            elif len(fields) == 3:
                current.append(tuple(fields))
    if current:
        yield current


def join(parts):
    text = ""
    for part in parts:
        if text and part not in NO_SPACE_BEFORE and text[-1] not in NO_SPACE_AFTER:
            text += " "
        text += part
    return text


NUMERIC = {"ADDRESS", "DECIMAL", "DIGIT", "FRACTION", "MEASURE", "MONEY", "TELEPHONE", "TIME"}


def spoken_form(cls, written, spoken):
    if spoken == "<self>":
        return written
    if spoken == "sil":
        return written if cls == "PUNCT" else ","
    # "a_letter" and the like are letters said by name; "sil" inside a
    # reading is a pause; "o" in a number is zero.
    words = [w.replace("_letter", "") if w.endswith("_letter") else w for w in spoken.split()]
    words = ["zero" if w == "o" and cls in NUMERIC else w for w in words]
    words = ["," if w == "sil" else w for w in words]
    # Letters said by name ("a l c s") are written dotted, "A.L.C.S.", so
    # they are spelled rather than read as the words "a" and "I".
    out, letters = [], []
    for w in words + [""]:
        if len(w) == 1 and w.isalpha() and cls != "PLAIN":
            letters.append(w.upper())
            continue
        if letters:
            out.append(".".join(letters) + ".")
            letters = []
        if w:
            out.append(w)
    return " ".join(out)


def phonemes(transcription):
    return re.sub(r"[0-2]", "", " ".join(re.findall(r"\[([^\]]*)\]", transcription))).split()


def transcribe(texts):
    result = subprocess.run(
        ["cargo", "run", "--release", "--quiet", "--example", "transcribe"],
        cwd=ROOT,
        input="\n".join(texts) + "\n",
        capture_output=True,
        text=True,
        encoding="utf-8",
        check=True,
    )
    return result.stdout.splitlines()


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    parser = argparse.ArgumentParser()
    parser.add_argument("--file", type=Path, default=DEFAULT)
    parser.add_argument("--errors", type=int, default=0, help="show this many wrong sentences per class")
    parser.add_argument("--class", dest="only", help="show errors only for this class")
    args = parser.parse_args()
    written, checks = [], []
    for tokens in sentences(args.file):
        text = join([w for _, w, _ in tokens])
        changed = [spoken not in ("<self>", "sil") for _, _, spoken in tokens]
        i = 0
        while i < len(tokens):
            if not changed[i]:
                i += 1
                continue
            end = i
            while end < len(tokens) and changed[end]:
                end += 1
            hybrid = join([spoken_form(c, t, sp) if i <= k < end else t for k, (c, t, sp) in enumerate(tokens)])
            for cls, w, spoken in tokens[i:end]:
                checks.append((cls if cls != "PLAIN" else "PLAIN (changed)", w, spoken, text, len(written), hybrid))
            i = end
        written.append(text)
    out = transcribe(written + [c[5] for c in checks])
    ours, theirs = out[: len(written)], out[len(written) :]
    by_class = collections.defaultdict(lambda: [0, 0])
    errors = collections.defaultdict(list)
    for (cls, w, spoken, text, k, _), reference in zip(checks, theirs):
        right = phonemes(ours[k]) == phonemes(reference)
        by_class[cls][0] += right
        by_class[cls][1] += 1
        if not right:
            errors[cls].append((w, spoken, text))
    total = sum(n for _, n in by_class.values())
    right_all = sum(r for r, _ in by_class.values())
    print(f"{len(written)} sentences, {total} tokens read differently from how they are written")
    print(f"{right_all / total:.1%} of those read as the reference reads them")
    print("\nBy class:")
    for cls, (right, n) in sorted(by_class.items(), key=lambda kv: -kv[1][1]):
        print(f"  {cls}: {right}/{n} ({right / n:.0%})")
        if args.errors and (not args.only or args.only == cls):
            for w, spoken, text in errors[cls][: args.errors]:
                print(f"      {w} -> {spoken}    | {text[:100]}")

if __name__ == "__main__":
    main()
