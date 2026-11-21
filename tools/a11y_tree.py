"""Prints the AT-SPI tree of the running spike example (roles, names, text, caret, selection).

With an offset argument, also asks the editor to move its caret there (AT-SPI setCaretOffset).
"""
import sys
import time

import pyatspi


def dump(node, depth=0, out=None):
    try:
        role = node.getRoleName()
        name = node.name
    except Exception as error:  # noqa: BLE001
        print("  " * depth + f"<error {error}>")
        return
    line = "  " * depth + f"[{role}] {name!r}"
    try:
        text = node.queryText()
        content = text.getText(0, -1)
        line += f" text={content!r} caret={text.caretOffset}"
        if text.getNSelections():
            line += f" selection={text.getSelection(0)}"
    except NotImplementedError:
        pass
    states = node.getState().getStates()
    if pyatspi.STATE_FOCUSED in states:
        line += " FOCUSED"
    print(line)
    for index in range(node.childCount):
        dump(node.getChildAtIndex(index), depth + 1)


desktop = pyatspi.Registry.getDesktop(0)
for _ in range(30):
    apps = [desktop.getChildAtIndex(i) for i in range(desktop.childCount)]
    apps = [app for app in apps if app is not None]
    if apps:
        break
    time.sleep(0.5)
print(f"{len(apps)} application(s)")
for app in apps:
    dump(app)

if len(sys.argv) > 1:
    offset = int(sys.argv[1])

    def find_entry(node):
        if node.getRoleName() == "entry":
            return node
        for index in range(node.childCount):
            found = find_entry(node.getChildAtIndex(index))
            if found:
                return found
        return None

    entry = find_entry(apps[0])
    print("setCaretOffset", offset, "->", entry.queryText().setCaretOffset(offset))
    time.sleep(1)
    text = entry.queryText()
    print(f"caret after: {text.caretOffset}")
