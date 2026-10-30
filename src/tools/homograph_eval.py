"""Homograph check: how often Klattara picks the right pronunciation.

Uses the Wikipedia homograph data of Gorman, Mazovetskiy and Nikolaev (2018):
162 English homographs with about 100 labeled Wikipedia sentences each, such
as "lives" in "their own lives" against "Norman lives at Greenwood". Get it
with

    curl -sSL -o whd.tgz https://codeload.github.com/google-research-datasets/WikipediaHomographData/tar.gz/refs/heads/master
    tar -xzf whd.tgz  (into the eval folder, renamed to wikipedia_homographs)

Each sentence is transcribed by Klattara. The word whose phonemes come
closest to one of the homograph's pronunciations is taken as Klattara's
reading, and counted right when it is closer to the labeled pronunciation
than to the others. Stress counts: "REC-ord" and "re-CORD" differ only in it.

Improve the rules by looking at errors in the training sentences (--split
train) and report the score on the test sentences (the default), which were
set aside for that purpose.

Usage, from the project folder:
    .venv/Scripts/python src/tools/homograph_eval.py [--split eval|train] [--errors N] [--word WORD]
"""

import argparse
import collections
import csv
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DATA = ROOT / "eval" / "wikipedia_homographs" / "data"

# IPA as used in the data, to ARPAbet. Two-letter vowels first.
IPA_VOWELS = [
    ("aɪ", "AY"), ("aʊ", "AW"), ("eɪ", "EY"), ("oʊ", "OW"), ("ɔɪ", "OY"),
    ("ɑ", "AA"), ("æ", "AE"), ("ʌ", "AH"), ("ə", "AH"), ("ɔ", "AO"), ("ɛ", "EH"), ("ɚ", "ER"),
    ("ɪ", "IH"), ("i", "IY"), ("ʊ", "UH"), ("u", "UW"), ("a", "AA"), ("e", "EY"), ("o", "OW"),
]
IPA_CONSONANTS = {
    "b": "B", "d": "D", "f": "F", "ɡ": "G", "g": "G", "h": "HH", "k": "K", "l": "L", "m": "M", "n": "N",
    "ŋ": "NG", "p": "P", "ɹ": "R", "s": "S", "ʃ": "SH", "t": "T", "θ": "TH", "ð": "DH", "v": "V", "w": "W",
    "j": "Y", "z": "Z", "ʒ": "ZH", "ʧ": "CH", "ʤ": "JH",
}
VOWELS = {"AA", "AE", "AH", "AO", "AW", "AY", "EH", "ER", "EY", "IH", "IY", "OW", "OY", "UH", "UW", "AX"}


def from_ipa(ipa):
    """Phonemes as (symbol, stressed or None for consonants)."""
    phonemes, stress, i = [], None, 0
    while i < len(ipa):
        c = ipa[i]
        if c in "'ˈ":
            stress, i = 1, i + 1
            continue
        if c == "ˌ":
            stress, i = 2, i + 1
            continue
        for letters, symbol in IPA_VOWELS:
            if ipa.startswith(letters, i):
                phonemes.append((symbol, stress))
                stress, i = None, i + len(letters)
                break
        else:
            if c in IPA_CONSONANTS:
                phonemes.append((IPA_CONSONANTS[c], None))
            i += 1
    vowels = [k for k, (s, _) in enumerate(phonemes) if s in VOWELS]
    # Unmarked one-syllable words are stressed.
    if vowels and all(phonemes[k][1] is None for k in vowels):
        symbol, _ = phonemes[vowels[0]]
        phonemes[vowels[0]] = (symbol, 1)
    # Only primary stress tells the readings apart; sources mark secondary
    # stress in different places.
    return [(s, (st == 1 if s in VOWELS else None)) for s, st in phonemes]


def from_arpabet(word):
    """Phonemes of one bracketed ARPAbet word, as from_ipa gives them."""
    out = []
    for symbol in word.split():
        base = symbol.rstrip("012")
        if base in VOWELS:
            out.append(("AH" if base == "AX" else base, symbol[-1] == "1"))
        else:
            out.append((base, None))
    return out


def cost(a, b):
    (sa, va), (sb, vb) = a, b
    if (va is None) != (vb is None):
        return 1.0
    if va is None:
        return 0.0 if sa == sb else 1.0
    if not va and not vb:
        # Reduced vowels vary between transcriptions ("ə", "ɪ"), but a full
        # one differs: "separate" ends in "-it" or "-ate".
        reduced = {"AH", "IH", "ER", "UH"}
        return 0.0 if sa == sb else 0.1 if sa in reduced and sb in reduced else 0.6
    return (0.0 if sa == sb else 0.6) + (0.0 if va == vb else 0.4)


def distance(x, y):
    rows = [[0.0] * (len(y) + 1) for _ in range(len(x) + 1)]
    for i in range(len(x) + 1):
        rows[i][0] = float(i)
    for j in range(len(y) + 1):
        rows[0][j] = float(j)
    for i in range(1, len(x) + 1):
        for j in range(1, len(y) + 1):
            rows[i][j] = min(rows[i - 1][j] + 1, rows[i][j - 1] + 1, rows[i - 1][j - 1] + cost(x[i - 1], y[j - 1]))
    return rows[-1][-1]


def load(split):
    readings = collections.defaultdict(dict)
    with open(DATA / "wordids.tsv", encoding="utf-8") as f:
        for row in list(csv.reader(f, delimiter="\t"))[1:]:
            readings[row[0]][row[1]] = from_ipa(row[3])
    examples = []
    for path in sorted((DATA / split).glob("*.tsv")):
        with open(path, encoding="utf-8") as f:
            for row in list(csv.reader(f, delimiter="\t"))[1:]:
                examples.append((row[0], row[1], row[2]))
    return readings, examples


def transcribe(sentences):
    result = subprocess.run(
        ["cargo", "run", "--release", "--quiet", "--example", "transcribe"],
        cwd=ROOT,
        input="\n".join(s.replace("\n", " ") for s in sentences) + "\n",
        capture_output=True,
        text=True,
        encoding="utf-8",
        check=True,
    )
    return result.stdout.splitlines()


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    parser = argparse.ArgumentParser()
    parser.add_argument("--split", default="eval", choices=["eval", "train"])
    parser.add_argument("--errors", type=int, default=0, help="show this many wrong sentences per word")
    parser.add_argument("--word", help="only this homograph")
    args = parser.parse_args()
    readings, examples = load(args.split)
    # Only sentences with the homograph once, so its reading can be found.
    usable = [
        e for e in examples
        if len(re.findall(rf"\b{re.escape(e[0])}\b", e[2], re.IGNORECASE)) == 1 and (not args.word or e[0] == args.word)
    ]
    lines = transcribe([e[2] for e in usable])
    by_word = collections.defaultdict(lambda: [0, 0])
    majority = collections.defaultdict(collections.Counter)
    errors = collections.defaultdict(list)
    for (word, gold, sentence), line in zip(usable, lines):
        majority[word][gold] += 1
        groups = [from_arpabet(g) for g in re.findall(r"\[([^\]]*)\]", line)]
        candidates = readings[word]
        best = min(groups, key=lambda g: min(distance(g, c) for c in candidates.values()), default=[])
        scores = sorted((distance(best, c), wordid) for wordid, c in candidates.items())
        chosen = scores[0][1] if len(scores) < 2 or scores[0][0] < scores[1][0] else None
        right = chosen == gold
        by_word[word][0] += right
        by_word[word][1] += 1
        if not right:
            errors[word].append((gold, chosen, sentence))
    total_right = sum(r for r, _ in by_word.values())
    total = sum(n for _, n in by_word.values())
    # The simplest baseline: always the reading most common in this split.
    common = sum(c.most_common(1)[0][1] for c in majority.values())
    print(f"{args.split} split: {len(usable)} sentences of {len(examples)} (the homograph appears once)")
    print(f"Klattara: {total_right / total:.1%} right")
    print(f"Always the most common reading: {common / total:.1%} right")
    print("\nWords below 80%, worst first:")
    for word, (right, n) in sorted(by_word.items(), key=lambda kv: kv[1][0] / kv[1][1]):
        if right / n < 0.8:
            mistakes = collections.Counter(f"{g} read as {c or 'unclear'}" for g, c, _ in errors[word])
            print(f"  {word}: {right}/{n}; " + "; ".join(f"{m} x{k}" for m, k in mistakes.most_common(2)))
            for gold, chosen, sentence in errors[word][: args.errors]:
                print(f"      [{gold} -> {chosen}] {sentence}")


if __name__ == "__main__":
    main()
