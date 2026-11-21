// KlattPad on the web: the same controls as KlattPad on Windows, speaking with
// Klattara compiled to WebAssembly (klattara.wasm, from src/plat/web).
"use strict";

const SAMPLE_RATES = [8000, 11025, 16000, 22050, 32000, 44100, 48000, 96000];
const DEFAULT_RATE = 22050;
const SAMPLE_TEXT =
  "Hello! I'm Klattara, a formant speech synthesizer, speaking in your browser.\n\n" +
  "[:vb] This is Breezy Brittany. [:vg] And this is Giant Gary. [:vd] Delicate David. [:vt] And Tiny Taylor.\n\n" +
  "[:vs][:speed 400] I can speak quickly, [:speed 120] or very slowly.\n\n" +
  "[:speed 200][:phone klattara enable] [D OW1<300,13> R EY1<300,15> M IY1<300,17> F AA1<300,18> S OW1<600,20>] [:phone klattara disable]\n";

const $ = (id) => document.getElementById(id);
const text = $("text"), voiceBox = $("voice"), rate = $("rate"), volume = $("volume"), sampleRate = $("sample-rate");
const playButton = $("play"), pauseButton = $("pause"), stopButton = $("stop"), wavButton = $("wav");
const status = $("status");

let module = null;
let worker = null;
let voices = [];
let context = null;
let sources = [];
let nextTime = 0;
let speaking = false;   // the worker is still making audio
let playing = false;    // something is playing or waiting to
let paused = false;
// The buttons stay enabled while speaking, so the keyboard focus never falls off
// one: each does nothing when there is nothing for it to do.

function store(key, value) {
  try { localStorage.setItem("klattpad." + key, value); } catch (e) { /* private browsing */ }
}
function stored(key) {
  try { return localStorage.getItem("klattpad." + key); } catch (e) { return null; }
}

function say(message) {
  status.textContent = message;
}

// The Rate slider is the NVDA add-on's: hundredths of normal over 0 to 100, 15 normal.
function rateAt(position) {
  return Math.round(position / 100 * (500 - 29) + 29) / 100;
}
function describeRate() {
  const r = rateAt(Number(rate.value));
  const words = r === 1 ? "normal" : r + " times normal";
  $("rate-value").textContent = words;
  rate.setAttribute("aria-valuetext", words);
}
function describeVolume() {
  const words = volume.value + " percent";
  $("volume-value").textContent = words;
  volume.setAttribute("aria-valuetext", words);
}

function startWorker() {
  worker = new Worker("worker.js");
  worker.onmessage = onWorkerMessage;
  worker.onerror = (event) => say("Klattara stopped with an error: " + (event.message || "unknown"));
  worker.postMessage({ type: "load", module });
}

function onWorkerMessage(event) {
  const message = event.data;
  if (message.type === "ready") {
    if (voices.length === 0) {
      voices = message.voices;
      voices.forEach((name, index) => voiceBox.add(new Option(name, String(index))));
      const v = stored("voice");
      if (v !== null && Number(v) < voices.length) voiceBox.value = v;
      for (const b of [playButton, pauseButton, stopButton, wavButton]) b.disabled = false;
      say("Ready. Press Play or F5.");
    }
  } else if (message.type === "audio") {
    schedule(message.samples);
  } else if (message.type === "done") {
    speaking = false;
    if (message.error) say(message.error);
    if (sources.length === 0) finished();
  } else if (message.type === "wav") {
    speaking = false;
    if (message.error) say(message.error);
    download(wavFile(message.samples, Number(sampleRate.value)), "KlattPad " + voices[Number(voiceBox.value)] + ".wav");
    say("Exported " + (message.samples.length / Number(sampleRate.value)).toFixed(1) + " seconds as WAV.");
  }
}

function schedule(samples) {
  if (!playing) return;
  const rateHz = Number(sampleRate.value);
  const buffer = context.createBuffer(1, samples.length, rateHz);
  buffer.copyToChannel(samples, 0);
  const source = context.createBufferSource();
  source.buffer = buffer;
  source.connect(context.destination);
  nextTime = Math.max(nextTime, context.currentTime + 0.03);
  source.start(nextTime);
  nextTime += buffer.duration;
  sources.push(source);
  source.onended = () => {
    sources = sources.filter((s) => s !== source);
    if (sources.length === 0 && !speaking && playing) finished();
  };
}

function finished() {
  playing = false;
  paused = false;
  pauseButton.textContent = "Pause";
  say("Done.");
}

function request(mode) {
  const rateHz = Number(sampleRate.value);
  return {
    type: "speak",
    mode,
    text: text.value,
    start: mode === "play" ? text.selectionStart : 0,
    end: mode === "play" ? text.selectionEnd : 0,
    voice: Number(voiceBox.value),
    rate: Number(rate.value),
    volume: Number(volume.value),
    sampleRate: rateHz,
  };
}

async function play() {
  if (!module || voices.length === 0) return;
  if (paused) return pause();
  stop(true);
  if (!context) context = new AudioContext();
  if (context.state === "suspended") await context.resume();
  playing = true;
  speaking = true;
  nextTime = 0;
  say("Playing " + voices[Number(voiceBox.value)] + ".");
  worker.postMessage(request("play"));
}

function pause() {
  if (!playing || !context) return;
  if (paused) {
    context.resume();
    paused = false;
    pauseButton.textContent = "Pause";
    say("Resumed.");
  } else {
    context.suspend();
    paused = true;
    pauseButton.textContent = "Resume";
    say("Paused. Press Resume, F6 or F5 to go on.");
  }
}

function stop(quiet) {
  const was = playing || speaking;
  for (const s of sources) {
    s.onended = null;
    try { s.stop(); } catch (e) { /* not started */ }
  }
  sources = [];
  if (speaking) {
    // The engine runs to the end of what it was given, so a new worker takes over.
    worker.terminate();
    startWorker();
  }
  speaking = false;
  playing = false;
  if (paused && context) context.resume();
  paused = false;
  pauseButton.textContent = "Pause";
  if (was && !quiet) say("Stopped.");
}

function exportWav() {
  if (!module || voices.length === 0 || (speaking && !playing)) return;
  stop(true);
  speaking = true;
  say("Making the WAV file…");
  worker.postMessage(request("wav"));
}

// 16-bit mono PCM, as KlattPad on Windows writes.
function wavFile(samples, rateHz) {
  const bytes = new ArrayBuffer(44 + samples.length * 2);
  const view = new DataView(bytes);
  const ascii = (at, s) => { for (let i = 0; i < s.length; i++) view.setUint8(at + i, s.charCodeAt(i)); };
  ascii(0, "RIFF"); view.setUint32(4, 36 + samples.length * 2, true); ascii(8, "WAVE");
  ascii(12, "fmt "); view.setUint32(16, 16, true); view.setUint16(20, 1, true); view.setUint16(22, 1, true);
  view.setUint32(24, rateHz, true); view.setUint32(28, rateHz * 2, true); view.setUint16(32, 2, true); view.setUint16(34, 16, true);
  ascii(36, "data"); view.setUint32(40, samples.length * 2, true);
  for (let i = 0; i < samples.length; i++) {
    const s = Math.max(-1, Math.min(1, samples[i]));
    view.setInt16(44 + i * 2, Math.round(s * 32767), true);
  }
  return new Blob([bytes], { type: "audio/wav" });
}

function download(blob, name) {
  const link = document.createElement("a");
  link.href = URL.createObjectURL(blob);
  link.download = name;
  document.body.appendChild(link);
  link.click();
  link.remove();
  setTimeout(() => URL.revokeObjectURL(link.href), 10000);
}

// ---- Setting up ---------------------------------------------------------------

for (const r of SAMPLE_RATES) sampleRate.add(new Option(r + " hertz", String(r)));
sampleRate.value = String(SAMPLE_RATES.includes(Number(stored("sampleRate"))) ? Number(stored("sampleRate")) : DEFAULT_RATE);
for (const [box, key] of [[rate, "rate"], [volume, "volume"]]) {
  const v = stored(key);
  if (v !== null) box.value = v;
}
text.value = stored("text") ?? SAMPLE_TEXT;
describeRate();
describeVolume();

rate.addEventListener("input", () => { describeRate(); store("rate", rate.value); });
volume.addEventListener("input", () => { describeVolume(); store("volume", volume.value); });
voiceBox.addEventListener("change", () => store("voice", voiceBox.value));
sampleRate.addEventListener("change", () => store("sampleRate", sampleRate.value));
let saveTimer = 0;
text.addEventListener("input", () => {
  clearTimeout(saveTimer);
  saveTimer = setTimeout(() => store("text", text.value), 500);
});
// Ctrl+Tab types a tab, as in KlattPad; Tab alone moves on.
text.addEventListener("keydown", (event) => {
  if (event.key === "Tab" && event.ctrlKey) {
    event.preventDefault();
    text.setRangeText("\t", text.selectionStart, text.selectionEnd, "end");
  }
});

playButton.addEventListener("click", play);
pauseButton.addEventListener("click", pause);
stopButton.addEventListener("click", () => stop(false));
wavButton.addEventListener("click", exportWav);
$("open").addEventListener("click", () => $("file").click());
$("file").addEventListener("change", async () => {
  const file = $("file").files[0];
  if (!file) return;
  text.value = await file.text();
  store("text", text.value);
  $("file").value = "";
  say("Opened " + file.name + ".");
  text.focus();
});
$("save").addEventListener("click", () => {
  download(new Blob([text.value], { type: "text/plain;charset=utf-8" }), "KlattPad.txt");
  say("Saved KlattPad.txt to your downloads.");
});

document.addEventListener("keydown", (event) => {
  if (event.altKey || event.ctrlKey || event.metaKey) return;
  if (event.key === "F5") { event.preventDefault(); play(); }
  else if (event.key === "F6") { event.preventDefault(); pause(); }
  else if (event.key === "F7") { event.preventDefault(); stop(false); }
});

(async () => {
  try {
    module = await WebAssembly.compileStreaming(fetch("klattara.wasm"));
  } catch (e) {
    try {
      const bytes = await (await fetch("klattara.wasm")).arrayBuffer();
      module = await WebAssembly.compile(bytes);
    } catch (e2) {
      say("Could not load Klattara: " + e2.message);
      return;
    }
  }
  startWorker();
})();
