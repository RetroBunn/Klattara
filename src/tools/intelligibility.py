"""Automatic intelligibility check for Klattara.

Renders the test sentences at several speaking rates, transcribes them with
the Whisper speech recognizer (faster-whisper), and reports the word error
rate: the share of words the recognizer got wrong, missed or added.

A recognizer is only a rough stand-in for a human listener. It leans on
context to guess unclear words, so it can be more forgiving than a person on
predictable sentences. Treat the numbers as a way to compare versions and
rates, not as an absolute score.

Usage, from the project folder:
    .venv/Scripts/python src/tools/intelligibility.py [--sentences file.tsv] [rate ...]
"""

import os
import re
import subprocess
import sys
import tempfile
import wave
from math import gcd
from pathlib import Path

os.environ.setdefault("HF_HUB_DISABLE_SYMLINKS_WARNING", "1")
# Without this, Intel's math library takes slightly different arithmetic
# paths depending on what the process did earlier, so the same audio can be
# heard differently once other sentences have been transcribed, and a change
# to one sentence moves the scores of others. Fixed paths make each result
# depend on its own audio only.
os.environ.setdefault("MKL_CBWR", "COMPATIBLE")

import numpy as np
from faster_whisper import WhisperModel
from scipy.signal import resample_poly

ROOT = Path(__file__).resolve().parents[2]
SENTENCES = ROOT / "src" / "tools" / "sentences.tsv"
MODEL = "small.en"
DIGITS = "zero one two three four five six seven eight nine".split()


def load_sentences(path):
    items = []
    for line in path.read_text(encoding="utf-8").splitlines():
        if line.strip() and not line.startswith("#"):
            sentence_id, text = line.split("\t")[:2]
            items.append((sentence_id, text))
    return items


def words(text):
    """Lowercase words without punctuation; digits spelled out."""
    text = re.sub(r"\d", lambda m: f" {DIGITS[int(m.group())]} ", text.lower())
    text = text.replace("it's", "it is")
    return re.findall(r"[a-z']+", text)


def edit_distance(reference, hypothesis):
    """Word-level Levenshtein distance, and the reference words that went wrong."""
    rows, cols = len(reference) + 1, len(hypothesis) + 1
    cost = [[0] * cols for _ in range(rows)]
    for i in range(rows):
        cost[i][0] = i
    for j in range(cols):
        cost[0][j] = j
    for i in range(1, rows):
        for j in range(1, cols):
            same = reference[i - 1] == hypothesis[j - 1]
            cost[i][j] = min(cost[i - 1][j] + 1, cost[i][j - 1] + 1, cost[i - 1][j - 1] + (not same))
    # Trace back to find which reference words were not matched.
    missed, i, j = [], rows - 1, cols - 1
    while i > 0 or j > 0:
        if i > 0 and j > 0 and cost[i][j] == cost[i - 1][j - 1] + (reference[i - 1] != hypothesis[j - 1]):
            if reference[i - 1] != hypothesis[j - 1]:
                missed.append(reference[i - 1])
            i, j = i - 1, j - 1
        elif i > 0 and cost[i][j] == cost[i - 1][j] + 1:
            missed.append(reference[i - 1])
            i -= 1
        else:
            j -= 1
    return cost[-1][-1], missed[::-1]


def render(sentences, folder, rate):
    """Renders with the batch example, or with a saved copy of it named by the
    KLATTARA_BATCH environment variable, so experiments can run while the
    engine is being changed."""
    saved = os.environ.get("KLATTARA_BATCH")
    command = [saved] if saved else ["cargo", "run", "--release", "--quiet", "--example", "batch", "--"]
    subprocess.run([*command, str(sentences), str(folder), str(rate)], cwd=ROOT, check=True)


def load_audio(path):
    """16-bit mono WAV as float samples at Whisper's 16 kHz, and its length in seconds."""
    with wave.open(str(path)) as w:
        rate = w.getframerate()
        samples = np.frombuffer(w.readframes(w.getnframes()), dtype=np.int16).astype(np.float32) / 32768.0
    divisor = gcd(16000, rate)
    return resample_poly(samples, 16000 // divisor, rate // divisor).astype(np.float32), len(samples) / rate


def main():
    args = sys.argv[1:]
    path = SENTENCES
    if args[:1] == ["--sentences"]:
        path, args = Path(args[1]).resolve(), args[2:]
    rates = [float(r) for r in args] or [1.0, 1.5, 2.0, 2.5, 3.0]
    sentences = load_sentences(path)
    model = WhisperModel(MODEL, device="cpu", compute_type="int8")
    summary = []
    for rate in rates:
        with tempfile.TemporaryDirectory() as folder:
            render(path, folder, rate)
            errors = total = recognized = 0
            seconds = 0.0
            print(f"\nRate {rate}:")
            for sentence_id, text in sentences:
                audio, length = load_audio(Path(folder) / f"{sentence_id}.wav")
                seconds += length
                segments, _ = model.transcribe(
                    audio, language="en", beam_size=5, temperature=0.0, condition_on_previous_text=False
                )
                heard = " ".join(s.text for s in segments).strip()
                reference = words(text)
                distance, missed = edit_distance(reference, words(heard))
                errors += distance
                total += len(reference)
                recognized += len(reference) - len(missed)
                status = "ok" if distance == 0 else f"{distance} wrong ({', '.join(missed) or 'extra words'})"
                print(f"  {sentence_id}: {status}. Heard: {heard}")
            wpm = total / seconds * 60
            summary.append((rate, wpm, recognized / total, errors / total))
    print("\nSummary (words per minute include pauses; the error rate also counts words the recognizer added):")
    for rate, wpm, hits, wer in summary:
        print(f"  rate {rate}: about {wpm:.0f} words per minute, {hits:.0%} of words recognized, error rate {wer:.0%}")


if __name__ == "__main__":
    main()
