"""Trains Klattara's letter-to-sound model from CMUdict.

The model is a joint-sequence model (Bisani & Ney 2002): every word is a
chain of joint units, each a letter together with the phonemes it stands
for, and an M-gram model gives the probability of each unit after the ones
before it. The engine pronounces an unknown word by finding the most likely
chain of units that spells it.

1. Alignment, after Black, Lenzo & Pagel (1998): every letter yields no
   phoneme, one, or two, chosen from a hand-written table of what each
   letter may sound like, and expectation maximization picks the most
   likely alignment of every word. Each letter and its phonemes, with
   stress, form one unit.
2. An M-gram model over the units with interpolated Kneser-Ney smoothing
   (Chen & Goodman 1998), with discounts from the counts of counts.
3. Pruning: long-context entries whose probability is close to what the
   shorter context already predicts are dropped, to keep the model small.

It also writes a table of word endings that fix the main stress, such as
-ation, since a model of the last few units can't see how far a stress is
from the end of the word.

Usage, from the project folder:
    .venv/Scripts/python src/tools/train_lts.py
        trains the shipped model on every word: src/data/lts/joint.bin and
        stress_endings.tsv
    .venv/Scripts/python src/tools/train_lts.py --evaluate
        trains on 90% of the words and writes target/lts-eval, for
        cargo run --release --example lts_eval -- target/lts-eval
Settings: LTS_ORDER (default 5) and LTS_PRUNE (log10 threshold, default 0.2).
"""

import math
import os
import struct
import sys
from collections import Counter, defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
DICTIONARY = ROOT / "src" / "data" / "cmudict" / "cmudict.dict"
MODEL = ROOT / "src" / "data" / "lts" / "joint.bin"
EVALUATION = ROOT / "target" / "lts-eval"

ORDER = int(os.environ.get("LTS_ORDER", 5))
PRUNE = float(os.environ.get("LTS_PRUNE", 0.2))
# A word ending goes in the stress table if at least this many words have it,
# and this share of them stress the same syllable.
ENDING_WORDS = 20
ENDING_SHARE = 0.95

# The phoneme codes used by the engine (src/engine/src/arpabet.rs).
SYMBOLS = (
    "AA AE AH AO AW AY B CH D DH EH ER EY F G HH IH IY JH K L M N NG OW OY P R S SH T TH UH UW V W Y Z ZH"
).split()
VOWELS = "AA AE AH AO AW AY EH ER EY IH IY OW OY UH UW".split()

# What each letter may sound like: phonemes without stress, "_" for none.
VOWEL_LETTER = VOWELS + ["_", "Y", "W"] + [f"Y {v}" for v in VOWELS] + [f"W {v}" for v in VOWELS]
ALLOWED = {
    "a": VOWEL_LETTER,
    "e": VOWEL_LETTER,
    "i": VOWEL_LETTER,
    "o": VOWEL_LETTER,
    "u": VOWEL_LETTER,
    "y": VOWEL_LETTER,
    "b": ["B", "_"],
    "c": ["K", "S", "CH", "SH", "_", "K S", "Z"],
    "d": ["D", "T", "JH", "_"],
    "f": ["F", "V", "_"],
    "g": ["G", "JH", "ZH", "F", "K", "_", "NG"],
    "h": ["HH", "_"],
    "j": ["JH", "Y", "HH", "ZH", "_"],
    "k": ["K", "_"],
    "l": ["L", "_", "AH L"],
    "m": ["M", "_", "AH M"],
    "n": ["N", "NG", "_", "AH N"],
    "p": ["P", "F", "_"],
    "q": ["K", "K W"],
    "r": ["R", "ER", "_", "AH R"],
    "s": ["S", "Z", "SH", "ZH", "_"],
    "t": ["T", "SH", "CH", "TH", "DH", "D", "_", "T S"],
    "v": ["V", "F"],
    "w": ["W", "_", "V", "HH W"],
    "x": ["K S", "G Z", "K SH", "Z", "S", "K", "G ZH"],
    "z": ["Z", "S", "ZH", "T S", "_"],
}

EOS, BOS = 0, 1
# Log10 probabilities and backoff weights are stored in one byte each,
# covering 0 down to -RANGE.
RANGE = 7.0


def load_dictionary():
    """Main pronunciations of plain alphabetic words: {word: [phonemes with stress]}."""
    words = {}
    for line in DICTIONARY.read_text(encoding="utf-8").splitlines():
        line = line.split("#")[0].strip()
        if not line:
            continue
        head, *phones = line.split()
        if "(" in head or not head.isalpha() or not head.isascii():
            continue
        words[head] = phones
    return words


def load_variants():
    """Every listed pronunciation of each word, for scoring."""
    variants = defaultdict(list)
    for line in DICTIONARY.read_text(encoding="utf-8").splitlines():
        line = line.split("#")[0].strip()
        if line:
            head, *phones = line.split()
            variants[head.split("(")[0]].append(phones)
    return variants


def base(phone):
    return phone.rstrip("012")


def align(word, phones, logp):
    """Best alignment of letters to phoneme groups, or None. logp maps
    (letter, group without stress) to a log probability."""
    n, m = len(word), len(phones)
    bases = [base(p) for p in phones]
    best = [[-math.inf] * (m + 1) for _ in range(n + 1)]
    back = [[0] * (m + 1) for _ in range(n + 1)]
    best[0][0] = 0.0
    for i in range(1, n + 1):
        letter = word[i - 1]
        for j in range(0, m + 1):
            for k in (0, 1, 2):
                if k > j or best[i - 1][j - k] == -math.inf:
                    continue
                group = " ".join(bases[j - k : j]) if k else "_"
                score = logp.get((letter, group))
                if score is None:
                    continue
                total = best[i - 1][j - k] + score
                if total > best[i][j]:
                    best[i][j], back[i][j] = total, k
    if best[n][m] == -math.inf:
        return None
    groups, j = [], m
    for i in range(n, 0, -1):
        k = back[i][j]
        groups.append(" ".join(phones[j - k : j]) if k else "_")
        j -= k
    return groups[::-1]


def learn_alignments(words, iterations=5):
    logp = {(letter, group): 0.0 for letter, groups in ALLOWED.items() for group in groups}
    aligned = {}
    for iteration in range(iterations):
        counts = Counter()
        aligned = {}
        for word, phones in words.items():
            groups = align(word, phones, logp)
            if groups:
                aligned[word] = groups
                counts.update((letter, " ".join(base(p) for p in g.split()) if g != "_" else "_") for letter, g in zip(word, groups))
        totals = Counter()
        for (letter, _), c in counts.items():
            totals[letter] += c
        logp = {
            (letter, group): math.log((counts[(letter, group)] + 0.1) / (totals[letter] + 0.1 * len(ALLOWED[letter])))
            for letter, groups in ALLOWED.items()
            for group in groups
        }
        print(f"  alignment pass {iteration + 1}: {len(aligned)} of {len(words)} words aligned", flush=True)
    return aligned


def units(aligned):
    """Numbers each (letter, phonemes) unit; 0 and 1 are the end and start marks."""
    ids = {"</s>": EOS, "<s>": BOS}
    sequences = []
    for word, groups in aligned.items():
        sequences.append([ids.setdefault((letter, group), len(ids)) for letter, group in zip(word, groups)])
    return ids, sequences


def kneser_ney(sequences, order):
    """Interpolated Kneser-Ney: probs[n][gram] is P(last | rest) for every
    seen n-gram, and gammas[n][context] the weight of the shorter context."""
    counts = [None] + [defaultdict(int) for _ in range(order)]
    for sequence in sequences:
        padded = [BOS] * (order - 1) + sequence + [EOS]
        for i in range(order - 1, len(padded)):
            counts[order][tuple(padded[i - order + 1 : i + 1])] += 1
    # Lower orders count distinct left extensions, not occurrences.
    for n in range(order - 1, 0, -1):
        for gram in counts[n + 1]:
            counts[n][gram[1:]] += 1
    discounts = [None]
    for n in range(1, order + 1):
        histogram = Counter(min(c, 3) for c in counts[n].values())
        n1, n2 = histogram[1], histogram[2]
        discounts.append(n1 / (n1 + 2 * n2) if n1 + n2 else 0.5)
    vocabulary = len(counts[1]) + 1
    probs = [None] + [{} for _ in range(order)]
    gammas = [None] + [{} for _ in range(order)]
    total = sum(counts[1].values())
    gamma = discounts[1] * len(counts[1]) / total
    for gram, c in counts[1].items():
        probs[1][gram] = max(c - discounts[1], 0) / total + gamma / vocabulary
    for n in range(2, order + 1):
        totals, types = defaultdict(int), defaultdict(int)
        for gram, c in counts[n].items():
            totals[gram[:-1]] += c
            types[gram[:-1]] += 1
        gammas[n] = {h: discounts[n] * types[h] / totals[h] for h in totals}
        for gram, c in counts[n].items():
            h = gram[:-1]
            probs[n][gram] = max(c - discounts[n], 0) / totals[h] + gammas[n][h] * probs[n - 1][gram[1:]]
    return probs, gammas


def prune(probs, gammas, order, threshold):
    """Which n-grams to keep: all unigrams and bigrams, every context of a
    kept longer n-gram, and others only where they differ from the backed-off
    estimate by more than `threshold` (log10)."""
    keep = [None] + [set(probs[n]) for n in range(1, order + 1)]
    for n in range(order, 2, -1):
        needed = {gram[:-1] for gram in keep[n + 1]} if n < order else set()
        for gram in list(keep[n]):
            if gram in needed:
                continue
            backed_off = gammas[n][gram[:-1]] * probs[n - 1][gram[1:]]
            if abs(math.log10(probs[n][gram]) - math.log10(backed_off)) < threshold:
                keep[n].discard(gram)
    return keep


def quantize(value):
    log = -math.log10(value) if value > 0 else RANGE
    return max(0, min(255, round(log / RANGE * 255)))


def encode_group(group):
    if group == "_":
        return b""
    codes = []
    for phone in group.split():
        stress = int(phone[-1]) + 1 if phone[-1].isdigit() else 0
        codes.append(SYMBOLS.index(base(phone)) | (stress << 6))
    return bytes(codes)


def write_model(path, ids, probs, gammas, keep, order):
    """Layout, little-endian:
    "KJSM", version (u32, 2), order M (u32), unit count (u32)
    per unit: letter (u8, 1 to 26; 0 for the marks), phoneme count (u8),
        phoneme codes (as in src/engine/src/arpabet.rs)
    then for each order n = 1 to M: entry count (u32) and the entries.
        Below order M an entry is unit (u16), probability (u8), backoff
        weight (u8) and first child (u32), the index of its first extension
        in the next order (its children run to the next entry's first
        child), and the entries end with a u32 sentinel. At order M, which
        has no extensions, an entry is just unit (u16) and probability (u8).
        Order 1 has one entry per unit, in unit order; higher orders are
        sorted by parent, then unit. Probabilities and weights are stored
        as -log10 scaled so that 255 means -7.
    """
    names = sorted(ids, key=ids.get)
    out = bytearray(b"KJSM")
    out += struct.pack("<III", 2, order, len(names))
    for name in names:
        if isinstance(name, str):
            out += struct.pack("<BB", 0, 0)
            continue
        letter, group = name
        codes = encode_group(group)
        out += struct.pack("<BB", ord(letter) - 96, len(codes)) + codes
    # Every context must have an entry to hang its extensions on, even ones
    # never seen as n-grams themselves, like two start marks in a row.
    for n in range(order, 2, -1):
        keep[n - 1].update(gram[:-1] for gram in keep[n])
    levels = [None, [(u,) for u in range(len(names))]]
    for n in range(2, order + 1):
        index = {gram: i for i, gram in enumerate(levels[n - 1])}
        levels.append(sorted(keep[n], key=lambda g: (index[g[:-1]], g[-1])))
    entries = 0
    for n in range(1, order + 1):
        level = levels[n]
        children = Counter(g[:-1] for g in levels[n + 1]) if n < order else Counter()
        out += struct.pack("<I", len(level))
        first = 0
        for gram in level:
            prob = probs[n].get(gram, 0.0)
            if n == order:
                out += struct.pack("<HB", gram[-1], quantize(prob))
                continue
            weight = gammas[n + 1].get(gram, 1.0)
            out += struct.pack("<HBBI", gram[-1], quantize(prob), quantize(weight), first)
            first += children[gram]
        if n < order:
            out += struct.pack("<I", first)
        entries += len(level)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(bytes(out))
    print(f"Wrote {path} ({len(out) / 1e6:.2f} MB, {entries} entries, order {order}, prune {PRUNE})")


def stress_endings(words):
    """Word endings of 3 to 6 letters that nearly always put the main stress
    on the same syllable, counted from the end of the word: {ending: place}."""
    counts = defaultdict(Counter)
    for word, phones in words.items():
        stresses = [p[-1] for p in phones if p[-1] in "012"]
        if len(stresses) >= 2 and "1" in stresses:
            place = len(stresses) - stresses.index("1")
            for k in range(3, 7):
                if len(word) > k:
                    counts[word[-k:]][place] += 1
    table = {}
    for ending, places in counts.items():
        place, most = places.most_common(1)[0]
        total = sum(places.values())
        if total >= ENDING_WORDS and most / total >= ENDING_SHARE:
            table[ending] = place
    return table


def train(words, path):
    table = stress_endings(words)
    path.parent.mkdir(parents=True, exist_ok=True)
    lines = "".join(f"{ending}\t{table[ending]}\n" for ending in sorted(table))
    path.with_name("stress_endings.tsv").write_text(lines, encoding="utf-8", newline="\n")
    print(f"  {len(table)} stress endings", flush=True)
    aligned = learn_alignments(words)
    ids, sequences = units(aligned)
    print(f"  {len(ids)} letter-phoneme units", flush=True)
    probs, gammas = kneser_ney(sequences, ORDER)
    keep = prune(probs, gammas, ORDER, PRUNE)
    write_model(path, ids, probs, gammas, keep, ORDER)


def main():
    words = load_dictionary()
    print(f"{len(words)} dictionary words with plain letters", flush=True)
    if "--evaluate" in sys.argv:
        held_out = {w for i, w in enumerate(sorted(words)) if i % 10 == 0}
        training = {w: p for w, p in words.items() if w not in held_out}
        train(training, EVALUATION / "model.bin")
        variants = load_variants()
        lines = [w + "\t" + " | ".join(" ".join(v) for v in variants[w]) for w in sorted(held_out)]
        (EVALUATION / "heldout.tsv").write_text("\n".join(lines) + "\n", encoding="utf-8")
        print(f"Wrote {len(lines)} held-out words to {EVALUATION / 'heldout.tsv'}")
    else:
        train(words, MODEL)


if __name__ == "__main__":
    main()
