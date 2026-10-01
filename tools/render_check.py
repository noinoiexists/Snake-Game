#!/usr/bin/env python3
"""Drive the game in a real PTY and reconstruct what it paints.

The game only writes the cells that changed since the last frame, so its raw
output is a stream of cursor moves and style changes rather than a picture.
This script runs it at a known window size, replays that stream through a
minimal ANSI interpreter, and prints the resulting screen.

    python3 tools/render_check.py [path-to-binary]
"""

import codecs
import fcntl
import os
import pty
import select
import signal
import struct
import sys
import termios
import threading
import time

COLS, ROWS = 100, 30


# --------------------------------------------------------------------------
# Minimal terminal emulator: enough to reconstruct the grid the game paints.
# --------------------------------------------------------------------------

class Screen:
    def __init__(self, cols, rows):
        self.cols, self.rows = cols, rows
        self.x = self.y = 0
        self.pen = ""
        self.grid = [[" "] * cols for _ in range(rows)]
        self.pen_at = [[""] * cols for _ in range(rows)]
        # Escape sequences arrive split across reads; hold the incomplete tail
        # until the rest of it shows up.
        self.buf = ""

    def clear(self):
        self.grid = [[" "] * self.cols for _ in range(self.rows)]
        self.pen_at = [[""] * self.cols for _ in range(self.rows)]

    def put(self, ch):
        if 0 <= self.y < self.rows and 0 <= self.x < self.cols:
            self.grid[self.y][self.x] = ch
            self.pen_at[self.y][self.x] = self.pen
        self.x += 1

    def handle_csi(self, params, final):
        if final == "H":
            parts = params.split(";")
            row = int(parts[0]) if parts and parts[0] else 1
            col = int(parts[1]) if len(parts) > 1 and parts[1] else 1
            self.y, self.x = row - 1, col - 1
        elif final == "J":
            # Only the full-screen clears matter here.
            if params in ("", "2", "3"):
                self.clear()
                self.x = self.y = 0
        elif final == "m":
            self.apply_sgr(params)

    def apply_sgr(self, params):
        parts = [p for p in params.split(";") if p != ""] or ["0"]
        i = 0
        while i < len(parts):
            p = parts[i]
            if p == "0":
                self.pen = ""
            elif p in ("38", "48") and i + 1 < len(parts):
                if parts[i + 1] == "2" and i + 4 < len(parts):
                    r, g, b = parts[i + 2], parts[i + 3], parts[i + 4]
                    self.pen = f"#{int(r):02x}{int(g):02x}{int(b):02x}"
                    i += 4
                elif parts[i + 1] == "5" and i + 2 < len(parts):
                    self.pen = f"idx{parts[i + 2]}"
                    i += 2
            i += 1

    def feed(self, data):
        self.buf += data
        buf = self.buf
        n = len(buf)
        i = 0
        while i < n:
            c = buf[i]
            if c == "\x1b":
                if i + 1 >= n:
                    break  # need the next byte to know what this is
                if buf[i + 1] == "[":
                    j = i + 2
                    start = j
                    while j < n and not ("@" <= buf[j] <= "~"):
                        j += 1
                    if j >= n:
                        break  # incomplete CSI; wait for the final byte
                    self.handle_csi(buf[start:j], buf[j])
                    i = j + 1
                else:
                    i += 2
            elif c == "\n":
                self.y += 1
                i += 1
            elif c == "\r":
                self.x = 0
                i += 1
            else:
                self.put(c)
                i += 1
        self.buf = buf[i:]

    def text(self):
        """The screen as text.

        Cells painted in the window background colour are shown as a light
        shade: that is how the heading's drop shadow is drawn, and without
        this it reads as a smear of blocks rather than depth.
        """
        bg = self.pen_at[0][0]
        rows = []
        for y in range(self.rows):
            row = []
            for x in range(self.cols):
                ch = self.grid[y][x]
                if ch == "█" and bg and self.pen_at[y][x] == bg:
                    ch = "░"
                row.append(ch)
            rows.append("".join(row).rstrip())
        return "\n".join(rows)


# --------------------------------------------------------------------------
# PTY harness
# --------------------------------------------------------------------------

class Game:
    def __init__(self, argv):
        master, slave = pty.openpty()
        fcntl.ioctl(slave, termios.TIOCSWINSZ, struct.pack("HHHH", ROWS, COLS, 0, 0))
        pid = os.fork()
        if pid == 0:
            os.setsid()
            try:
                fcntl.ioctl(slave, termios.TIOCSCTTY, 0)
            except OSError:
                pass
            os.dup2(slave, 0)
            os.dup2(slave, 1)
            os.dup2(slave, 2)
            if slave > 2:
                os.close(slave)
            os.close(master)
            os.execv(argv[0], argv)
            os._exit(127)
        os.close(slave)
        self.pid = pid
        self.master = master
        self.screen = Screen(COLS, ROWS)
        self.raw = b""
        self.eof = False
        self.lock = threading.Lock()
        # A glyph like "▰" is three bytes and can straddle two reads; decode
        # incrementally so it never turns into replacement characters.
        self.decoder = codecs.getincrementaldecoder("utf-8")("replace")
        # The game repaints continuously. If nobody drains the pty it blocks in
        # write() and stops reading input, so drain on a thread of our own
        # rather than only between keystrokes.
        self.reader = threading.Thread(target=self._drain, daemon=True)
        self.reader.start()

    def _drain(self):
        while True:
            try:
                r, _, _ = select.select([self.master], [], [], 0.05)
                if not r:
                    continue
                chunk = os.read(self.master, 65536)
            except OSError:
                self.eof = True
                return
            if not chunk:
                self.eof = True
                return
            with self.lock:
                self.raw += chunk
                self.screen.feed(self.decoder.decode(chunk))

    def pump(self, seconds):
        """Wait `seconds`, letting the drain thread keep the game unblocked."""
        time.sleep(seconds)
        return not self.eof

    def send(self, keys):
        os.write(self.master, keys.encode())

    def alive(self):
        pid, _ = os.waitpid(self.pid, os.WNOHANG)
        return pid == 0

    def kill(self):
        try:
            if self.alive():
                os.kill(self.pid, signal.SIGKILL)
                os.waitpid(self.pid, 0)
        except ChildProcessError:
            pass


def show(title, screen):
    print(f"\n{'=' * COLS}")
    print(f"  {title}")
    print("=" * COLS)
    print(screen.text())
    print("=" * COLS)


def main():
    binary = sys.argv[1] if len(sys.argv) > 1 else "./target/debug/snake"
    game = Game([binary])

    try:
        game.pump(1.2)
        show("MAIN MENU", game.screen)

        game.send("\r")  # Play
        game.pump(2.0)
        show("GAME (after ~2s of play)", game.screen)

        game.send("\x1b[A")  # steer up
        game.pump(0.6)
        game.send("\x1b[D")  # steer left
        game.pump(0.6)
        show("GAME (after steering)", game.screen)

        game.send("\x1b")  # pause
        game.pump(0.5)
        show("PAUSED", game.screen)

        game.send("\x1b")  # resume
        game.pump(0.4)
        game.send("q")  # back to menu
        game.pump(0.5)
        show("BACK AT MENU", game.screen)

        # Run straight into the right-hand wall: the snake starts out moving
        # right, so this needs no input beyond letting it travel.
        game.send("\r")
        game.pump(6.0)
        show("GAME OVER (wall)", game.screen)

        game.send("q")  # quit
        time.sleep(0.6)
        exited = not game.alive()
        print(f"\nprocess exited on quit: {exited}")
        tail = game.raw[-60:].decode("utf-8", "replace")
        print(f"restore sequence at end of output: {tail!r}")
        print(f"total bytes written: {len(game.raw)}")
    finally:
        game.kill()


if __name__ == "__main__":
    main()
