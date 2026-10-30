"""Builds src/data/homographs.tsv, Klattara's table of homographs.

From the training half of the Wikipedia homograph data (Gorman, Mazovetskiy
and Nikolaev 2018; see homograph_eval.py for how to get it), for each of its
162 homographs:

- each reading's pronunciation: the CMUdict variant that matches the data's
  IPA transcription closely, including secondary stress, and better than it
  matches the other readings; otherwise the transcription itself, converted
  to ARPAbet ("document" as a verb needs its last syllable's secondary stress,
  which CMUdict lacks);
- each reading's role, noun or adjective, verb, or other (names, places,
  tenses), from the data's labels, so the engine's context rules can choose
  between a noun and a verb;
- the most common reading, both for the word in lowercase and for the word
  capitalized inside a sentence, where it is usually a name: "Polish",
  "Nestle", "Ravel". A capital at the start of a sentence says nothing, so
  those sentences count only when there is nothing else.

Only the training sentences are used, so homograph_eval.py can still score
the engine fairly on the test sentences.

Usage, from the project folder:
    .venv/Scripts/python src/tools/build_homographs.py
"""

import collections
import csv
import re
import sys

from homograph_eval import DATA, IPA_CONSONANTS, IPA_VOWELS, ROOT, VOWELS

CMUDICT = ROOT / "src" / "data" / "cmudict" / "cmudict.dict"
OUTPUT = ROOT / "src" / "data" / "homographs.tsv"
# Fewer capitalized examples than this, and capitals are not trusted.
MIN_CAPITALIZED = 3
# How far a CMUdict variant may be from the data's transcription.
MAX_DISTANCE = 0.6


def cmudict_variants(words):
    variants = collections.defaultdict(list)
    with open(CMUDICT, encoding="utf-8") as f:
        for line in f:
            head, _, rest = line.partition(" ")
            word = head.split("(")[0]
            if word in words:
                variants[word].append(rest.split("#")[0].strip())
    return variants


def ipa_to_arpabet(ipa):
    """The data's IPA as ARPAbet, with primary (1) and secondary (2) stress."""
    out, stress, i = [], "0", 0
    while i < len(ipa):
        c = ipa[i]
        if c in "'ˈ":
            stress, i = "1", i + 1
            continue
        if c == "ˌ":
            stress, i = "2", i + 1
            continue
        for letters, symbol in IPA_VOWELS:
            if ipa.startswith(letters, i):
                out.append(symbol + stress)
                stress, i = "0", i + len(letters)
                break
        else:
            if c in IPA_CONSONANTS:
                out.append(IPA_CONSONANTS[c])
            i += 1
    vowels = [k for k, p in enumerate(out) if p[:-1] in VOWELS]
    # Unmarked one-syllable words are stressed.
    if vowels and all(not out[k].endswith(("1", "2")) for k in vowels):
        out[vowels[0]] = out[vowels[0]][:-1] + "1"
    # Before R, CMUdict writes the vowels of "ear" and "tour" as IH and UH.
    for k in range(len(out) - 1):
        if out[k + 1] == "R" and out[k][:-1] in ("IY", "UW"):
            out[k] = {"IY": "IH", "UW": "UH"}[out[k][:-1]] + out[k][-1]
    return " ".join(out)


def strict_distance(a, b):
    """Distance between ARPAbet strings that counts every stress level."""
    x, y = a.split(), b.split()
    reduced = {"AH", "IH", "ER", "UH"}

    def cost(p, q):
        pv, qv = p.rstrip("012"), q.rstrip("012")
        if (pv in VOWELS) != (qv in VOWELS):
            return 1.0
        if pv not in VOWELS:
            return 0.0 if p == q else 1.0
        ps, qs = p[-1], q[-1]
        quality = 0.0 if pv == qv else 0.1 if ps == qs == "0" and pv in reduced and qv in reduced else 1.0
        stress = 0.0 if ps == qs else 0.5 if "1" in (ps, qs) else 0.2
        return quality + stress

    rows = [[float(i + j) if i * j == 0 else 0.0 for j in range(len(y) + 1)] for i in range(len(x) + 1)]
    for i in range(1, len(x) + 1):
        for j in range(1, len(y) + 1):
            rows[i][j] = min(rows[i - 1][j] + 1, rows[i][j - 1] + 1, rows[i - 1][j - 1] + cost(x[i - 1], y[j - 1]))
    return rows[-1][-1]


def role(wordid, label):
    """noun, verb or other, from the data's description of the reading."""
    label = label.strip().lower()
    if label == "verb":
        return "verb"
    if label in {"noun", "adjective", "adjective-noun", "noun-adjective"}:
        return "noun"
    parts = wordid.split("_", 1)[1].split("-") if "_" in wordid else []
    if parts == ["vrb"]:
        return "verb"
    if parts and set(parts) <= {"nou", "adj"}:
        return "noun"
    return "other"


def case_of(sentence, start):
    """lower, capital (inside a sentence) or initial (capital starting one)."""
    if not sentence[start : start + 1].isupper():
        return "lower"
    if start == 0 or re.search(r"[.!?:\"“(]\s*$", sentence[:start]):
        return "initial"
    return "capital"


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    readings = collections.defaultdict(list)
    with open(DATA / "wordids.tsv", encoding="utf-8") as f:
        for row in list(csv.reader(f, delimiter="\t"))[1:]:
            readings[row[0]].append((row[1], row[2], ipa_to_arpabet(row[3])))
    counts = collections.defaultdict(lambda: collections.defaultdict(collections.Counter))
    for path in sorted((DATA / "train").glob("*.tsv")):
        with open(path, encoding="utf-8") as f:
            for word, wordid, sentence, start, _ in list(csv.reader(f, delimiter="\t"))[1:]:
                # The data gives byte offsets.
                at = len(sentence.encode("utf-8")[: int(start)].decode("utf-8", errors="ignore"))
                counts[word][case_of(sentence, at)][wordid] += 1
    variants = cmudict_variants(set(readings))
    lines = [
        "# Klattara's homographs, built by src/tools/build_homographs.py from the",
        "# training half of the Wikipedia homograph data (Gorman, Mazovetskiy and",
        "# Nikolaev 2018; Apache License 2.0) and CMUdict. Each line: the word, the",
        "# reading most common in lowercase and when capitalized inside a sentence",
        "# (numbered from 0), then each reading as role:ARPAbet.",
    ]
    converted = 0
    for word in sorted(readings):
        entries = []
        for wordid, label, from_data in readings[word]:
            others = [a for w, _, a in readings[word] if w != wordid]
            choice = from_data
            for variant in sorted(variants.get(word, []), key=lambda v: strict_distance(v, from_data)):
                near = strict_distance(variant, from_data)
                if near <= MAX_DISTANCE and all(near < strict_distance(variant, o) for o in others):
                    choice = variant
                break
            converted += choice == from_data
            entries.append((wordid, role(wordid, label), choice))
        ids = [wordid for wordid, _, _ in entries]
        lower, capital, initial = (counts[word][k] for k in ("lower", "capital", "initial"))
        overall = lower + capital + initial
        lower_best = (lower or initial or overall).most_common(1)[0][0] if overall else ids[0]
        capital_best = capital.most_common(1)[0][0] if sum(capital.values()) >= MIN_CAPITALIZED else lower_best
        fields = [word, str(ids.index(lower_best)), str(ids.index(capital_best))]
        fields += [f"{r}:{a}" for _, r, a in entries]
        lines.append("\t".join(fields))
    OUTPUT.write_text("\n".join(lines) + "\n", encoding="utf-8")
    print(f"Wrote {len(readings)} homographs to {OUTPUT}; {converted} readings converted from the data's IPA.")


if __name__ == "__main__":
    main()
