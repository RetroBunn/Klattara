# KlattPad on the web

KlattPad in a browser: the same Voice, Rate and Volume controls as KlattPad on Windows, Play (F5),
Pause (F6) and Stop (F7), a sampling rate choice from 8000 to 96000 hertz, Export WAV, and opening and
saving text. Commands, phoneme input and singing work as they do in KlattPad. It runs entirely in the
browser: the engine is compiled to WebAssembly and speaks in a Web Worker, so nothing is sent anywhere and
the page stays responsive. Your text and settings are remembered in the browser.

- `src/lib.rs`: the engine's WebAssembly interface, with no bindings library. The page writes text into
  memory from `klattara_alloc` and gets audio back, piece by piece, through `env.klattara_audio`.
- `www/`: the whole site, `index.html`, `klattpad.js` and `worker.js`, plus `klattara.wasm` once built.

```
rustup target add wasm32-unknown-unknown
./build-web.sh
python3 -m http.server -d www 8000     # then open http://localhost:8000
```

Any static web server works, GitHub Pages included. A page opened straight from the disk (`file://`) can't
load the engine; browsers only allow that from a server.
