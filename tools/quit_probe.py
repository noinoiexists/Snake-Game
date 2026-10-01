#!/usr/bin/env python3
"""Probe how the game responds to keys, one key at a time.

Answers questions the screen dump cannot: does a key actually reach the app,
does the app exit, and does it put the terminal back. Writes to a file rather
than a pipe so a backgrounded run cannot stall on EOF.

    python3 -u tools/quit_probe.py <binary> <outfile>
"""

import os
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from render_check import Game  # noqa: E402


def describe(g):
    return "alive" if g.alive() else "EXITED"


def main():
    binary = sys.argv[1] if len(sys.argv) > 1 else "./target/debug/snake"
    out = sys.argv[2] if len(sys.argv) > 2 else "/tmp/quit_probe.txt"

    lines = []

    def say(msg):
        lines.append(msg)
        with open(out, "w") as f:
            f.write("\n".join(lines) + "\n")

    # Every send must be followed by a pump: the game repaints continuously,
    # and if nobody drains the pty it blocks in write() and never reads the
    # key you just sent.
    def step(game, keys, label, seconds=0.8):
        game.send(keys)
        game.pump(seconds)
        say(f"{label}: {describe(game)}")

    # 1. plain q from the menu
    g = Game([binary])
    g.pump(1.0)
    say(f"menu rendered, process {describe(g)}")
    step(g, "q", "after 'q' on the menu")
    # On the way out the game must leave the alternate screen, show the
    # cursor again, and reset colours — otherwise the shell comes back broken.
    tail = g.raw[-24:].decode("utf-8", "replace")
    say(f"  restore sequence on exit: {tail!r}")
    say(f"  left alt screen: {chr(27) + '[?1049l' in g.raw.decode('utf-8', 'replace')}")
    say(f"  cursor shown:    {chr(27) + '[?25h' in g.raw.decode('utf-8', 'replace')}")
    say(f"  colours reset:   {chr(27) + '[0m' in g.raw.decode('utf-8', 'replace')}")
    g.kill()

    # 2. q after a pause/resume cycle
    g = Game([binary])
    g.pump(1.0)
    step(g, "\r", "after Enter (play)")
    step(g, "\x1b", "after ESC (pause)")
    step(g, "\x1b", "after ESC (resume)")
    step(g, "q", "after 'q' in game")
    step(g, "q", "after 'q' on the menu", seconds=1.2)
    g.kill()

    # 3. ctrl-c from a running game
    g = Game([binary])
    g.pump(1.0)
    step(g, "\r", "after Enter (play)")
    step(g, "\x03", "after ctrl-c", seconds=1.2)
    g.kill()

    # 4. the quit key while the board is live
    g = Game([binary])
    g.pump(1.0)
    step(g, "\r", "after Enter (play)")
    step(g, "\x1b", "after ESC (pause)")
    step(g, "q", "after 'q' from the pause screen", seconds=1.2)
    g.kill()

    print("\n".join(lines))


if __name__ == "__main__":
    main()
