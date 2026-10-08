#!/usr/bin/env python3
"""Binds a GNOME custom keyboard shortcut to `calliope --quick-ask`, or removes it.

The GlobalShortcuts portal ("Quick Ask Shortcut…" in Calliope's menu) is the
better way: a custom shortcut launches its command without a usable
activation token, so GNOME may show "Calliope is ready" instead of focusing the
window. This is the fallback for desktops without the portal.

The user's other custom shortcuts are kept.

    install-shortcut.py BINDING COMMAND   add or update, e.g. '<Control><Alt>m'
    install-shortcut.py --remove          remove it
"""

import ast
import subprocess
import sys

SCHEMA = "org.gnome.settings-daemon.plugins.media-keys"
ENTRY_SCHEMA = SCHEMA + ".custom-keybinding"
PATH = "/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings/calliope-quick-ask/"


def gsettings(*args):
    return subprocess.run(
        ["gsettings", *args], check=True, capture_output=True, text=True
    ).stdout.strip()


def custom_paths():
    value = gsettings("get", SCHEMA, "custom-keybindings")
    if value.startswith("@as"):
        value = value[len("@as") :].strip()
    return list(ast.literal_eval(value))


def set_paths(paths):
    gsettings("set", SCHEMA, "custom-keybindings", str(paths))


def main(argv):
    paths = custom_paths()
    if argv[1:] == ["--remove"]:
        if PATH in paths:
            set_paths([p for p in paths if p != PATH])
        gsettings("reset-recursively", f"{ENTRY_SCHEMA}:{PATH}")
        print("Removed the Calliope quick-ask shortcut.")
        return 0
    if len(argv) != 3:
        print(__doc__.strip(), file=sys.stderr)
        return 2
    binding, command = argv[1], argv[2]
    entry = f"{ENTRY_SCHEMA}:{PATH}"
    gsettings("set", entry, "name", "Calliope Quick Ask")
    gsettings("set", entry, "command", command)
    gsettings("set", entry, "binding", binding)
    if PATH not in paths:
        set_paths(paths + [PATH])
    print(f"{binding} now runs: {command}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
