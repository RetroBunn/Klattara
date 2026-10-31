//! Playing speech through Windows' sound output, on a thread of its own.
//!
//! Speech is made in chunks as it is needed, so it starts at once and memory stays
//! small however long the text is. Only a second or so of sound is ever queued, and
//! Windows can stop or pause that immediately.

use crate::speech::SAMPLE_RATE;
use klattara::{Reading, Voice, speak_with};
use std::mem::size_of;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr::{addr_of, null, null_mut, read_volatile};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE};
use windows_sys::Win32::Media::Audio::{
    CALLBACK_EVENT, HWAVEOUT, WAVE_FORMAT_PCM, WAVE_MAPPER, WAVEFORMATEX, WAVEHDR, WHDR_DONE, waveOutClose,
    waveOutOpen, waveOutPause, waveOutPrepareHeader, waveOutReset, waveOutRestart, waveOutUnprepareHeader,
    waveOutWrite,
};
use windows_sys::Win32::System::Threading::{CreateEventW, WaitForSingleObject};

/// How many buffers are queued at most, and how many samples each holds: about a
/// sixth of a second each.
const BUFFERS: usize = 8;
const BUFFER_SAMPLES: usize = 4096;
/// How often a waiting thread looks to see if it should stop, ms. Stopping wakes it
/// at once anyway, by finishing the buffers.
const WAIT_MS: u32 = 50;

/// Windows' handles, which may be used from any thread.
#[derive(Clone, Copy)]
struct Output {
    device: HWAVEOUT,
    event: HANDLE,
}

// SAFETY: the sound output functions and events may be called from any thread.
unsafe impl Send for Output {}

/// One buffer of sound. Its address is given to Windows.
struct Slot {
    header: WAVEHDR,
    samples: Vec<i16>,
    /// Whether Windows has it, or has finished with it and it is still prepared.
    in_use: bool,
}

impl Slot {
    fn finished(&self) -> bool {
        // SAFETY: Windows changes this field from another thread while it has the
        // buffer, so it must be read afresh every time.
        !self.in_use || unsafe { read_volatile(addr_of!(self.header.dwFlags)) } & WHDR_DONE != 0
    }
}

/// Queues sound, waiting for room when the queue is full.
struct Queue {
    output: Output,
    /// These never move, because this list never changes size.
    slots: Vec<Slot>,
    stop: Arc<AtomicBool>,
}

impl Queue {
    fn new(output: Output, stop: Arc<AtomicBool>) -> Queue {
        let slot = || Slot { header: unsafe { std::mem::zeroed() }, samples: Vec::new(), in_use: false };
        Queue { output, slots: (0..BUFFERS).map(|_| slot()).collect(), stop }
    }

    fn stopped(&self) -> bool {
        self.stop.load(Ordering::Relaxed)
    }

    /// Gives Windows back a buffer's preparation once it has finished with it.
    fn release(&mut self, index: usize) {
        let device = self.output.device;
        let slot = &mut self.slots[index];
        if slot.in_use {
            // SAFETY: the header was prepared on this device and is finished with.
            unsafe { waveOutUnprepareHeader(device, &mut slot.header, size_of::<WAVEHDR>() as u32) };
            slot.in_use = false;
        }
    }

    /// Queues samples. Returns false if playing was stopped.
    fn write(&mut self, samples: &[i16]) -> bool {
        let index = loop {
            if self.stopped() {
                return false;
            }
            match self.slots.iter().position(|slot| slot.finished()) {
                Some(index) => break index,
                // SAFETY: the event lives until this thread has finished.
                None => unsafe {
                    WaitForSingleObject(self.output.event, WAIT_MS);
                },
            }
        };
        self.release(index);
        let device = self.output.device;
        let slot = &mut self.slots[index];
        slot.samples.clear();
        slot.samples.extend_from_slice(samples);
        // SAFETY: the zeroed header is valid, and it and the samples stay in place
        // and untouched until Windows marks the buffer done.
        unsafe {
            slot.header = std::mem::zeroed();
            slot.header.lpData = slot.samples.as_mut_ptr().cast();
            slot.header.dwBufferLength = (slot.samples.len() * 2) as u32;
            waveOutPrepareHeader(device, &mut slot.header, size_of::<WAVEHDR>() as u32);
            slot.in_use = true;
            waveOutWrite(device, &mut slot.header, size_of::<WAVEHDR>() as u32);
        }
        true
    }

    /// Waits until everything queued has played, unless playing is stopped.
    fn finish(&mut self) {
        while !self.stopped() && !self.slots.iter().all(|slot| slot.finished()) {
            // SAFETY: the event lives until this thread has finished.
            unsafe { WaitForSingleObject(self.output.event, WAIT_MS) };
        }
        // SAFETY: resetting returns any buffers still queued, so all can be released.
        unsafe { waveOutReset(self.output.device) };
        for index in 0..BUFFERS {
            self.release(index);
        }
    }
}

/// Makes the speech and queues it.
fn play(text: &str, voice: &mut Voice, queue: &mut Queue) {
    let mut pending: Vec<i16> = Vec::with_capacity(2 * BUFFER_SAMPLES);
    let spoken = speak_with(text, Reading::Text, voice, SAMPLE_RATE, &mut |chunk| {
        pending.extend(chunk.iter().map(|&x| (x.clamp(-1.0, 1.0) * 32767.0).round() as i16));
        let mut written = 0;
        while pending.len() - written >= BUFFER_SAMPLES {
            if !queue.write(&pending[written..written + BUFFER_SAMPLES]) {
                return false;
            }
            written += BUFFER_SAMPLES;
        }
        pending.drain(..written);
        true
    });
    if spoken == Ok(true) && !pending.is_empty() {
        queue.write(&pending);
    }
}

/// Speech playing now. Dropping this stops it at once.
pub struct Player {
    output: Output,
    stop: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
}

impl Player {
    /// Starts speaking. `on_finished` is called on the player's thread when the
    /// speech has ended, by finishing or by being stopped.
    pub fn start(
        text: String,
        mut voice: Voice,
        on_finished: impl FnOnce() + Send + 'static,
    ) -> Result<Player, String> {
        let format = WAVEFORMATEX {
            wFormatTag: WAVE_FORMAT_PCM as u16,
            nChannels: 1,
            nSamplesPerSec: SAMPLE_RATE,
            nAvgBytesPerSec: SAMPLE_RATE * 2,
            nBlockAlign: 2,
            wBitsPerSample: 16,
            cbSize: 0,
        };
        // SAFETY: the pointers are valid for the calls, and the event outlives the device.
        let output = unsafe {
            let event = CreateEventW(null(), 0, 0, null());
            if event.is_null() {
                return Err("Windows could not create an event for playing sound.".to_string());
            }
            let mut device: HWAVEOUT = null_mut();
            let opened = waveOutOpen(&mut device, WAVE_MAPPER, &format, event as usize, 0, CALLBACK_EVENT);
            if opened != 0 {
                CloseHandle(event);
                return Err(format!("Windows could not open the sound output (error {opened})."));
            }
            Output { device, event }
        };
        let stop = Arc::new(AtomicBool::new(false));
        let (thread_output, thread_stop) = (output, stop.clone());
        let worker = std::thread::spawn(move || {
            let output = thread_output;
            let mut queue = Queue::new(output, thread_stop);
            // Whatever goes wrong in making the speech, the sound output still has
            // to be cleaned up and the window told.
            let _ = catch_unwind(AssertUnwindSafe(|| play(&text, &mut voice, &mut queue)));
            queue.finish();
            on_finished();
        });
        Ok(Player { output, stop, worker: Some(worker) })
    }

    pub fn pause(&self) {
        // SAFETY: the device stays open until this player is dropped.
        unsafe { waveOutPause(self.output.device) };
    }

    pub fn resume(&self) {
        // SAFETY: the device stays open until this player is dropped.
        unsafe { waveOutRestart(self.output.device) };
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // SAFETY: the device is open until the close below, and the thread has
        // released every buffer by the time it has been joined.
        unsafe {
            waveOutReset(self.output.device);
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
            waveOutClose(self.output.device);
            CloseHandle(self.output.event);
        }
    }
}
