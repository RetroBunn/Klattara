// Speaks in the background for KlattPad on the web. The page sends the compiled
// engine once, then one request at a time; audio goes back in pieces as it is
// made. To stop, the page ends this worker and starts another.
"use strict";

let engine = null;
let pending = [];
let pendingLength = 0;
let mode = "play";
let collected = [];
// Pieces of about a tenth of a second, so the page schedules few of them.
let minimum = 2205;

function flush() {
  if (pendingLength === 0) return;
  const samples = new Float32Array(pendingLength);
  let at = 0;
  for (const piece of pending) {
    samples.set(piece, at);
    at += piece.length;
  }
  pending = [];
  pendingLength = 0;
  if (mode === "play") {
    postMessage({ type: "audio", samples }, [samples.buffer]);
  } else {
    collected.push(samples);
  }
}

self.onmessage = async (event) => {
  const message = event.data;
  if (message.type === "load") {
    const instance = await WebAssembly.instantiate(message.module, {
      env: {
        klattara_audio(pointer, count) {
          const piece = new Float32Array(engine.memory.buffer, pointer, count).slice();
          pending.push(piece);
          pendingLength += piece.length;
          if (pendingLength >= minimum) flush();
          return 1;
        },
      },
    });
    engine = instance.exports;
    const count = engine.klattara_voices();
    postMessage({ type: "ready", voices: readMessage().split("\n").slice(0, count) });
    return;
  }
  if (message.type === "speak") {
    mode = message.mode;
    collected = [];
    minimum = Math.round(message.sampleRate / 10);
    const bytes = new TextEncoder().encode(message.text);
    const pointer = engine.klattara_alloc(bytes.length);
    new Uint8Array(engine.memory.buffer, pointer, bytes.length).set(bytes);
    const result = engine.klattara_speak(
      pointer, bytes.length, message.start, message.end,
      message.voice, message.rate, message.volume, message.sampleRate);
    engine.klattara_free(pointer, bytes.length);
    flush();
    const error = result < 0 ? readMessage() : "";
    if (mode === "wav") {
      let total = 0;
      for (const piece of collected) total += piece.length;
      const samples = new Float32Array(total);
      let at = 0;
      for (const piece of collected) {
        samples.set(piece, at);
        at += piece.length;
      }
      collected = [];
      postMessage({ type: "wav", samples, error }, [samples.buffer]);
    } else {
      postMessage({ type: "done", error });
    }
  }
};

function readMessage() {
  const pointer = engine.klattara_message();
  const length = engine.klattara_message_length();
  return new TextDecoder().decode(new Uint8Array(engine.memory.buffer, pointer, length));
}
