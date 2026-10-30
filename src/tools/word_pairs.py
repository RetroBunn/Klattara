"""Word-pair check: which sound differences survive at a speaking rate.

The words of each pair differ in one sound: "bat" and "pat" differ only in
voicing. Every word is spoken in a carrier, "Say ___ again." unless another
is given, six to a recording. Whisper then scores how well each written word fits the audio,
and the spoken word is compared with its partner in the same place. Whisper
favors a word it has just heard, so no recording holds either word of a pair
twice: "sit" spoken just before "sick" made "sick" sound like "sit".

Whisper favors common words, so each pair is compared both ways: the margin
is how much better "bat" fits the "bat" recording than "pat" does, plus how
much better "pat" fits the "pat" recording than "bat" does. That cancels the
preference for either word. A pair counts as told apart when its margin is
positive; larger margins mean a clearer difference. Margins are in natural
log units of probability.

Usage, from the project folder:
    .venv/Scripts/python src/tools/word_pairs.py [--pairs file.tsv] [--carrier "I said {}."]
        [--save results.json] [--compare old.json] [rate ...]
"""

import json
import os
import sys
import tempfile
from collections import defaultdict
from pathlib import Path

# Repeatable arithmetic, as in intelligibility.py; set before the recognizer loads.
os.environ.setdefault("MKL_CBWR", "COMPATIBLE")

import numpy as np
from faster_whisper import WhisperModel
from faster_whisper.audio import pad_or_trim
from faster_whisper.tokenizer import Tokenizer

from intelligibility import MODEL, ROOT, load_audio, render

PAIRS = ROOT / "src" / "tools" / "word_pairs.tsv"
PER_RECORDING = 6
WORST = 12
CARRIER = "Say {} again."


def load_pairs(path):
    pairs = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.strip() and not line.startswith("#"):
            group, first, second = line.split("\t")
            pairs.append((group, first, second))
    return pairs


def carrier(words):
    return " ".join(CARRIER.format(word) for word in words)


class Scorer:
    def __init__(self):
        self.model = WhisperModel(MODEL, device="cpu", compute_type="int8", cpu_threads=8)
        self.tokenizer = Tokenizer(self.model.hf_tokenizer, self.model.model.is_multilingual, "transcribe", "en")

    def scores(self, audio, texts):
        """Log probability of each text as the transcription of the audio."""
        features = self.model.feature_extractor(audio)
        frames = features.shape[-1]
        encoded = self.model.encode(pad_or_trim(features))
        result = []
        for text in texts:
            tokens = self.tokenizer.encode(" " + text)
            aligned = self.model.model.align(encoded, self.tokenizer.sot_sequence, [tokens], frames)
            result.append(float(np.sum(np.log(aligned[0].text_token_probs))))
        return result


def recording_groups(pairs):
    """Pairs grouped, in order, up to six to a recording, so that no word of
    a pair appears with another pair in the same group."""
    groups = []
    for i, (_, first, second) in enumerate(pairs):
        for group in groups:
            words = {word for j in group for word in pairs[j][1:]}
            if len(group) < PER_RECORDING and first not in words and second not in words:
                group.append(i)
                break
        else:
            groups.append([i])
    return groups


def margins(scorer, pairs, rate):
    """Each pair's margin at a rate, in the order of `pairs`."""
    groups = recording_groups(pairs)
    advantage = [0.0] * len(pairs)
    with tempfile.TemporaryDirectory() as folder:
        listing = Path(folder) / "recordings.tsv"
        lines = []
        for n, group in enumerate(groups):
            for side in (1, 2):
                lines.append(f"{side}-{n:02}\t{carrier(pairs[i][side] for i in group)}")
        listing.write_text("\n".join(lines) + "\n", encoding="utf-8")
        render(listing, folder, rate)
        for n, group in enumerate(groups):
            for side in (1, 2):
                spoken = [pairs[i][side] for i in group]
                texts = [carrier(spoken)]
                for k, i in enumerate(group):
                    swapped = list(spoken)
                    swapped[k] = pairs[i][3 - side]
                    texts.append(carrier(swapped))
                audio, _ = load_audio(Path(folder) / f"{side}-{n:02}.wav")
                truth, *partners = scorer.scores(audio, texts)
                for i, partner in zip(group, partners):
                    advantage[i] += truth - partner
    return advantage


def report(pairs, rate, values, previous=None):
    print(f"\nRate {rate}:")
    by_group = defaultdict(list)
    for (group, _, _), value in zip(pairs, values):
        by_group[group].append(value)
    old_by_group = defaultdict(list)
    if previous:
        for (group, _, _), value in zip(pairs, previous):
            old_by_group[group].append(value)
    for group, group_values in [*by_group.items(), ("all", values)]:
        apart = sum(v > 0 for v in group_values) / len(group_values)
        line = f"  {group}: {apart:.0%} told apart, mean margin {np.mean(group_values):.1f}"
        if previous:
            old = previous if group == "all" else old_by_group[group]
            line += f" (was {sum(v > 0 for v in old) / len(old):.0%}, {np.mean(old):.1f})"
        print(line)
    order = sorted(range(len(pairs)), key=lambda i: values[i])
    print("  Hardest pairs: " + ", ".join(f"{pairs[i][1]}/{pairs[i][2]} {values[i]:.1f}" for i in order[:WORST]))


def main():
    global CARRIER
    args = sys.argv[1:]
    save = compare = None
    path = PAIRS
    while args[:1] in (["--save"], ["--compare"], ["--pairs"], ["--carrier"]):
        if args[0] == "--save":
            save = Path(args[1])
        elif args[0] == "--pairs":
            path = Path(args[1]).resolve()
        elif args[0] == "--carrier":
            CARRIER = args[1]
        else:
            compare = json.loads(Path(args[1]).read_text())
        args = args[2:]
    rates = [float(r) for r in args] or [1.0, 2.24]
    pairs = load_pairs(path)
    scorer = Scorer()
    results = {}
    for rate in rates:
        values = margins(scorer, pairs, rate)
        results[str(rate)] = values
        report(pairs, rate, values, compare.get(str(rate)) if compare else None)
    if save:
        save.write_text(json.dumps(results))


if __name__ == "__main__":
    main()
