"""Tests KlattPad itself: the real program, driven through its real window.

It runs on a hidden desktop of its own, so it doesn't take the focus, and a screen
reader doesn't notice it. Playing is tested with the Volume slider at 0, so it makes
no sound. Cut, Copy and Paste aren't tested, because they would overwrite your
clipboard, which all desktops share.

Usage, from the project folder, after building:
    cargo build --release -p klattpad
    cargo build --release -p klattara --example say
    py -3-64 src/plat/klattpad/test_klattpad.py
"""

import ctypes
import os
import subprocess
import sys
import tempfile
import time
from ctypes import wintypes
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
PROGRAM = ROOT / "target" / "release" / "klattpad.exe"
SAY = ROOT / "target" / "release" / "examples" / "say.exe"

user32 = ctypes.WinDLL("user32", use_last_error=True)
kernel32 = ctypes.WinDLL("kernel32", use_last_error=True)
HWND, LPARAM, WPARAM = wintypes.HWND, ctypes.c_ssize_t, ctypes.c_size_t
user32.SendMessageW.argtypes = [HWND, wintypes.UINT, WPARAM, LPARAM]
user32.SendMessageW.restype = ctypes.c_ssize_t
user32.PostMessageW.argtypes = [HWND, wintypes.UINT, WPARAM, LPARAM]
user32.FindWindowW.argtypes = [wintypes.LPCWSTR, wintypes.LPCWSTR]
user32.FindWindowW.restype = HWND
user32.FindWindowExW.argtypes = [HWND, HWND, wintypes.LPCWSTR, wintypes.LPCWSTR]
user32.FindWindowExW.restype = HWND
user32.WaitForInputIdle.argtypes = [wintypes.HANDLE, wintypes.DWORD]
user32.WaitForInputIdle.restype = wintypes.DWORD
user32.GetWindow.argtypes = [HWND, wintypes.UINT]
user32.GetWindow.restype = HWND
user32.GetDlgItem.argtypes = [HWND, ctypes.c_int]
user32.GetDlgItem.restype = HWND
user32.GetMenu.argtypes = [HWND]
user32.GetMenu.restype = wintypes.HMENU
user32.GetSubMenu.argtypes = [wintypes.HMENU, ctypes.c_int]
user32.GetSubMenu.restype = wintypes.HMENU
user32.GetMenuItemCount.argtypes = [wintypes.HMENU]
user32.GetMenuItemID.argtypes = [wintypes.HMENU, ctypes.c_int]
user32.GetMenuItemID.restype = wintypes.UINT
user32.GetMenuState.argtypes = [wintypes.HMENU, wintypes.UINT, wintypes.UINT]
user32.GetMenuState.restype = wintypes.UINT
user32.GetMenuStringW.argtypes = [wintypes.HMENU, wintypes.UINT, wintypes.LPWSTR, ctypes.c_int, wintypes.UINT]
user32.GetClassNameW.argtypes = [HWND, wintypes.LPWSTR, ctypes.c_int]
user32.GetWindowRect.argtypes = [HWND, ctypes.POINTER(wintypes.RECT)]
user32.GetClientRect.argtypes = [HWND, ctypes.POINTER(wintypes.RECT)]
user32.ClientToScreen.argtypes = [HWND, ctypes.POINTER(wintypes.POINT)]
user32.GetWindowLongW.argtypes = [HWND, ctypes.c_int]
user32.GetWindowLongW.restype = ctypes.c_long
user32.CreateDesktopW.argtypes = [wintypes.LPCWSTR, wintypes.LPCWSTR, wintypes.LPVOID, wintypes.DWORD, wintypes.DWORD, wintypes.LPVOID]
user32.CreateDesktopW.restype = wintypes.HANDLE
user32.SetThreadDesktop.argtypes = [wintypes.HANDLE]
user32.CloseDesktop.argtypes = [wintypes.HANDLE]
user32.IsWindow.argtypes = [HWND]

EnumProc = ctypes.WINFUNCTYPE(wintypes.BOOL, HWND, LPARAM)
user32.EnumChildWindows.argtypes = [HWND, EnumProc, LPARAM]
user32.GetParent.argtypes = [HWND]
user32.GetParent.restype = HWND
user32.GetDlgCtrlID.argtypes = [HWND]
user32.IsWindowVisible.argtypes = [HWND]

WM_SETTEXT, WM_GETTEXT, WM_GETTEXTLENGTH, WM_COMMAND, WM_KEYDOWN, WM_KEYUP, WM_CHAR, WM_GETDLGCODE = 0x0C, 0x0D, 0x0E, 0x111, 0x100, 0x101, 0x102, 0x87
EM_GETSEL, EM_SETSEL = 0xB0, 0xB1
CB_GETCOUNT, CB_GETCURSEL, CB_GETLBTEXT, CB_SETCURSEL, CBN_SELCHANGE = 0x146, 0x147, 0x148, 0x14E, 1
TBM_GETPOS, TBM_SETPOS, TBM_GETRANGEMAX, TBM_GETRANGEMIN = 0x400, 0x405, 0x402, 0x401
BM_CLICK, VK_F5, VK_F6, VK_F7 = 0xF5, 0x74, 0x75, 0x76
VK_TAB, VK_RETURN, VK_SPACE, VK_LEFT, VK_RIGHT, VK_DOWN = 0x09, 0x0D, 0x20, 0x25, 0x27, 0x28
GW_CHILD, GW_HWNDNEXT, GW_OWNER, GWL_STYLE = 5, 2, 4, -16
WS_TABSTOP, ES_MULTILINE, DLGC_HASSETSEL = 0x10000, 0x4, 0x8
MF_BYPOSITION, MF_CHECKED, MF_POPUP = 0x400, 0x8, 0x10
IDOK, IDCANCEL, IDYES, IDNO = 1, 2, 6, 7
# The file name box is numbered differently in the Save and Open dialogs.
FILE_NAME_BOXES = (1001, 1148)


class GUITHREADINFO(ctypes.Structure):
    _fields_ = [
        ("cbSize", wintypes.DWORD),
        ("flags", wintypes.DWORD),
        ("hwndActive", HWND),
        ("hwndFocus", HWND),
        ("hwndCapture", HWND),
        ("hwndMenuOwner", HWND),
        ("hwndMoveSize", HWND),
        ("hwndCaret", HWND),
        ("rcCaret", wintypes.RECT),
    ]


user32.GetGUIThreadInfo.argtypes = [wintypes.DWORD, ctypes.POINTER(GUITHREADINFO)]
user32.GetWindowThreadProcessId.argtypes = [HWND, ctypes.POINTER(wintypes.DWORD)]
user32.GetWindowThreadProcessId.restype = wintypes.DWORD


class STARTUPINFOW(ctypes.Structure):
    _fields_ = [
        ("cb", wintypes.DWORD),
        ("lpReserved", wintypes.LPWSTR),
        ("lpDesktop", wintypes.LPWSTR),
        ("lpTitle", wintypes.LPWSTR),
        ("dwX", wintypes.DWORD),
        ("dwY", wintypes.DWORD),
        ("dwXSize", wintypes.DWORD),
        ("dwYSize", wintypes.DWORD),
        ("dwXCountChars", wintypes.DWORD),
        ("dwYCountChars", wintypes.DWORD),
        ("dwFillAttribute", wintypes.DWORD),
        ("dwFlags", wintypes.DWORD),
        ("wShowWindow", wintypes.WORD),
        ("cbReserved2", wintypes.WORD),
        ("lpReserved2", ctypes.POINTER(ctypes.c_byte)),
        ("hStdInput", wintypes.HANDLE),
        ("hStdOutput", wintypes.HANDLE),
        ("hStdError", wintypes.HANDLE),
    ]


class PROCESS_INFORMATION(ctypes.Structure):
    _fields_ = [
        ("hProcess", wintypes.HANDLE),
        ("hThread", wintypes.HANDLE),
        ("dwProcessId", wintypes.DWORD),
        ("dwThreadId", wintypes.DWORD),
    ]


kernel32.CreateProcessW.argtypes = [
    wintypes.LPCWSTR, wintypes.LPWSTR, wintypes.LPVOID, wintypes.LPVOID, wintypes.BOOL, wintypes.DWORD,
    wintypes.LPVOID, wintypes.LPCWSTR, ctypes.POINTER(STARTUPINFOW), ctypes.POINTER(PROCESS_INFORMATION),
]
kernel32.WaitForSingleObject.argtypes = [wintypes.HANDLE, wintypes.DWORD]
kernel32.WaitForSingleObject.restype = wintypes.DWORD
kernel32.GetExitCodeProcess.argtypes = [wintypes.HANDLE, ctypes.POINTER(wintypes.DWORD)]
kernel32.TerminateProcess.argtypes = [wintypes.HANDLE, wintypes.UINT]
kernel32.CloseHandle.argtypes = [wintypes.HANDLE]


class Process:
    """The program, started on a desktop of its own. Python's subprocess module can't
    do this: it ignores the desktop it is given."""

    def __init__(self, program, desktop):
        startup = STARTUPINFOW(cb=ctypes.sizeof(STARTUPINFOW), lpDesktop=f"WinSta0\\{desktop}")
        info = PROCESS_INFORMATION()
        if not kernel32.CreateProcessW(str(program), None, None, None, False, 0, None, None, ctypes.byref(startup), ctypes.byref(info)):
            raise SystemExit(f"could not start the program: error {ctypes.get_last_error()}")
        kernel32.CloseHandle(info.hThread)
        self.handle = info.hProcess
        # Keys posted before the program's message loop runs would wait in its queue,
        # and the test would look for their effect too soon.
        if user32.WaitForInputIdle(self.handle, 30000) != 0:
            raise AssertionError("the program didn't become ready for input")

    def poll(self):
        """The exit code, or None while it is running."""
        if kernel32.WaitForSingleObject(self.handle, 0) != 0:
            return None
        code = wintypes.DWORD()
        kernel32.GetExitCodeProcess(self.handle, ctypes.byref(code))
        return code.value

    def wait(self, seconds):
        wait_for(lambda: self.poll() is not None, "the program to exit", seconds)
        return self.poll()

    def kill(self):
        kernel32.TerminateProcess(self.handle, 1)
        kernel32.WaitForSingleObject(self.handle, 5000)


def check(condition, message):
    if not condition:
        raise AssertionError(message)
    print(f"ok: {message}")


def wait_for(find, what, timeout=15.0):
    end = time.time() + timeout
    while time.time() < end:
        found = find()
        if found:
            return found
        time.sleep(0.03)
    raise AssertionError(f"timed out waiting for {what}")


def text_of(window):
    length = user32.SendMessageW(window, WM_GETTEXTLENGTH, 0, 0)
    buffer = ctypes.create_unicode_buffer(length + 1)
    user32.SendMessageW(window, WM_GETTEXT, length + 1, ctypes.addressof(buffer))
    return buffer.value


def set_window_text(window, text):
    # The buffer has to outlive the call, so it can't be a temporary.
    buffer = ctypes.create_unicode_buffer(text)
    user32.SendMessageW(window, WM_SETTEXT, 0, ctypes.addressof(buffer))


def file_name_field(dialog):
    """The file name box of a file dialog, once it is showing: the dialog builds it
    hidden, and text put in before then doesn't count. The Save dialog nests it
    several windows deep, so this looks through all of them."""
    found = []

    @EnumProc
    def visit(window, _):
        if class_of(window) == "Edit" and user32.GetDlgCtrlID(window) in FILE_NAME_BOXES and user32.IsWindowVisible(window):
            found.append(window)
        return True

    user32.EnumChildWindows(dialog, visit, 0)
    return found[0] if found else None


def class_of(window):
    buffer = ctypes.create_unicode_buffer(64)
    user32.GetClassNameW(window, buffer, 64)
    return buffer.value


def rect_of(window):
    rect = wintypes.RECT()
    user32.GetWindowRect(window, ctypes.byref(rect))
    return rect


def children_of(window):
    found, child = [], user32.GetWindow(window, GW_CHILD)
    while child:
        found.append(child)
        child = user32.GetWindow(child, GW_HWNDNEXT)
    return found


def menu_text(menu, position):
    buffer = ctypes.create_unicode_buffer(128)
    user32.GetMenuStringW(menu, position, buffer, 128, MF_BYPOSITION)
    return buffer.value


def menu_items(menu):
    return [menu_text(menu, i) for i in range(user32.GetMenuItemCount(menu))]


class Pad:
    """The running program, and how to work its window."""

    def __init__(self, process):
        self.process = process
        self.window = wait_for(lambda: user32.FindWindowW("KlattPad", None), "the window")
        controls = wait_for(lambda: len(children_of(self.window)) >= 11 and children_of(self.window), "the controls")
        self.controls = controls
        self.edit, self.voice, self.rate, self.volume = controls[1], controls[3], controls[5], controls[7]
        self.play, self.pause, self.stop = controls[8], controls[9], controls[10]
        self.menu = user32.GetMenu(self.window)
        self.file, self.edit_menu, self.speech = (user32.GetSubMenu(self.menu, i) for i in range(3))

    def command(self, menu, label):
        for i in range(user32.GetMenuItemCount(menu)):
            if menu_text(menu, i).replace("&", "").split("\t")[0].rstrip(".") == label:
                user32.PostMessageW(self.window, WM_COMMAND, user32.GetMenuItemID(menu, i), 0)
                return
        raise AssertionError(f"no menu item {label}: {menu_items(menu)}")

    def key(self, code):
        user32.PostMessageW(self.edit, WM_KEYDOWN, code, 1)
        self.settle()

    def focus(self):
        """The control that has the keyboard focus."""
        info = GUITHREADINFO(cbSize=ctypes.sizeof(GUITHREADINFO))
        user32.GetGUIThreadInfo(user32.GetWindowThreadProcessId(self.window, None), ctypes.byref(info))
        return info.hwndFocus

    def press(self, code):
        """Presses and lets go a key in the focused control, as the keyboard would."""
        focus = self.focus()
        user32.PostMessageW(focus, WM_KEYDOWN, code, 1)
        user32.PostMessageW(focus, WM_KEYUP, code, 0xC0000001)
        self.settle()

    def settle(self):
        """Waits until the program has handled everything sent so far."""
        user32.SendMessageW(self.window, WM_GETTEXTLENGTH, 0, 0)
        time.sleep(0.15)

    def type(self, text):
        for character in text:
            user32.PostMessageW(self.edit, WM_CHAR, ord(character), 1)
        self.settle()

    def set_text(self, text):
        set_window_text(self.edit, text)
        self.settle()

    def set_slider(self, slider, position):
        user32.SendMessageW(slider, TBM_SETPOS, 1, position)

    def select_voice(self, index):
        user32.SendMessageW(self.voice, CB_SETCURSEL, index, 0)
        user32.PostMessageW(self.window, WM_COMMAND, (CBN_SELCHANGE << 16) | 501, self.voice)
        self.settle()

    def title(self):
        return text_of(self.window)

    def dialog(self):
        """The dialog this program has open, once there is one."""

        def find():
            dialog = None
            while True:
                dialog = user32.FindWindowExW(None, dialog, "#32770", None)
                if not dialog:
                    return None
                if user32.GetWindow(dialog, GW_OWNER) == self.window:
                    return dialog

        return wait_for(find, "a dialog")

    def answer_file_dialog(self, path):
        dialog = self.dialog()
        field = wait_for(lambda: file_name_field(dialog), "the file name box")
        time.sleep(0.3)
        set_window_text(field, str(path))
        check(text_of(field) == str(path), f"the file dialog takes the name {path.name}")
        user32.PostMessageW(user32.GetDlgItem(dialog, IDOK), BM_CLICK, 0, 0)
        end = time.time() + 15
        while time.time() < end and user32.IsWindow(dialog):
            time.sleep(0.03)
        if user32.IsWindow(dialog):
            title = user32.FindWindowExW(None, None, "#32770", None)
            raise AssertionError(f"the file dialog didn't close: its file name box holds {text_of(field)!r}, and the top dialog is {text_of(title)!r}")
        self.settle()

    def answer_message(self, button):
        dialog = self.dialog()
        # The message's icon comes first, as an empty label.
        text = " ".join(text_of(c) for c in children_of(dialog) if class_of(c) == "Static").strip()
        # The OK button of a message with only that button is numbered as Cancel.
        target = user32.GetDlgItem(dialog, button) or user32.GetDlgItem(dialog, IDCANCEL)
        user32.PostMessageW(target, BM_CLICK, 0, 0)
        wait_for(lambda: not user32.IsWindow(dialog), "the message to close")
        self.settle()
        return text


def wav_data(path):
    data = Path(path).read_bytes()
    check(data[:4] == b"RIFF", f"{Path(path).name} is a WAV file")
    return data


def say_wav(arguments, path):
    subprocess.run([str(SAY), *arguments, str(path)], check=True, capture_output=True)
    return Path(path).read_bytes()


def test_design(pad):
    check(pad.title() == "Untitled - KlattPad", f"the window is called KlattPad: {pad.title()!r}")
    window = rect_of(pad.window)
    screen = (user32.GetSystemMetrics(0), user32.GetSystemMetrics(1))
    off = (abs(window.left + window.right - screen[0]), abs(window.top + window.bottom - screen[1]))
    check(max(off) <= 1, f"the window is in the dead center of the {screen} screen: {window.left},{window.top}")

    names = [(class_of(c), text_of(c)) for c in pad.controls]
    expected = [
        ("Static", "&Text:"), ("Edit", ""), ("Static", "&Voice:"), ("ComboBox", "Steve"), ("Static", "&Rate:"),
        ("msctls_trackbar32", ""), ("Static", "Vo&lume:"), ("msctls_trackbar32", ""), ("Button", "&Play"),
        ("Button", "P&ause"), ("Button", "St&op"),
    ]
    check(names == expected, f"each control has its label just before it, in Tab order: {names}")
    for control in [pad.edit, pad.voice, pad.rate, pad.volume, pad.play, pad.pause, pad.stop]:
        check(user32.GetWindowLongW(control, GWL_STYLE) & WS_TABSTOP, f"Tab reaches the {class_of(control)} {text_of(control)!r}")
    check(user32.GetWindowLongW(pad.edit, GWL_STYLE) & ES_MULTILINE, "the edit field is multi-line")
    code = user32.SendMessageW(pad.edit, WM_GETDLGCODE, 0, 0)
    check(not code & DLGC_HASSETSEL, "tabbing into the edit field doesn't select everything")

    edit, voice, rate, volume = rect_of(pad.edit), rect_of(pad.voice), rect_of(pad.rate), rect_of(pad.volume)
    check(min(voice.left, rate.left, volume.left) >= edit.right, "the voice controls are to the right of the edit field")
    check(voice.bottom <= rate.top and rate.bottom <= volume.top, "they run down in order: voice, rate, volume")
    buttons = [rect_of(b) for b in (pad.play, pad.pause, pad.stop)]
    check(all(b.top >= edit.bottom and b.top >= volume.bottom for b in buttons), "the buttons are below everything else")
    check(buttons[0].right <= buttons[1].left and buttons[1].right <= buttons[2].left, "in order: Play, Pause, Stop")
    client = wintypes.RECT()
    user32.GetClientRect(pad.window, ctypes.byref(client))
    point = wintypes.POINT(0, client.bottom)
    user32.ClientToScreen(pad.window, ctypes.byref(point))
    check(0 < point.y - buttons[0].bottom < 40, "at the bottom of the window")

    count = user32.SendMessageW(pad.voice, CB_GETCOUNT, 0, 0)
    voices = []
    for i in range(count):
        buffer = ctypes.create_unicode_buffer(64)
        user32.SendMessageW(pad.voice, CB_GETLBTEXT, i, ctypes.addressof(buffer))
        voices.append(buffer.value)
    check(voices == ["Steve", "Brittany", "Gary"], f"the voice box lists the voices: {voices}")
    check(user32.SendMessageW(pad.rate, TBM_GETPOS, 0, 0) == 15, "Rate starts at normal, 15")
    check(user32.SendMessageW(pad.volume, TBM_GETPOS, 0, 0) == 100, "Volume starts at full, 100")
    for slider in (pad.rate, pad.volume):
        low, high = (user32.SendMessageW(slider, m, 0, 0) for m in (TBM_GETRANGEMIN, TBM_GETRANGEMAX))
        check((low, high) == (0, 100), "the sliders run from 0 to 100")


def test_menus(pad):
    check(menu_items(pad.menu) == ["&File", "&Edit", "&Speech"], f"the menus: {menu_items(pad.menu)}")
    file_items = [i.split("\t")[0].replace("&", "").rstrip(".") for i in menu_items(pad.file) if i]
    check(file_items == ["New", "Open", "Save", "Save As", "Export WAV", "Exit"], f"the File menu: {file_items}")
    edit_items = [i.split("\t")[0].replace("&", "") for i in menu_items(pad.edit_menu) if i]
    check(edit_items == ["Undo", "Cut", "Copy", "Paste", "Delete", "Select All"], f"the Edit menu: {edit_items}")
    speech_items = menu_items(pad.speech)
    check(speech_items[:3] == ["&Play\tF5", "P&ause\tF6", "&Stop\tF7"], f"the Speech menu shows F5, F6 and F7: {speech_items[:3]}")
    check(speech_items[-1] == "&Voice", "and ends in a Voice submenu")
    voices = user32.GetSubMenu(pad.speech, user32.GetMenuItemCount(pad.speech) - 1)
    check(menu_items(voices) == ["&Steve", "&Brittany", "&Gary"], f"which lists the voices: {menu_items(voices)}")
    checked = lambda i: bool(user32.GetMenuState(voices, i, MF_BYPOSITION) & MF_CHECKED)
    check(checked(0) and not checked(1), "with Steve checked")
    # The menu and the box follow each other.
    user32.PostMessageW(pad.window, WM_COMMAND, user32.GetMenuItemID(voices, 1), 0)
    pad.settle()
    check(user32.SendMessageW(pad.voice, CB_GETCURSEL, 0, 0) == 1 and checked(1) and not checked(0), "choosing from the menu changes the box")
    pad.select_voice(0)
    check(checked(0) and not checked(1), "choosing from the box changes the menu")


def test_speech_keys(pad):
    pad.set_slider(pad.volume, 0)
    pad.set_text("The quick brown fox jumps over the lazy dog. " * 40)
    check(text_of(pad.pause) == "P&ause", "the Pause button starts as Pause")
    pad.key(VK_F6)
    check(text_of(pad.pause) == "P&ause", "F6 does nothing when nothing is playing")
    pad.key(VK_F5)
    pad.key(VK_F6)
    check(text_of(pad.pause) == "Res&ume", "F5 plays and F6 pauses, and the button says so")
    check(menu_text(pad.speech, 1) == "&Resume\tF6", "and so does the menu")
    pad.key(VK_F6)
    check(text_of(pad.pause) == "P&ause", "F6 resumes")
    pad.key(VK_F6)
    pad.key(VK_F5)
    check(text_of(pad.pause) == "P&ause", "F5 resumes too")
    pad.key(VK_F7)
    pad.key(VK_F6)
    check(text_of(pad.pause) == "P&ause", "F7 stops: then there is nothing to pause")
    pad.key(VK_F5)
    pad.key(VK_F6)
    pad.key(VK_F7)
    check(text_of(pad.pause) == "P&ause", "F7 stops paused speech and resets the button")
    # The buttons do the same as the keys.
    user32.PostMessageW(pad.play, BM_CLICK, 0, 0)
    pad.settle()
    user32.PostMessageW(pad.pause, BM_CLICK, 0, 0)
    pad.settle()
    check(text_of(pad.pause) == "Res&ume", "the Play and Pause buttons work")
    user32.PostMessageW(pad.stop, BM_CLICK, 0, 0)
    pad.settle()
    check(text_of(pad.pause) == "P&ause", "the Stop button works")
    # Speech that ends by itself leaves nothing playing.
    pad.set_text("Hi.")
    pad.key(VK_F5)
    time.sleep(2.0)
    pad.key(VK_F6)
    check(text_of(pad.pause) == "P&ause", "speech that has ended can't be paused")
    pad.set_text("")
    pad.key(VK_F5)
    pad.key(VK_F6)
    check(text_of(pad.pause) == "P&ause", "with no text, Play does nothing")
    pad.set_slider(pad.volume, 100)


def test_keyboard(pad):
    """Moving around and working everything with the keys. Shift+Tab and Ctrl+Tab
    can't be tested this way, because a posted key can't hold Shift or Ctrl down."""
    pad.set_text("Some text.")
    check(pad.focus() == pad.edit, "the edit field has the focus when the program starts")
    user32.SendMessageW(pad.edit, EM_SETSEL, 4, 4)
    order = [pad.voice, pad.rate, pad.volume, pad.play, pad.pause, pad.stop, pad.edit]
    for control in order:
        pad.press(VK_TAB)
        check(pad.focus() == control, f"Tab moves on to the {class_of(control)} {text_of(control)!r}, not {class_of(pad.focus())}")
    check(text_of(pad.edit) == "Some text.", f"Tab doesn't type into the edit field: {text_of(pad.edit)!r}")
    selection = user32.SendMessageW(pad.edit, EM_GETSEL, 0, 0)
    check((selection & 0xFFFF, selection >> 16) == (4, 4), f"coming back with Tab keeps the cursor where it was: {selection}")

    pad.press(VK_TAB)
    pad.press(VK_DOWN)
    check(user32.SendMessageW(pad.voice, CB_GETCURSEL, 0, 0) == 1, "the Down arrow changes the voice in the box")
    voices = user32.GetSubMenu(pad.speech, user32.GetMenuItemCount(pad.speech) - 1)
    check(user32.GetMenuState(voices, 1, MF_BYPOSITION) & MF_CHECKED, "and the Voice submenu follows it")
    pad.select_voice(0)

    pad.press(VK_TAB)
    before = user32.SendMessageW(pad.rate, TBM_GETPOS, 0, 0)
    pad.press(VK_RIGHT)
    check(user32.SendMessageW(pad.rate, TBM_GETPOS, 0, 0) == before + 1, "the Right arrow moves the Rate slider up by 1")
    pad.press(VK_LEFT)
    check(user32.SendMessageW(pad.rate, TBM_GETPOS, 0, 0) == before, "the Left arrow moves it back")

    pad.press(VK_TAB)
    pad.set_slider(pad.volume, 0)
    pad.press(VK_TAB)
    pad.set_text("The quick brown fox jumps over the lazy dog. " * 40)
    check(pad.focus() == pad.play, "Tab reaches the Play button")
    pad.press(VK_RETURN)
    pad.press(VK_TAB)
    pad.press(VK_SPACE)
    check(text_of(pad.pause) == "Res&ume", "Enter works Play, and Space works Pause")
    check(pad.focus() == pad.pause, "and the focus stays on the button")
    pad.press(VK_RETURN)
    check(text_of(pad.pause) == "P&ause", "Enter works Pause too")
    pad.press(VK_TAB)
    pad.press(VK_SPACE)
    pad.key(VK_F6)
    check(text_of(pad.pause) == "P&ause", "Space works Stop: then there is nothing to pause")
    pad.set_slider(pad.volume, 100)
    pad.press(VK_TAB)
    check(pad.focus() == pad.edit, "Tab from the last button comes back to the edit field")
    pad.set_text("")


def test_editing(pad):
    pad.set_text("")
    pad.type("Hello")
    check(pad.title() == "*Untitled - KlattPad", f"typing marks the document changed: {pad.title()!r}")
    check(text_of(pad.edit) == "Hello", "typing reaches the edit field")
    pad.command(pad.edit_menu, "Select All")
    pad.settle()
    selection = user32.SendMessageW(pad.edit, EM_GETSEL, 0, 0)
    check((selection & 0xFFFF, selection >> 16) == (0, 5), f"Select All selects everything: {selection}")
    pad.command(pad.edit_menu, "Delete")
    pad.settle()
    check(text_of(pad.edit) == "", "Delete deletes the selection")
    pad.command(pad.edit_menu, "Undo")
    pad.settle()
    check(text_of(pad.edit) == "Hello", "Undo brings it back")
    pad.type("\r")
    pad.type("x")
    check("\r\n" in text_of(pad.edit), "Enter starts a new line in the edit field")


def test_files(pad, folder):
    saved = folder / "saved.txt"
    pad.set_text("")
    pad.type("café")
    pad.command(pad.file, "Save As")
    pad.answer_file_dialog(saved)
    check(saved.read_bytes() == "café".encode("utf-8"), f"Save As writes UTF-8: {saved.read_bytes()!r}")
    check(pad.title() == "saved.txt - KlattPad", f"and the title names the file: {pad.title()!r}")
    pad.type("!")
    check(pad.title() == "*saved.txt - KlattPad", "another change marks it again")
    pad.command(pad.file, "Save")
    pad.settle()
    check(saved.read_bytes() == "café!".encode("utf-8"), "Save writes to the same file without asking")
    check(pad.title() == "saved.txt - KlattPad", "and clears the mark")

    pad.type("?")
    pad.command(pad.file, "New")
    pad.answer_message(IDCANCEL)
    check(text_of(pad.edit).endswith("?"), "New asks about unsaved changes, and Cancel keeps them")
    pad.command(pad.file, "New")
    pad.answer_message(IDNO)
    check(text_of(pad.edit) == "" and pad.title() == "Untitled - KlattPad", "No discards them and starts afresh")

    other = folder / "older.txt"
    other.write_bytes("Line one\nLine two — dash".encode("utf-16"))
    pad.command(pad.file, "Open")
    pad.answer_file_dialog(other)
    check(text_of(pad.edit) == "Line one\r\nLine two — dash", f"Open reads UTF-16, with Windows line endings: {text_of(pad.edit)!r}")
    check(pad.title() == "older.txt - KlattPad", "and names the file")

    pad.type("y")
    pad.command(pad.file, "New")
    pad.answer_message(IDYES)
    check(b"dash" in other.read_bytes() and other.read_bytes().endswith(b"y") or other.read_bytes().startswith(b"yLine"), f"Yes saves before starting afresh: {other.read_bytes()!r}")
    check(text_of(pad.edit) == "", "and then starts afresh")


def export(pad, path):
    pad.command(pad.file, "Export WAV")
    pad.answer_file_dialog(path)
    message = pad.answer_message(IDOK)
    check(path.name in message and message.startswith("Exported"), f"Export WAV reports: {message!r}")
    check(path.is_file(), f"Export WAV writes {path.name}")
    return path.read_bytes()


def test_export(pad, folder):
    text = "Hello, world. This is KlattPad."
    pad.set_text(text)
    exported = export(pad, folder / "steve.wav")
    check(exported[:4] == b"RIFF", "as a WAV file")
    check(exported == say_wav([text], folder / "say_steve.wav"), "the same audio as the engine makes for Steve")

    pad.select_voice(1)
    exported = export(pad, folder / "brittany.wav")
    check(exported == say_wav(["--voice=Brittany", text], folder / "say_brittany.wav"), "the voice box chooses Brittany")
    pad.select_voice(2)
    exported = export(pad, folder / "gary.wav")
    check(exported == say_wav(["--voice=Gary", text], folder / "say_gary.wav"), "and Gary")
    pad.select_voice(0)

    pad.set_slider(pad.rate, 41)
    fast = export(pad, folder / "fast.wav")
    check(len(fast) < 0.6 * len(exported), f"the Rate slider speeds it up: {len(fast)} against {len(exported)} bytes")
    pad.set_slider(pad.rate, 15)

    pad.set_slider(pad.volume, 50)
    quiet = export(pad, folder / "quiet.wav")
    level = lambda data: sum(abs(int.from_bytes(data[i:i + 2], "little", signed=True)) for i in range(44, len(data), 2))
    full = folder / "steve.wav"
    check(0.4 < level(quiet) / level(full.read_bytes()) < 0.6, "the Volume slider makes it quieter")
    pad.set_slider(pad.volume, 100)

    song = "[:phone klattara enable][D EY1<500,27> Z IY0<500,24>]."
    pad.set_text(song)
    check(export(pad, folder / "song.wav") == say_wav([song], folder / "say_song.wav"), "commands reach the engine, so it sings")

    voice_text = "[:vb]" + text
    pad.set_text(voice_text)
    check(export(pad, folder / "command.wav") == say_wav(["--voice=Brittany", text], folder / "say_command.wav"), "a voice command in the text changes the voice")
    pad.set_text("[:cv pi 200 hs 120 rg 30 br 40]" + text)
    customized = say_wav(["[:cv pi 200 hs 120 rg 30 br 40]" + text], folder / "say_custom.wav")
    check(export(pad, folder / "custom.wav") == customized, "and a [:cv] command customizes it")
    pad.set_text("[:foo]")
    check(export(pad, folder / "alert.wav") == say_wav(["Incorrect command."], folder / "say_alert.wav"), "a bad command says so")
    pad.set_text("[:phone klattara enable][QQ]")
    check(export(pad, folder / "alert2.wav") == say_wav(["Incorrect phoneme."], folder / "say_alert2.wav"), "and so does a bad phoneme")
    pad.set_text("[:cv rc 80 sm 20 ge f]" + text)
    richer = say_wav(["[:cv rc 80 sm 20 ge f]" + text], folder / "say_richer.wav")
    check(export(pad, folder / "richer.wav") == richer, "richness, smoothness and gender reach the sound")
    pad.set_text("[:cv f4 3800 f5 4600 f6 6200 b4 400 b5 500 b6 1500]" + text)
    upper = say_wav(["[:cv f4 3800 f5 4600 f6 6200 b4 400 b5 500 b6 1500]" + text], folder / "say_upper.wav")
    check(export(pad, folder / "upper.wav") == upper, "the upper formants reach the sound")
    pad.set_text("[:speed 500]" + text)
    check(len(export(pad, folder / "speed.wav")) < 0.6 * len((folder / "steve.wav").read_bytes()), "a [:speed] command speeds it up")
    pad.set_text("[:vb][:speed 500][:reset]" + text)
    check(export(pad, folder / "reset.wav") == (folder / "steve.wav").read_bytes(), "and [:reset] puts everything back")
    pad.set_text("")


def test_exit(pad):
    pad.set_text("")
    pad.type("unsaved")
    pad.command(pad.file, "Exit")
    pad.answer_message(IDCANCEL)
    check(pad.process.poll() is None and user32.IsWindow(pad.window), "Exit asks about unsaved changes, and Cancel stays")
    pad.command(pad.file, "Exit")
    pad.answer_message(IDNO)
    check(pad.process.wait(10) == 0, "No exits cleanly")


def start_program(desktop, running):
    """Starts the program and sees that it takes keys. A new program copies the state
    of the real keyboard, and on a hidden desktop it never hears a key let go. If Ctrl
    happened to be down at that moment, say to silence a screen reader, it believes
    Ctrl is held for good, and rightly ignores F5. So that attempt doesn't count. A
    program whose keys are really broken fails every attempt."""
    for _ in range(6):
        process = Process(PROGRAM, desktop)
        running.append(process)
        pad = Pad(process)
        pad.set_slider(pad.volume, 0)
        pad.set_text("Testing the keys.")
        pad.key(VK_F5)
        pad.key(VK_F6)
        answered = text_of(pad.pause) == "Res&ume"
        pad.key(VK_F7)
        pad.set_text("")
        pad.set_slider(pad.volume, 100)
        if answered:
            return pad
        process.kill()
        time.sleep(1.0)
    raise AssertionError("F5 and F6 didn't work on any attempt")


def main():
    for needed in (PROGRAM, SAY):
        if not needed.is_file():
            raise SystemExit(f"{needed} isn't built. See the top of this file.")
    name = f"KlattPadTest{os.getpid()}"
    desktop = user32.CreateDesktopW(name, None, None, 0, 0x10000000, None)
    if not desktop or not user32.SetThreadDesktop(desktop):
        raise SystemExit(f"could not make a hidden desktop: error {ctypes.get_last_error()}")
    running = []
    try:
        pad = start_program(name, running)
        with tempfile.TemporaryDirectory() as folder:
            folder = Path(folder)
            test_design(pad)
            test_menus(pad)
            test_keyboard(pad)
            test_speech_keys(pad)
            test_editing(pad)
            test_files(pad, folder)
            test_export(pad, folder)
            test_exit(pad)
    finally:
        for process in running:
            if process.poll() is None:
                process.kill()
        user32.CloseDesktop(desktop)
    print("All KlattPad tests passed.")


if __name__ == "__main__":
    main()
