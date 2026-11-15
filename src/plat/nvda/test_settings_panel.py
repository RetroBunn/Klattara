"""Exercise the Klattara settings panel: live pitch/rate preview and NVDA's
Apply/Cancel lifecycle. Modelled on Reloq-C's test_hertz_settings_panel.py.
Run standalone: python3 test_settings_panel.py
"""

import builtins
import importlib.util
from pathlib import Path
import sys
import types


class Widget:
    def __init__(self, *args, **kwargs):
        self.value = kwargs.get("initial", False)
        self.label = kwargs.get("label", "")
        self.enabled = True
        self.name = ""
        self.selection = 0
        self.items = []

    def Add(self, *a, **k):
        pass

    def Bind(self, *a, **k):
        pass

    def SetValue(self, value):
        self.value = value

    def GetValue(self):
        return self.value

    def Enable(self, value):
        self.enabled = value

    def SetName(self, value):
        self.name = value

    def GetLabelText(self):
        return self.label

    def SetLabel(self, value):
        self.label = value

    def SetSelection(self, value):
        self.selection = value

    def GetSelection(self):
        return self.selection

    def Set(self, values):
        self.items = values

    def Show(self):
        self.shown = True

    def Hide(self):
        self.shown = False

    def Skip(self):
        pass


def module(name, **values):
    result = types.ModuleType(name)
    vars(result).update(values)
    sys.modules[name] = result
    return result


def check(condition, message):
    if not condition:
        raise AssertionError(message)
    print(f"ok: {message}")


class Synth:
    name = "klattara"
    _pitchInHertz = False
    _pitchHertz = 110
    _rateInWpm = False
    _rateWpm = 190

    def configure_pitch_hertz(self, enabled, value):
        if type(enabled) is not bool or type(value) is not int or not 40 <= value <= 550:
            raise ValueError("invalid Hertz pitch setting")
        self._pitchHertz, self._pitchInHertz = value, enabled

    def configure_rate_wpm(self, enabled, value):
        if type(enabled) is not bool or type(value) is not int or not 38 <= value <= 1796:
            raise ValueError("invalid WPM rate setting")
        self._rateWpm, self._rateInWpm = value, enabled

    def saveSettings(self):
        self.saved = True


def main():
    builtins._ = lambda x: x
    module("addonHandler", initTranslation=lambda: None)
    module(
        "globalPluginHandler",
        GlobalPlugin=type("GlobalPlugin", (), {"terminate": lambda self: None}),
    )
    module("gui", __path__=[], messageBox=lambda *a, **k: None)

    class SettingsPanel(Widget):
        def onPanelActivated(self):
            # The real base class shows the panel; NVDA hides it when the
            # category is left, so without this the panel never appears.
            self.Show()

    categoryClasses = []
    module(
        "gui.settingsDialogs",
        SettingsPanel=SettingsPanel,
        NVDASettingsDialog=type(
            "NVDASettingsDialog", (), {"categoryClasses": categoryClasses}
        ),
    )
    module("ui", message=lambda message: None)
    wx = module(
        "wx",
        Dialog=Widget,
        StaticText=Widget,
        CheckBox=Widget,
        SpinCtrl=Widget,
        Choice=Widget,
        ListBox=Widget,
        BoxSizer=Widget,
        Button=Widget,
        CallAfter=lambda fn, *a: fn(*a),
    )
    for name in (
        "ALL",
        "LEFT",
        "RIGHT",
        "BOTTOM",
        "EXPAND",
        "HORIZONTAL",
        "EVT_CHECKBOX",
        "EVT_SPINCTRL",
        "EVT_TEXT",
        "EVT_BUTTON",
        "EVT_WINDOW_DESTROY",
        "EVT_CHOICE",
        "EVT_LISTBOX",
    ):
        setattr(wx, name, 1)
    synth = Synth()
    module("synthDriverHandler", getSynth=lambda: synth)
    path = (
        Path(__file__).parent
        / "addon/globalPlugins/klattaraSettings/__init__.py"
    )
    spec = importlib.util.spec_from_file_location("klattara_panel_test", path)
    plugin = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(plugin)

    # The panel registers itself as a category of NVDA's settings dialog,
    # and unregisters when the add-on unloads.
    check(
        plugin.KlattaraSettingsPanel not in categoryClasses,
        "the Klattara panel starts unregistered",
    )
    global_plugin = plugin.GlobalPlugin()
    check(
        plugin.KlattaraSettingsPanel in categoryClasses,
        "the Klattara panel registers as a settings category",
    )
    global_plugin.terminate()
    check(
        plugin.KlattaraSettingsPanel not in categoryClasses,
        "the Klattara panel unregisters when the add-on unloads",
    )

    panel = plugin.KlattaraSettingsPanel()
    panel.makeSettings(Widget())
    check(
        panel.pitch_hz_mode.enabled and not panel.pitch_hz_value.enabled,
        "the hertz value is disabled until its check box is on",
    )
    check(
        panel.wpm_mode.enabled and not panel.wpm_value.enabled,
        "the words-per-minute value is disabled until its check box is on",
    )
    check(
        panel.pitch_hz_value.name == "Base pitch in hertz (40–550):",
        "the pitch input is labelled with its exact range",
    )
    check(
        panel.wpm_value.name == "Speech rate in words per minute (38–1796):",
        "the rate input is labelled with its exact range",
    )
    # Live preview: checking the box applies at once, without saving.
    panel.pitch_hz_mode.SetValue(True)
    panel.on_pitch_changed(Widget())
    check(panel.pitch_hz_value.enabled and panel.pitch_dirty, "checking hertz pitch enables its value and marks it changed")
    check(
        synth._pitchInHertz and synth._pitchHertz == 110 and not hasattr(synth, "saved"),
        "checking hertz pitch applies it live, without saving yet",
    )
    panel.pitch_hz_value.SetValue(150)
    panel.on_pitch_changed(Widget())
    check(synth._pitchHertz == 150 and not hasattr(synth, "saved"), "typing a hertz value applies it live")
    panel.wpm_mode.SetValue(True)
    panel.on_wpm_changed(Widget())
    check(panel.wpm_value.enabled and panel.wpm_dirty, "checking words-per-minute rate enables its value and marks it changed")
    panel.wpm_value.SetValue(400)
    panel.on_wpm_changed(Widget())
    check(synth._rateInWpm and synth._rateWpm == 400, "typing a words-per-minute value applies it live")
    panel.onSave()
    check(
        synth._pitchInHertz and synth._pitchHertz == 150 and synth.saved,
        "OK saves the hertz pitch",
    )
    check(
        synth._rateInWpm and synth._rateWpm == 400 and not panel.wpm_dirty,
        "OK saves the words-per-minute rate",
    )
    # Apply becomes the new baseline; cancelling subsequent live changes
    # restores it instead of the values from when the dialog was opened.
    panel.pitch_hz_value.SetValue(220)
    panel.on_pitch_changed(Widget())
    panel.wpm_mode.SetValue(False)
    panel.on_wpm_changed(Widget())
    check(synth._pitchHertz == 220 and not synth._rateInWpm, "a later live change applies at once")
    panel.onDiscard()
    check(
        (synth._pitchInHertz, synth._pitchHertz) == (True, 150),
        "Cancel restores the hertz pitch from the last save",
    )
    check(
        (synth._rateInWpm, synth._rateWpm) == (True, 400),
        "Cancel restores the words-per-minute rate from the last save",
    )
    check(not panel.pitch_dirty and not panel.wpm_dirty, "Cancel clears the changed flags")
    # Incomplete typed input must not preview an invalid value.
    panel.wpm_value.SetValue(0)
    panel.on_wpm_changed(Widget())
    check(synth._rateWpm == 400 and not panel.wpm_dirty, "a zero rate is ignored, not applied")
    panel.pitch_hz_value.SetValue(600)
    panel.on_pitch_changed(Widget())
    check(synth._pitchHertz == 150 and not panel.pitch_dirty, "a hertz value above 550 is ignored, not applied")
    # OK must never die on a value typed past the ends: it clamps. A valid
    # value applied live, then an out-of-range value typed before OK, leaves
    # the changed flag set with an out-of-range value in the control.
    panel.wpm_value.SetValue(30)
    panel.wpm_dirty = True
    panel.pitch_hz_value.SetValue(600)
    panel.pitch_dirty = True
    panel.onSave()
    check(synth._rateWpm == 38, "OK clamps a rate below 38 up to 38")
    check(synth._pitchHertz == 550, "OK clamps a pitch above 550 down to 550")
    # Activating the panel shows it: NVDA hides the panel when the category
    # is left, and the base onPanelActivated is what shows it again. Without
    # the super call the category looks like an empty scroll panel.
    panel.shown = False
    panel.onPanelActivated()
    check(
        getattr(panel, "shown", False),
        "activating the panel shows it",
    )
    # Activating the panel refreshes it from the synth, so a mode switched
    # off by moving a normal slider shows as off.
    synth._rateInWpm = False
    panel.onPanelActivated()
    check(
        not panel.wpm_mode.GetValue() and not panel.wpm_value.enabled,
        "activating the panel shows the rate mode as off",
    )
    synth._pitchInHertz = False
    panel.onPanelActivated()
    check(
        not panel.pitch_hz_mode.GetValue() and not panel.pitch_hz_value.enabled,
        "activating the panel shows the hertz mode as off",
    )
    # Without Klattara as the synthesizer, the panel says so and disables
    # its controls.
    sys.modules["synthDriverHandler"].getSynth = lambda: None
    panel = plugin.KlattaraSettingsPanel()
    panel.makeSettings(Widget())
    check(
        not panel.pitch_hz_mode.enabled and not panel.wpm_mode.enabled,
        "without Klattara the panel's controls are disabled",
    )
    print("All settings panel tests passed.")


if __name__ == "__main__":
    main()
