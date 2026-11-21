# Issue: Widen the speaking-rate range to 30–1920 words per minute

File at: https://github.com/RetroBunn/Klattara/issues/new

## Title

Widen the speaking-rate range: 30 to 1920 words per minute

## Body

The speaking rate currently runs from 38 to 1796 words per minute. Those
endpoints are the ends of the measured `WORDS_PER_MINUTE` table in
`src/engine/src/speech.rs` — 38 WPM is rate 0.25, 1796 WPM is rate 8.0 —
and `timeline` clamps the rate to 0.25–8.0, so nothing outside that range
is reachable, including through `[:speed]` (which is bounded by
`SLOWEST_WPM` / `FASTEST_WPM`).

38 and 1796 are odd measured values to end on. A rounder, slightly wider
range would be more useful and more balanced: 30 words per minute at the
slow end, and about 1920 at the fast end. 30 would not be a comfortable
listening speed, but it is the slowest rate the synthesizer can represent,
and 1920 is the fastest rate it can represent — so the range spans the
full capability of the engine in both directions, instead of stopping at
arbitrary measured points. 190 stays the default normal rate.

What extending it would take, from the code:

- New table endpoints, extrapolating the end segments: 30 WPM is about
  rate 0.21 (the 38→87 segment runs 0.25→0.5), and 1920 WPM is about
  rate 8.5 (the 1326→1796 segment runs 6.0→8.0).
- The `timeline` rate clamp (0.25–8.0) would need to widen to match, or
  the new endpoints would clamp back to the old range.
- `SLOWEST_WPM` / `FASTEST_WPM` and the `[:speed]` / `[:spd]` range
  follow from the table, so they move automatically.
- The actual words per minute at the new endpoints would need measuring,
  since the table is measured on real text rather than derived — and the
  timing model at 0.21x and 8.5x is worth a check, since pauses shrink
  faster than words do and segment compression may behave differently
  outside the measured range.

Happy to test the new endpoints by ear once they exist.
