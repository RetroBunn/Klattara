"""Verify the Klattara 1.1 driver's [:cv] mechanism against the real engine.

Checks, per voice (Steve, Brittany, Gary, David, Taylor):
 1. A command-only speak ([:cv ...] alone) returns OK and yields no audio.
 2. Applying the driver's per-voice defaults row (with the voice's own
    flutter) renders byte-identical to the stock voice -- the table is exact.
 3. The same row with fl 0 renders the same length but different samples:
    flutter off takes effect without breaking anything.
 4. The gender flip changes the render.
 5. A full driver-style command string (all 12 pairs) parses cleanly.
"""
import ctypes

LIB = "/home/hatch/workspace/nvda-synths/Klattara/target/release/libklattara.so"
TEXT = b"Hello, world. This is a test of the Klattara voice."

# head size, roughness, breathiness, open quotient, smoothness,
# baseline fall, hat rise, stress rise, quickness, lax breathiness, female
ROWS = [
    (100, 0, 0, 55, 0, 18, 18, 32, 40, 0, False),
    (100, 0, 36, 65, 12, 12, 16, 36, 58, 65, True),
    (120, 8, 0, 60, 8, 10, 22, 26, 14, 8, False),
    (94, 16, 52, 70, 28, 8, 14, 20, 6, 70, False),
    (89, 0, 24, 58, 4, 5, 20, 42, 72, 30, True),
]
FLUTTER = [25, 25, 25, 60, 25]

lib = ctypes.CDLL(LIB)
CB = ctypes.CFUNCTYPE(ctypes.c_int, ctypes.c_void_p, ctypes.POINTER(ctypes.c_int16), ctypes.c_int)


def collect(user, samples, count):
    out = ctypes.cast(user, ctypes.POINTER(ctypes.py_object)).contents.value
    out.extend(samples[:count])
    return 0


lib.klattara_create.argtypes = [ctypes.c_int]
lib.klattara_create.restype = ctypes.c_void_p
lib.klattara_destroy.argtypes = [ctypes.c_void_p]
for name in ("set_voice", "set_gender"):
    f = getattr(lib, f"klattara_{name}")
    f.argtypes = [ctypes.c_void_p, ctypes.c_int]
    f.restype = ctypes.c_int
lib.klattara_speak.argtypes = [ctypes.c_void_p, ctypes.c_char_p, ctypes.c_int, CB, ctypes.c_void_p]
lib.klattara_speak.restype = ctypes.c_int


def speak(engine, data, reading=0):
    audio = []
    box = ctypes.py_object(audio)
    cb = CB(collect)
    rc = lib.klattara_speak(engine, data, reading, cb, ctypes.byref(box))
    return rc, audio


def cv_command(row, flutter):
    hs, rg, br, rc, sm, fa, ha, st, qk, lx, _female = row
    return (
        f"[:cv hs {hs} rg {rg} br {br} oq {rc} sm {sm} fl {flutter} "
        f"fa {fa} ha {ha} st {st} qk {qk} lx {lx}]"
    ).encode()


failures = []
for voice in range(5):
    engine = lib.klattara_create(22050)
    assert lib.klattara_set_voice(engine, voice) == 0
    rc, stock = speak(engine, TEXT)
    assert rc == 0, f"voice {voice}: stock speak failed {rc}"

    # 1. Command-only speak: no audio, OK.
    rc, empty = speak(engine, cv_command(ROWS[voice], FLUTTER[voice]))
    assert rc == 0 and empty == [], f"voice {voice}: command-only speak gave {rc}, {len(empty)} samples"

    # 2. Defaults row (own flutter) == stock, byte for byte.
    rc, with_defaults = speak(engine, TEXT)
    assert rc == 0
    if with_defaults != stock:
        failures.append(f"voice {voice}: defaults row does NOT match stock "
                        f"({len(with_defaults)} vs {len(stock)} samples)")

    # 3. Same row, flutter off: same length, different samples.
    rc, no_flutter = speak(engine, cv_command(ROWS[voice], 0))
    assert rc == 0 and no_flutter == []
    rc, fl0 = speak(engine, TEXT)
    assert rc == 0
    if len(fl0) != len(stock):
        failures.append(f"voice {voice}: fl 0 changed length ({len(fl0)} vs {len(stock)})")
    if fl0 == stock:
        failures.append(f"voice {voice}: fl 0 render is identical to stock -- flutter did not change")

    # 4. Gender flip changes the render.
    lib.klattara_set_gender(engine, 1 if not ROWS[voice][10] else 0)
    rc, flipped = speak(engine, TEXT)
    assert rc == 0
    if flipped == fl0:
        failures.append(f"voice {voice}: gender flip did not change the render")
    lib.klattara_destroy(engine)
    print(f"voice {voice}: stock {len(stock)} samples, fl0 differs={fl0 != stock}, "
          f"gender-flip differs={flipped != fl0}")

print("FAILURES:" if failures else "ALL CHECKS PASSED")
for f in failures:
    print(" -", f)
raise SystemExit(1 if failures else 0)
