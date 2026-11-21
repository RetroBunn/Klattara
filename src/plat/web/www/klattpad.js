// KlattPad on the web, with every setting the Mac app has: the voice, rate in
// words per minute, base pitch in hertz, inflection, flutter (off unless turned
// up), volume, abbreviations, phrase prediction, the 20 sampling rates, and each
// voice's own [:cv] settings. Klattara compiled to WebAssembly (klattara.wasm,
// from src/plat/web) speaks; the settings reach it as its own commands.
"use strict";

const SAMPLE_RATES = [8000, 10000, 11000, 11025, 12000, 16000, 20000, 22000, 22050, 24000,
  32000, 40000, 44000, 44100, 48000, 64000, 80000, 88000, 88200, 96000];
const DEFAULT_RATE = 22050;

// Klattara's [:cv] settings, as the Mac and Android apps offer them per voice.
const GROUPS = [
  ["Voice quality", [
    ["ge", "Gender", "The model of the vocal tract, female or male. It changes nothing else, so set the pitch too.", 0, 1, 1, ""],
    ["hs", "Head size", "Percent of normal. Smaller sounds younger and higher, larger deeper.", 64, 135, 1, "percent"],
    ["rg", "Roughness", "From none to the most: unevenness from one pulse of the voice to the next.", 0, 100, 1, ""],
    ["br", "Breathiness", "Breath running through the voice, in decibels.", 0, 72, 1, "decibels"],
    ["lx", "Lax breathiness", "Breath at the ends of sentences, percent.", 0, 100, 1, "percent"],
    ["rc", "Richness", "Percent. More is thinner and buzzier, less fuller. Sets the open quotient, from the other end.", 0, 100, 1, "percent"],
    ["oq", "Open quotient", "Percent of each period the vocal folds are open. Sets the same thing as richness.", 20, 90, 1, "percent"],
    ["sm", "Smoothness", "Percent. More softens the voice, but not the consonants.", 0, 100, 1, "percent"],
  ]],
  ["Source levels", [
    ["vvl", "Voicing level", "Percent of normal: vowels and everything the vocal folds sound. 0 is off.", 0, 200, 5, "percent"],
    ["fvl", "Frication level", "Percent of normal: the hiss of S and F, and the bursts of stops.", 0, 200, 5, "percent"],
    ["avl", "Aspiration level", "Percent of normal: H, and the puff after P, T and K.", 0, 200, 5, "percent"],
    ["nvl", "Nasal level", "Percent of normal: M, N and NG.", 0, 200, 5, "percent"],
  ]],
  ["Melody", [
    ["fa", "Baseline fall", "How far the pitch falls through a phrase, 0 to 90. 18 is normal.", 0, 90, 1, ""],
    ["ha", "Hat rise", "The slow rise over a phrase, 0 to 100. 18 is normal.", 0, 100, 1, ""],
    ["st", "Stress rise", "The size of each accent, 0 to 100. 32 is normal.", 0, 100, 1, ""],
    ["qk", "Quickness", "How quickly the pitch moves, percent. 40 is normal.", 0, 100, 1, "percent"],
  ]],
  ["Upper formants", [
    ["f4", "F4", "The fourth formant, in hertz. Automatic follows head size and gender.", 2000, 6000, 50, "hertz"],
    ["b4", "F4 bandwidth", "In hertz.", 100, 4000, 10, "hertz"],
    ["f5", "F5", "The fifth formant, in hertz.", 2500, 6000, 50, "hertz"],
    ["b5", "F5 bandwidth", "In hertz.", 100, 4000, 10, "hertz"],
    ["f6", "F6", "The sixth formant, in hertz.", 3000, 9000, 50, "hertz"],
    ["b6", "F6 bandwidth", "In hertz.", 100, 4000, 10, "hertz"],
  ]],
];
// Richness and open quotient set the same thing: setting one clears the other.
const PARTNER = { rc: "oq", oq: "rc" };

const SAMPLE_TEXT =
  "Hello! I'm Klattara, a formant speech synthesizer, speaking in your browser.\n\n" +
  "[:vb] This is Breezy Brittany. [:vg] And this is Giant Gary. [:vd] Delicate David. [:vt] And Tiny Taylor.\n\n" +
  "[:vs][:speed 400] I can speak quickly, [:speed 120] or very slowly.\n\n" +
  "[:speed 200][:phone klattara enable] [D OW1<300,13> R EY1<300,15> M IY1<300,17> F AA1<300,18> S OW1<600,20>] [:phone klattara disable]\n";

const $ = (id) => document.getElementById(id);
const text = $("text"), voiceBox = $("voice"), sampleRate = $("sample-rate"), status = $("status");
const playButton = $("play"), pauseButton = $("pause"), stopButton = $("stop"), wavButton = $("wav");
const flutter = $("flutter"), exactPitch = $("exact-pitch");

let module = null, worker = null, voices = [], own = [];
let context = null, sources = [], nextTime = 0;
let speaking = false, playing = false, paused = false;
// The buttons stay enabled while speaking, so the keyboard focus never falls off
// one: each does nothing when there is nothing for it to do.

function store(key, value) {
  try { localStorage.setItem("klattpad." + key, value); } catch (e) { /* private browsing */ }
}
function stored(key) {
  try { return localStorage.getItem("klattpad." + key); } catch (e) { return null; }
}
function say(message) { status.textContent = message; }

function shown(v) {
  const t = Math.round(v * 10) / 10;
  return Number.isInteger(t) ? String(t) : t.toFixed(1);
}

// A typed number past either end goes to the end, and the page says so.
function clampNumber(input, name) {
  const min = Number(input.min), max = Number(input.max);
  let v = Number(input.value);
  if (input.value === "" || !Number.isFinite(v)) v = Number(input.defaultValue);
  const c = Math.min(max, Math.max(min, v));
  if (c !== v) say(name + " " + c + ", the " + (v < c ? "lowest" : "highest") + " allowed.");
  input.value = String(c);
  return c;
}

// ---- Each voice's own settings, and the user's changes -------------------------

function voiceKey() { return "voice." + voiceBox.value + "."; }
function custom(code) {
  const v = stored(voiceKey() + code);
  return v === null || v === "" ? null : Number(v);
}
function setCustom(code, value) {
  try {
    if (value === null) localStorage.removeItem("klattpad." + voiceKey() + code);
    else localStorage.setItem("klattpad." + voiceKey() + code, String(value));
  } catch (e) { /* private browsing */ }
}
function ownValue(code) {
  const v = own[Number(voiceBox.value)] && own[Number(voiceBox.value)][code];
  return v === null || v === undefined ? null : v;
}
function describe(code, v, unit) {
  if (v === null) return "automatic";
  if (code === "ge") return v >= 0.5 ? "male" : "female";
  return shown(v) + (unit ? " " + unit : "");
}

function buildVoiceSettings() {
  const box = $("quality");
  box.textContent = "";
  $("voice-name").textContent = voices[Number(voiceBox.value)] || "";
  for (const [group, params] of GROUPS) {
    const div = document.createElement("div");
    div.className = "group";
    const h = document.createElement("h3");
    h.textContent = group;
    const grid = document.createElement("div");
    grid.className = "quality-grid";
    div.append(h, grid);
    for (const [code, name, help, min, max, step, unit] of params) {
      const id = "q-" + code;
      const cell = document.createElement("div");
      const label = document.createElement("label");
      label.htmlFor = id;
      label.textContent = name;
      const range = document.createElement("span");
      range.className = "value";
      range.textContent = " (" + min + " to " + max + (unit ? " " + unit : "") + ")";
      label.appendChild(range);
      const row = document.createElement("div");
      row.className = "row";
      let input;
      if (code === "ge") {
        input = document.createElement("select");
        input.add(new Option("Female", "0"));
        input.add(new Option("Male", "1"));
        const v = custom(code) ?? ownValue(code) ?? 1;
        input.value = v >= 0.5 ? "1" : "0";
        input.addEventListener("change", () => setCustom(code, Number(input.value)));
      } else {
        input = document.createElement("input");
        Object.assign(input, { type: "number", min, max, step });
        const ownV = ownValue(code);
        input.placeholder = ownV === null ? "Automatic" : shown(ownV);
        const c = custom(code);
        input.value = c !== null ? shown(c) : (ownV === null ? "" : shown(ownV));
        input.addEventListener("change", () => {
          if (input.value === "") { setCustom(code, null); return; }
          const v = clampNumber(input, name);
          if (v === ownV) { setCustom(code, null); return; }
          setCustom(code, v);
          if (PARTNER[code]) { setCustom(PARTNER[code], null); buildVoiceSettings(); }
        });
      }
      input.id = id;
      const reset = document.createElement("button");
      reset.textContent = "Reset";
      reset.setAttribute("aria-label", "Reset " + name);
      reset.addEventListener("click", () => {
        setCustom(code, null);
        buildVoiceSettings();
        say(name + ", " + voices[Number(voiceBox.value)] + "'s own, " + describe(code, ownValue(code), unit) + ".");
        $("q-" + code).focus();
      });
      const helpText = document.createElement("p");
      helpText.className = "help";
      helpText.id = id + "-help";
      helpText.textContent = help + " " + voices[Number(voiceBox.value)] + "'s own: " + describe(code, ownValue(code), unit) + ".";
      input.setAttribute("aria-describedby", helpText.id);
      row.append(input, reset);
      cell.append(label, row, helpText);
      grid.appendChild(cell);
    }
    box.appendChild(div);
  }
}

/** The page's settings as Klattara commands, sent after the voice is chosen. */
function commands() {
  const wpm = clampNumber($("wpm"), "Rate");
  const cv = ["fl " + flutter.value, "in " + clampNumber($("inflection"), "Inflection")];
  if (exactPitch.checked) cv.push("pi " + clampNumber($("pitch"), "Pitch"));
  for (const [, params] of GROUPS) {
    for (const [code] of params) {
      const v = custom(code);
      if (v !== null) cv.push(code + " " + v);
    }
  }
  return "[:speed " + wpm + "][:volume " + clampNumber($("volume"), "Volume") + "][:cv " + cv.join(" ") + "]";
}

// ---- Speaking ------------------------------------------------------------------

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
      own = message.own;
      voices.forEach((name, index) => voiceBox.add(new Option(name, String(index))));
      const v = stored("voice");
      if (v !== null && Number(v) < voices.length) voiceBox.value = v;
      if (stored("pitch") === null) $("pitch").value = String(Math.round(own[Number(voiceBox.value)].pi || 110));
      buildVoiceSettings();
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
    const rate = Number(sampleRate.value);
    download(wavFile(message.samples, rate), "KlattPad " + voices[Number(voiceBox.value)] + " " + rate + " Hz.wav");
    say("Exported " + (message.samples.length / rate).toFixed(1) + " seconds as WAV.");
  }
}

function schedule(samples) {
  if (!playing) return;
  const buffer = context.createBuffer(1, samples.length, Number(sampleRate.value));
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
  return {
    type: "speak",
    mode,
    text: text.value,
    start: mode === "play" ? text.selectionStart : 0,
    end: mode === "play" ? text.selectionEnd : 0,
    voice: Number(voiceBox.value),
    commands: commands(),
    abbreviations: $("abbreviations").checked,
    phrasePrediction: $("phrase").checked,
    sampleRate: Number(sampleRate.value),
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
    view.setInt16(44 + i * 2, Math.round(Math.max(-1, Math.min(1, samples[i])) * 32767), true);
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

for (const r of SAMPLE_RATES) sampleRate.add(new Option(r + " hertz" + (r === DEFAULT_RATE ? " (recommended)" : ""), String(r)));
sampleRate.value = String(SAMPLE_RATES.includes(Number(stored("sampleRate"))) ? Number(stored("sampleRate")) : DEFAULT_RATE);
sampleRate.addEventListener("change", () => store("sampleRate", sampleRate.value));

for (const [id, name] of [["wpm", "Rate"], ["pitch", "Pitch"], ["inflection", "Inflection"], ["volume", "Volume"]]) {
  const input = $(id);
  const v = stored(id);
  if (v !== null) input.value = v;
  input.addEventListener("change", () => store(id, String(clampNumber(input, name))));
}
for (const id of ["abbreviations", "phrase", "exact-pitch"]) {
  const box = $(id);
  const v = stored(id);
  if (v !== null) box.checked = v === "1";
  box.addEventListener("change", () => store(id, box.checked ? "1" : "0"));
}
$("pitch").disabled = !exactPitch.checked;
exactPitch.addEventListener("change", () => { $("pitch").disabled = !exactPitch.checked; });

function describeFlutter() {
  const v = Number(flutter.value);
  const words = v === 0 ? "off" : v + " percent";
  $("flutter-value").textContent = words;
  flutter.setAttribute("aria-valuetext", words);
}
if (stored("flutter") !== null) flutter.value = stored("flutter");
describeFlutter();
flutter.addEventListener("input", () => { describeFlutter(); store("flutter", flutter.value); });

voiceBox.addEventListener("change", () => {
  store("voice", voiceBox.value);
  if (!exactPitch.checked) $("pitch").value = String(Math.round(own[Number(voiceBox.value)].pi || 110));
  buildVoiceSettings();
});

text.value = stored("text") ?? SAMPLE_TEXT;
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
$("reset").addEventListener("click", () => {
  for (const [, params] of GROUPS) for (const [code] of params) setCustom(code, null);
  buildVoiceSettings();
  say(voices[Number(voiceBox.value)] + " is back to its own settings.");
});
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
      module = await WebAssembly.compile(await (await fetch("klattara.wasm")).arrayBuffer());
    } catch (e2) {
      say("Could not load Klattara: " + e2.message);
      return;
    }
  }
  startWorker();
})();
