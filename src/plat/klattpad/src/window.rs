//! The KlattPad window: its controls, its menus and its keyboard commands.
//!
//! The controls are ordinary Windows controls, each with a label just before it, so
//! a screen reader can name them. They are never disabled, because the focus would
//! be lost from a button that was. A button with nothing to do does nothing.

use crate::document;
use crate::player::Player;
use crate::speech::{self, FULL_VOLUME_POSITION, NORMAL_RATE_POSITION};
use klattara::VOICES;
use std::cell::{Cell, RefCell};
use std::mem::{size_of, zeroed};
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use windows_sys::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    COLOR_BTNFACE, CreateFontIndirectW, GetDC, GetDeviceCaps, LOGPIXELSY, ReleaseDC,
};
use windows_sys::Win32::System::DataExchange::IsClipboardFormatAvailable;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::Ole::CF_UNICODETEXT;
use windows_sys::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, GetSaveFileNameW, OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_HIDEREADONLY, OFN_NOCHANGEDIR,
    OFN_OVERWRITEPROMPT, OFN_PATHMUSTEXIST, OPENFILENAMEW,
};
use windows_sys::Win32::UI::Controls::{
    EM_CANUNDO, EM_GETMODIFY, EM_GETSEL, EM_SETLIMITTEXT, EM_SETMODIFY, EM_SETSEL, ICC_BAR_CLASSES,
    ICC_STANDARD_CLASSES, INITCOMMONCONTROLSEX, InitCommonControlsEx, TBM_SETLINESIZE, TBM_SETPAGESIZE, TBM_SETPOS,
    TBM_SETRANGE, TBS_NOTICKS,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetFocus, GetKeyState, SetFocus, VK_CONTROL, VK_F5, VK_F6, VK_F7, VK_TAB,
};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

/// Asks a slider for its position. This one is missing from the bindings.
const TBM_GETPOS: u32 = WM_USER;

const CLASS: &str = "KlattPad";
/// The window's size before the screen's scaling, including its frame.
const WINDOW_SIZE: (i32, i32) = (860, 580);
const SMALLEST_SIZE: (i32, i32) = (560, 400);

const ID_NEW: u16 = 100;
const ID_OPEN: u16 = 101;
const ID_SAVE: u16 = 102;
const ID_SAVE_AS: u16 = 103;
const ID_EXPORT: u16 = 104;
const ID_EXIT: u16 = 105;
const ID_UNDO: u16 = 200;
const ID_CUT: u16 = 201;
const ID_COPY: u16 = 202;
const ID_PASTE: u16 = 203;
const ID_DELETE: u16 = 204;
const ID_SELECT_ALL: u16 = 205;
const ID_PLAY: u16 = 300;
const ID_PAUSE: u16 = 301;
const ID_STOP: u16 = 302;
/// The Voice submenu takes one number for each voice, from this one.
const ID_FIRST_VOICE: u16 = 400;
const ID_EDIT: u16 = 500;
const ID_VOICE_BOX: u16 = 501;
const ID_RATE: u16 = 502;
const ID_VOLUME: u16 = 503;

/// Speech has ended: the number in `wparam` says which speech.
const WM_SPEECH_ENDED: u32 = WM_APP + 1;
/// An export has ended: `lparam` holds a boxed message, or nothing if all went well.
const WM_EXPORT_ENDED: u32 = WM_APP + 2;

const TEXT_FILES: &str = "Text files (*.txt)\0*.txt\0All files (*.*)\0*.*\0\0";
const WAV_FILES: &str = "WAV files (*.wav)\0*.wav\0All files (*.*)\0*.*\0\0";

/// Text for Windows: UTF-16, ending in a null.
fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

/// Everything the window keeps. There is one, for as long as the program runs.
struct App {
    window: HWND,
    edit: HWND,
    voice_box: HWND,
    rate: HWND,
    volume: HWND,
    labels: Labels,
    play_button: HWND,
    pause_button: HWND,
    stop_button: HWND,
    menus: Menus,
    scale: f64,
    path: RefCell<Option<PathBuf>>,
    player: RefCell<Option<Player>>,
    /// Counts speech, so news of old speech ending can be told from the new.
    speech_number: Cell<usize>,
    paused: Cell<bool>,
    exporting: Cell<bool>,
    /// Which control had the focus when the window last lost it.
    focus: Cell<HWND>,
}

struct Labels {
    text: HWND,
    voice: HWND,
    rate: HWND,
    volume: HWND,
}

struct Menus {
    edit: HMENU,
    speech: HMENU,
    voice: HMENU,
}

thread_local! {
    static APP: Cell<Option<&'static App>> = const { Cell::new(None) };
}

/// Sends a control a message.
fn send(control: HWND, message: u32, wparam: usize, lparam: isize) -> isize {
    // SAFETY: all the messages sent here pass numbers, or pointers valid for the call.
    unsafe { SendMessageW(control, message, wparam, lparam) }
}

/// Builds the menu bar.
fn build_menus() -> (HMENU, Menus) {
    let item = |menu: HMENU, id: u16, text: &str| {
        // SAFETY: the menu is live and the text lasts for the call.
        unsafe { AppendMenuW(menu, MF_STRING, usize::from(id), wide(text).as_ptr()) };
    };
    let separator = |menu: HMENU| {
        // SAFETY: the menu is live.
        unsafe { AppendMenuW(menu, MF_SEPARATOR, 0, null()) };
    };
    let submenu = |parent: HMENU, child: HMENU, text: &str| {
        // SAFETY: both menus are live and the text lasts for the call.
        unsafe { AppendMenuW(parent, MF_POPUP, child as usize, wide(text).as_ptr()) };
    };
    // SAFETY: creating empty menus has no requirements.
    let (bar, file, edit, speech, voice) =
        unsafe { (CreateMenu(), CreatePopupMenu(), CreatePopupMenu(), CreatePopupMenu(), CreatePopupMenu()) };

    item(file, ID_NEW, "&New\tCtrl+N");
    item(file, ID_OPEN, "&Open...\tCtrl+O");
    item(file, ID_SAVE, "&Save\tCtrl+S");
    item(file, ID_SAVE_AS, "Save &As...\tCtrl+Shift+S");
    separator(file);
    item(file, ID_EXPORT, "&Export WAV...\tCtrl+E");
    separator(file);
    item(file, ID_EXIT, "E&xit");

    item(edit, ID_UNDO, "&Undo\tCtrl+Z");
    separator(edit);
    item(edit, ID_CUT, "Cu&t\tCtrl+X");
    item(edit, ID_COPY, "&Copy\tCtrl+C");
    item(edit, ID_PASTE, "&Paste\tCtrl+V");
    item(edit, ID_DELETE, "De&lete\tDel");
    separator(edit);
    item(edit, ID_SELECT_ALL, "Select &All\tCtrl+A");

    item(speech, ID_PLAY, "&Play\tF5");
    item(speech, ID_PAUSE, "P&ause\tF6");
    item(speech, ID_STOP, "&Stop\tF7");
    separator(speech);
    for (index, def) in VOICES.iter().enumerate() {
        item(voice, ID_FIRST_VOICE + index as u16, &format!("&{}", def.full_name));
    }
    submenu(speech, voice, "&Voice");

    submenu(bar, file, "&File");
    submenu(bar, edit, "&Edit");
    submenu(bar, speech, "&Speech");
    (bar, Menus { edit, speech, voice })
}

/// The keyboard commands. Cut, Copy, Paste, Undo and Delete aren't here: the edit
/// field does those itself, and a table entry would take them from it.
fn accelerators() -> Vec<ACCEL> {
    let control = FVIRTKEY | FCONTROL;
    let keys = [
        (FVIRTKEY, VK_F5, ID_PLAY),
        (FVIRTKEY, VK_F6, ID_PAUSE),
        (FVIRTKEY, VK_F7, ID_STOP),
        (control, b'N'.into(), ID_NEW),
        (control, b'O'.into(), ID_OPEN),
        (control, b'S'.into(), ID_SAVE),
        (control | FSHIFT, b'S'.into(), ID_SAVE_AS),
        (control, b'E'.into(), ID_EXPORT),
        (control, b'A'.into(), ID_SELECT_ALL),
    ];
    keys.iter().map(|&(virt, key, cmd)| ACCEL { fVirt: virt, key, cmd }).collect()
}

/// Asks where a file is, or where to save one. `suggested` starts the dialog off.
fn ask_for_file(owner: HWND, saving: bool, filter: &str, extension: &str, suggested: Option<&Path>) -> Option<PathBuf> {
    let mut name = vec![0u16; 32768];
    if let Some(path) = suggested {
        let path: Vec<u16> = path.to_string_lossy().encode_utf16().collect();
        if path.len() < name.len() {
            name[..path.len()].copy_from_slice(&path);
        }
    }
    let (filter, extension) = (wide(filter), wide(extension));
    let filter = filter.split_last().map_or(&[][..], |(_, rest)| rest);
    // SAFETY: all-zero is valid for this structure, and every pointer in it lasts
    // for the call.
    let chosen = unsafe {
        let mut dialog: OPENFILENAMEW = zeroed();
        dialog.lStructSize = size_of::<OPENFILENAMEW>() as u32;
        dialog.hwndOwner = owner;
        dialog.lpstrFilter = filter.as_ptr();
        dialog.nFilterIndex = 1;
        dialog.lpstrFile = name.as_mut_ptr();
        dialog.nMaxFile = name.len() as u32;
        dialog.lpstrDefExt = extension.as_ptr();
        dialog.Flags = OFN_EXPLORER | OFN_HIDEREADONLY | OFN_NOCHANGEDIR | OFN_PATHMUSTEXIST;
        if saving {
            dialog.Flags |= OFN_OVERWRITEPROMPT;
            GetSaveFileNameW(&mut dialog)
        } else {
            dialog.Flags |= OFN_FILEMUSTEXIST;
            GetOpenFileNameW(&mut dialog)
        }
    };
    if chosen == 0 {
        return None;
    }
    let length = name.iter().position(|&unit| unit == 0).unwrap_or(name.len());
    Some(PathBuf::from(String::from_utf16_lossy(&name[..length])))
}

impl App {
    fn show_message(&self, text: &str, style: MESSAGEBOX_STYLE) -> MESSAGEBOX_RESULT {
        // SAFETY: the window is live and the text lasts for the call.
        unsafe { MessageBoxW(self.window, wide(text).as_ptr(), wide("KlattPad").as_ptr(), style) }
    }

    fn show_error(&self, text: &str) {
        self.show_message(text, MB_OK | MB_ICONERROR);
    }

    fn set_text(&self, control: HWND, text: &str) {
        // SAFETY: the control is live and the text lasts for the call.
        unsafe { SetWindowTextW(control, wide(text).as_ptr()) };
    }

    /// All the text in the edit field.
    fn text(&self) -> Vec<u16> {
        // SAFETY: the buffer holds the length Windows gave, and its null.
        unsafe {
            let length = GetWindowTextLengthW(self.edit).max(0) as usize;
            let mut text = vec![0u16; length + 1];
            let copied = GetWindowTextW(self.edit, text.as_mut_ptr(), text.len() as i32).max(0) as usize;
            text.truncate(copied);
            text
        }
    }

    fn selection(&self) -> (usize, usize) {
        let (mut start, mut end) = (0u32, 0u32);
        send(self.edit, EM_GETSEL, &mut start as *mut u32 as usize, &mut end as *mut u32 as isize);
        (start as usize, end as usize)
    }

    fn modified(&self) -> bool {
        send(self.edit, EM_GETMODIFY, 0, 0) != 0
    }

    fn update_title(&self) {
        let title = document::title(self.path.borrow().as_deref(), self.modified());
        self.set_text(self.window, &title);
    }

    /// Replaces the document.
    fn load(&self, text: &str, path: Option<PathBuf>) {
        self.set_text(self.edit, text);
        send(self.edit, EM_SETMODIFY, 0, 0);
        *self.path.borrow_mut() = path;
        self.update_title();
    }

    /// Saves to a file. Returns whether it worked.
    fn save_to(&self, path: PathBuf) -> bool {
        let text = String::from_utf16_lossy(&self.text());
        if let Err(error) = std::fs::write(&path, text) {
            self.show_error(&format!("Could not save {}: {error}", path.display()));
            return false;
        }
        send(self.edit, EM_SETMODIFY, 0, 0);
        *self.path.borrow_mut() = Some(path);
        self.update_title();
        true
    }

    fn save(&self) -> bool {
        let path = self.path.borrow().clone();
        match path {
            Some(path) => self.save_to(path),
            None => self.save_as(),
        }
    }

    fn save_as(&self) -> bool {
        let current = self.path.borrow().clone();
        match ask_for_file(self.window, true, TEXT_FILES, "txt", current.as_deref()) {
            Some(path) => self.save_to(path),
            None => false,
        }
    }

    /// Asks about unsaved changes. Returns false if the user changed their mind.
    fn may_discard(&self) -> bool {
        if !self.modified() {
            return true;
        }
        let name = document::name(self.path.borrow().as_deref());
        match self.show_message(&format!("Do you want to save changes to {name}?"), MB_YESNOCANCEL | MB_ICONWARNING) {
            IDYES => self.save(),
            IDNO => true,
            _ => false,
        }
    }

    fn new_document(&self) {
        if self.may_discard() {
            self.stop();
            self.load("", None);
        }
    }

    fn open(&self) {
        if !self.may_discard() {
            return;
        }
        let Some(path) = ask_for_file(self.window, false, TEXT_FILES, "txt", None) else { return };
        match std::fs::read(&path) {
            Ok(bytes) => {
                self.stop();
                self.load(&document::with_windows_lines(&document::decode(&bytes)), Some(path));
            }
            Err(error) => self.show_error(&format!("Could not open {}: {error}", path.display())),
        }
    }

    /// The voice the controls have chosen.
    fn chosen_voice(&self) -> klattara::Voice {
        speech::voice_from_controls(
            send(self.voice_box, CB_GETCURSEL, 0, 0).max(0) as usize,
            send(self.rate, TBM_GETPOS, 0, 0).max(0) as u32,
            send(self.volume, TBM_GETPOS, 0, 0).max(0) as u32,
        )
    }

    /// Exports on its own thread, so a long text doesn't freeze the window.
    fn export(&self) {
        if self.exporting.get() {
            self.show_error("An export is still running.");
            return;
        }
        let suggested = self.path.borrow().as_ref().map(|path| path.with_extension("wav"));
        let Some(path) = ask_for_file(self.window, true, WAV_FILES, "wav", suggested.as_deref()) else { return };
        let text = String::from_utf16_lossy(&self.text());
        let mut voice = self.chosen_voice();
        let window = self.window as usize;
        self.exporting.set(true);
        std::thread::spawn(move || {
            let message = match speech::export_wav(&text, &mut voice, &path) {
                Ok(()) => format!("Exported {}.", path.display()),
                Err(error) => format!("Could not export {}: {error}", path.display()),
            };
            let message = Box::into_raw(Box::new(message));
            // SAFETY: the window handles this message by taking back the box. If the
            // window has gone the program is ending, and the box doesn't matter.
            unsafe { PostMessageW(window as HWND, WM_EXPORT_ENDED, 0, message as isize) };
        });
    }

    fn may_close(&self) -> bool {
        if self.exporting.get()
            && self.show_message("An export is still running. Exit anyway?", MB_YESNO | MB_ICONWARNING) != IDYES
        {
            return false;
        }
        self.may_discard()
    }

    fn show_paused(&self, paused: bool) {
        self.paused.set(paused);
        self.set_text(self.pause_button, if paused { "Res&ume" } else { "P&ause" });
        let label = wide(if paused { "&Resume\tF6" } else { "P&ause\tF6" });
        // SAFETY: the menu is live and the text lasts for the call.
        unsafe {
            ModifyMenuW(
                self.menus.speech,
                u32::from(ID_PAUSE),
                MF_BYCOMMAND | MF_STRING,
                usize::from(ID_PAUSE),
                label.as_ptr(),
            );
        }
    }

    /// Plays the selection, or else all the text. Resumes instead if paused.
    fn play(&self) {
        if self.paused.get() {
            self.toggle_pause();
            return;
        }
        self.stop();
        let mut voice = self.chosen_voice();
        let text = speech::part_to_speak(&self.text(), self.selection(), &mut voice);
        if text.trim().is_empty() {
            return;
        }
        let number = self.speech_number.get() + 1;
        self.speech_number.set(number);
        let window = self.window as usize;
        let ended = move || {
            // SAFETY: posting to a window that has gone does nothing.
            unsafe { PostMessageW(window as HWND, WM_SPEECH_ENDED, number, 0) };
        };
        match Player::start(text, voice, ended) {
            Ok(player) => *self.player.borrow_mut() = Some(player),
            Err(error) => self.show_error(&error),
        }
    }

    fn toggle_pause(&self) {
        if let Some(player) = self.player.borrow().as_ref() {
            if self.paused.get() {
                player.resume();
            } else {
                player.pause();
            }
            self.show_paused(!self.paused.get());
        }
    }

    fn stop(&self) {
        drop(self.player.borrow_mut().take());
        if self.paused.get() {
            self.show_paused(false);
        }
    }

    fn choose_voice(&self, index: usize) {
        send(self.voice_box, CB_SETCURSEL, index, 0);
        let first = u32::from(ID_FIRST_VOICE);
        // SAFETY: the menu is live.
        unsafe {
            CheckMenuRadioItem(
                self.menus.voice,
                first,
                first + VOICES.len() as u32 - 1,
                first + index as u32,
                MF_BYCOMMAND,
            );
        }
    }

    /// Carries out a menu command, a keyboard command or a control's news.
    fn command(&self, id: u16, code: u32) {
        match id {
            ID_NEW => self.new_document(),
            ID_OPEN => self.open(),
            ID_SAVE => {
                self.save();
            }
            ID_SAVE_AS => {
                self.save_as();
            }
            ID_EXPORT => self.export(),
            ID_EXIT => {
                // SAFETY: the window is live.
                unsafe { PostMessageW(self.window, WM_CLOSE, 0, 0) };
            }
            ID_UNDO => {
                send(self.edit, WM_UNDO, 0, 0);
            }
            ID_CUT => {
                send(self.edit, WM_CUT, 0, 0);
            }
            ID_COPY => {
                send(self.edit, WM_COPY, 0, 0);
            }
            ID_PASTE => {
                send(self.edit, WM_PASTE, 0, 0);
            }
            ID_DELETE => {
                send(self.edit, WM_CLEAR, 0, 0);
            }
            ID_SELECT_ALL => {
                send(self.edit, EM_SETSEL, 0, -1);
            }
            ID_PLAY => self.play(),
            ID_PAUSE => self.toggle_pause(),
            ID_STOP => self.stop(),
            ID_EDIT if code == EN_CHANGE => self.update_title(),
            ID_VOICE_BOX if code == CBN_SELCHANGE => {
                self.choose_voice(send(self.voice_box, CB_GETCURSEL, 0, 0).max(0) as usize);
            }
            // Enter on a focused button arrives as OK from the dialog keys.
            1 => {
                // SAFETY: asking for the focus has no requirements.
                let focus = unsafe { GetFocus() };
                if focus == self.play_button {
                    self.play();
                } else if focus == self.pause_button {
                    self.toggle_pause();
                } else if focus == self.stop_button {
                    self.stop();
                }
            }
            id if (ID_FIRST_VOICE..ID_FIRST_VOICE + VOICES.len() as u16).contains(&id) => {
                self.choose_voice(usize::from(id - ID_FIRST_VOICE));
            }
            _ => {}
        }
    }

    /// Greys out the Edit menu's commands that have nothing to act on.
    fn update_edit_menu(&self) {
        let (start, end) = self.selection();
        // SAFETY: asking whether text is on the clipboard has no requirements.
        let paste = unsafe { IsClipboardFormatAvailable(u32::from(CF_UNICODETEXT)) } != 0;
        let states = [
            (ID_UNDO, send(self.edit, EM_CANUNDO, 0, 0) != 0),
            (ID_CUT, start != end),
            (ID_COPY, start != end),
            (ID_DELETE, start != end),
            (ID_PASTE, paste),
        ];
        for (id, enabled) in states {
            let state = if enabled { MF_ENABLED } else { MF_GRAYED };
            // SAFETY: the menu is live.
            unsafe { EnableMenuItem(self.menus.edit, u32::from(id), MF_BYCOMMAND | state) };
        }
    }

    /// Places the controls: the edit field on the left, the voice controls stacked on
    /// its right, and the buttons along the bottom.
    fn layout(&self) {
        let px = |size: i32| (f64::from(size) * self.scale).round() as i32;
        let mut client: RECT = RECT { left: 0, top: 0, right: 0, bottom: 0 };
        // SAFETY: the window is live and the structure is writable.
        unsafe { GetClientRect(self.window, &mut client) };
        let (width, height) = (client.right, client.bottom);
        let (margin, gap, label, panel) = (px(12), px(14), px(20), px(220));
        let (button_width, button_height, slider) = (px(96), px(30), px(34));
        let place = |control: HWND, x: i32, y: i32, w: i32, h: i32| {
            // SAFETY: the control is live.
            unsafe { MoveWindow(control, x, y, w, h, 1) };
        };

        let buttons_y = height - margin - button_height;
        let buttons_width = 3 * button_width + 2 * px(8);
        let mut x = (width - buttons_width) / 2;
        for button in [self.play_button, self.pause_button, self.stop_button] {
            place(button, x, buttons_y, button_width, button_height);
            x += button_width + px(8);
        }

        let panel_x = width - margin - panel;
        let edit_width = (panel_x - gap - margin).max(0);
        place(self.labels.text, margin, margin, edit_width, label);
        place(self.edit, margin, margin + label, edit_width, (buttons_y - gap - margin - label).max(0));

        let mut y = margin;
        place(self.labels.voice, panel_x, y, panel, label);
        y += label;
        // A combo box's height includes its open list.
        place(self.voice_box, panel_x, y, panel, px(200));
        y += px(28) + gap;
        for (name, control) in [(self.labels.rate, self.rate), (self.labels.volume, self.volume)] {
            place(name, panel_x, y, panel, label);
            y += label;
            place(control, panel_x, y, panel, slider);
            y += slider + gap;
        }
    }
}

/// Makes the edit field work with the keyboard as part of the window. A multi-line
/// edit field asks for every key, so Tab would never leave it: now Tab moves on, and
/// Ctrl+Tab types a tab. And Windows would select all the text as Tab moves into it,
/// so that typing would replace the whole document: now the cursor stays where it was.
unsafe extern "system" fn edit_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    _: usize,
    _: usize,
) -> LRESULT {
    // SAFETY: Windows calls this with its own valid arguments, passed straight on.
    let result = unsafe { DefSubclassProc(window, message, wparam, lparam) };
    if message != WM_GETDLGCODE {
        return result;
    }
    let mut wants = result as u32 & !DLGC_HASSETSEL;
    // SAFETY: for this message Windows passes null, or the key message being asked about.
    if let Some(key) = unsafe { (lparam as *const MSG).as_ref() }
        && key.message == WM_KEYDOWN
        && key.wParam == usize::from(VK_TAB)
        && unsafe { GetKeyState(i32::from(VK_CONTROL)) } >= 0
    {
        wants &= !(DLGC_WANTALLKEYS | DLGC_WANTMESSAGE | DLGC_WANTTAB);
    }
    wants as LRESULT
}

unsafe extern "system" fn window_proc(window: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let Some(app) = APP.with(Cell::get) else {
        // SAFETY: Windows calls this with its own valid arguments, passed straight on.
        return unsafe { DefWindowProcW(window, message, wparam, lparam) };
    };
    match message {
        WM_SIZE => app.layout(),
        WM_GETMINMAXINFO => {
            let px = |size: i32| (f64::from(size) * app.scale).round() as i32;
            // SAFETY: for this message Windows passes a valid structure to fill in.
            let info = unsafe { &mut *(lparam as *mut MINMAXINFO) };
            info.ptMinTrackSize = POINT { x: px(SMALLEST_SIZE.0), y: px(SMALLEST_SIZE.1) };
        }
        WM_COMMAND => app.command((wparam & 0xFFFF) as u16, ((wparam >> 16) & 0xFFFF) as u32),
        WM_INITMENUPOPUP if wparam as HMENU == app.menus.edit => app.update_edit_menu(),
        // Windows doesn't put the focus back by itself when the user returns to a
        // window that isn't a dialog.
        WM_ACTIVATE => {
            // SAFETY: asking for and setting the focus on our own control.
            unsafe {
                if (wparam & 0xFFFF) as u32 == WA_INACTIVE {
                    let focus = GetFocus();
                    if !focus.is_null() {
                        app.focus.set(focus);
                    }
                } else {
                    SetFocus(app.focus.get());
                }
            }
        }
        WM_SETFOCUS => {
            // SAFETY: setting the focus on our own control.
            unsafe { SetFocus(app.focus.get()) };
        }
        WM_SPEECH_ENDED if wparam == app.speech_number.get() => app.stop(),
        WM_SPEECH_ENDED => {}
        WM_EXPORT_ENDED => {
            app.exporting.set(false);
            // SAFETY: the export thread made this box and gave it up.
            let message = unsafe { Box::from_raw(lparam as *mut String) };
            app.show_message(&message, MB_OK | MB_ICONINFORMATION);
        }
        WM_CLOSE => {
            if app.may_close() {
                app.stop();
                // SAFETY: the window is live.
                unsafe { DestroyWindow(window) };
            }
        }
        WM_DESTROY => {
            // SAFETY: ending the message loop has no requirements.
            unsafe { PostQuitMessage(0) };
        }
        // SAFETY: Windows calls this with its own valid arguments, passed straight on.
        _ => return unsafe { DefWindowProcW(window, message, wparam, lparam) },
    }
    0
}

/// Makes a control. Their order here is their Tab order, and each label comes just
/// before what it names, which is how a screen reader finds the name.
fn make_control(parent: HWND, instance: HINSTANCE, class: &str, text: &str, style: u32, extra: u32, id: u16) -> HWND {
    // SAFETY: the parent is live and the strings last for the call.
    unsafe {
        CreateWindowExW(
            extra,
            wide(class).as_ptr(),
            wide(text).as_ptr(),
            WS_CHILD | WS_VISIBLE | style,
            0,
            0,
            0,
            0,
            parent,
            usize::from(id) as HMENU,
            instance,
            null(),
        )
    }
}

/// Creates the window and runs the program until it closes.
pub fn run() {
    // SAFETY: everything below is the standard sequence for creating a window, with
    // structures filled in as Windows asks and strings that last for each call.
    unsafe {
        let instance = GetModuleHandleW(null());
        let controls = INITCOMMONCONTROLSEX {
            dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_BAR_CLASSES | ICC_STANDARD_CLASSES,
        };
        InitCommonControlsEx(&controls);

        let class_name = wide(CLASS);
        let mut class: WNDCLASSEXW = zeroed();
        class.cbSize = size_of::<WNDCLASSEXW>() as u32;
        class.lpfnWndProc = Some(window_proc);
        class.hInstance = instance;
        class.hIcon = LoadIconW(null_mut(), IDI_APPLICATION);
        class.hCursor = LoadCursorW(null_mut(), IDC_ARROW);
        class.hbrBackground = (COLOR_BTNFACE + 1) as usize as _;
        class.lpszClassName = class_name.as_ptr();
        RegisterClassExW(&class);

        let screen = GetDC(null_mut());
        let scale = f64::from(GetDeviceCaps(screen, LOGPIXELSY as i32)) / 96.0;
        ReleaseDC(null_mut(), screen);
        let width = (f64::from(WINDOW_SIZE.0) * scale).round() as i32;
        let height = (f64::from(WINDOW_SIZE.1) * scale).round() as i32;
        // The whole window, frame and all, sits in the dead center of the screen.
        let x = (GetSystemMetrics(SM_CXSCREEN) - width) / 2;
        let y = (GetSystemMetrics(SM_CYSCREEN) - height) / 2;

        let (bar, menus) = build_menus();
        let window = CreateWindowExW(
            WS_EX_CONTROLPARENT,
            class_name.as_ptr(),
            wide(&document::title(None, false)).as_ptr(),
            WS_OVERLAPPEDWINDOW | WS_CLIPCHILDREN,
            x,
            y,
            width,
            height,
            null_mut(),
            bar,
            instance,
            null(),
        );

        let make = |class: &str, text: &str, style: u32, extra: u32, id: u16| {
            make_control(window, instance, class, text, style, extra, id)
        };
        let label = |text: &str| make("STATIC", text, 0, 0, 0xFFFF);
        let edit_style = ES_MULTILINE | ES_AUTOVSCROLL | ES_WANTRETURN | ES_NOHIDESEL;
        let slider_style = TBS_NOTICKS | WS_TABSTOP;
        let button_style = (BS_PUSHBUTTON as u32) | WS_TABSTOP;

        let text_label = label("&Text:");
        let edit = make("EDIT", "", (edit_style as u32) | WS_VSCROLL | WS_TABSTOP, WS_EX_CLIENTEDGE, ID_EDIT);
        let voice_label = label("&Voice:");
        let voice_box = make("COMBOBOX", "", (CBS_DROPDOWNLIST as u32) | WS_VSCROLL | WS_TABSTOP, 0, ID_VOICE_BOX);
        let rate_label = label("&Rate:");
        let rate = make("msctls_trackbar32", "", slider_style, 0, ID_RATE);
        let volume_label = label("Vo&lume:");
        let volume = make("msctls_trackbar32", "", slider_style, 0, ID_VOLUME);
        let play_button = make("BUTTON", "&Play", button_style, 0, ID_PLAY);
        let pause_button = make("BUTTON", "P&ause", button_style, 0, ID_PAUSE);
        let stop_button = make("BUTTON", "St&op", button_style, 0, ID_STOP);

        // The screen's own message font, at the size the user has chosen.
        let mut metrics: NONCLIENTMETRICSW = zeroed();
        metrics.cbSize = size_of::<NONCLIENTMETRICSW>() as u32;
        SystemParametersInfoW(SPI_GETNONCLIENTMETRICS, metrics.cbSize, (&raw mut metrics).cast(), 0);
        let font = CreateFontIndirectW(&metrics.lfMessageFont);
        let controls = [
            text_label,
            edit,
            voice_label,
            voice_box,
            rate_label,
            rate,
            volume_label,
            volume,
            play_button,
            pause_button,
            stop_button,
        ];
        for control in controls {
            send(control, WM_SETFONT, font as usize, 1);
        }

        SetWindowSubclass(edit, Some(edit_proc), 1, 0);
        // As long as text can be: the default is about 32,000 characters.
        send(edit, EM_SETLIMITTEXT, 0, 0);
        for def in VOICES {
            send(voice_box, CB_ADDSTRING, 0, wide(def.full_name).as_ptr() as isize);
        }
        for (slider, position) in [(rate, NORMAL_RATE_POSITION), (volume, FULL_VOLUME_POSITION)] {
            send(slider, TBM_SETRANGE, 1, 100 << 16);
            send(slider, TBM_SETLINESIZE, 0, 1);
            send(slider, TBM_SETPAGESIZE, 0, 10);
            send(slider, TBM_SETPOS, 1, position as isize);
        }

        let app: &'static App = Box::leak(Box::new(App {
            window,
            edit,
            voice_box,
            rate,
            volume,
            labels: Labels { text: text_label, voice: voice_label, rate: rate_label, volume: volume_label },
            play_button,
            pause_button,
            stop_button,
            menus,
            scale,
            path: RefCell::new(None),
            player: RefCell::new(None),
            speech_number: Cell::new(0),
            paused: Cell::new(false),
            exporting: Cell::new(false),
            focus: Cell::new(edit),
        }));
        APP.with(|cell| cell.set(Some(app)));
        app.choose_voice(0);
        app.layout();
        ShowWindow(window, SW_SHOWNORMAL);
        SetFocus(edit);

        let keys = accelerators();
        let table = CreateAcceleratorTableW(keys.as_ptr(), keys.len() as i32);
        let mut message: MSG = zeroed();
        while GetMessageW(&mut message, null_mut(), 0, 0) > 0 {
            if TranslateAcceleratorW(window, table, &message) != 0 || IsDialogMessageW(window, &message) != 0 {
                continue;
            }
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_function_keys_run_play_pause_and_stop() {
        let keys = accelerators();
        for (key, command) in [(VK_F5, ID_PLAY), (VK_F6, ID_PAUSE), (VK_F7, ID_STOP)] {
            assert!(keys.iter().any(|k| k.key == key && k.cmd == command && k.fVirt == FVIRTKEY));
        }
    }

    #[test]
    fn keyboard_commands_leave_the_edit_field_its_own_keys() {
        // The edit field handles these itself, and Delete with nothing selected has
        // to go on deleting a character.
        for key in *b"XCVZ" {
            assert!(!accelerators().iter().any(|k| k.key == u16::from(key)), "{}", key as char);
        }
        assert!(!accelerators().iter().any(|k| k.cmd == ID_DELETE));
    }

    #[test]
    fn no_two_commands_share_a_key() {
        let keys = accelerators();
        for (i, a) in keys.iter().enumerate() {
            assert!(!keys[i + 1..].iter().any(|b| a.key == b.key && a.fVirt == b.fVirt), "{}", a.key);
        }
    }

    #[test]
    fn text_for_windows_ends_in_a_null() {
        assert_eq!(wide("Hi"), [u16::from(b'H'), u16::from(b'i'), 0]);
    }
}
