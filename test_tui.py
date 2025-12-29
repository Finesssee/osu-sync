#!/usr/bin/env python3
"""
TUI Test Script for osu-sync
Uses wexpect to send keyboard inputs and capture output
"""
import wexpect
import time
import sys
import re
import os

# Set UTF-8 output
sys.stdout.reconfigure(encoding='utf-8', errors='replace')

# Key codes
ENTER = '\r'
ESC = '\x1b'
UP = '\x1b[A'
DOWN = '\x1b[B'
LEFT = '\x1b[D'
RIGHT = '\x1b[C'
TAB = '\t'

def log(msg):
    print(f"[TEST] {msg}", flush=True)

def strip_ansi(text):
    """Remove ANSI escape codes to get readable text"""
    ansi_escape = re.compile(r'\x1b\[[0-9;]*[a-zA-Z]|\x1b\].*?\x07|\x1b[PX^_].*?\x1b\\|\x1b.|\x07|\?25[lh]')
    return ansi_escape.sub('', text)

def extract_text(text):
    """Extract meaningful text from TUI output"""
    clean = strip_ansi(text)
    # Remove box drawing characters but keep content
    lines = []
    for line in clean.split('\n'):
        # Remove common box chars
        line = re.sub(r'[─│┌┐└┘├┤┬┴┼╭╮╰╯]+', ' ', line)
        line = re.sub(r'\s+', ' ', line).strip()
        if line and len(line) > 2:
            lines.append(line)
    return lines

class TUITester:
    def __init__(self):
        self.child = None
        self.errors = []
        self.all_output = ""

    def start(self):
        log("Launching osu-sync.exe...")
        self.child = wexpect.spawn('target/release/osu-sync.exe', timeout=120)
        time.sleep(1)

    def send(self, key, wait=0.5):
        self.child.send(key)
        time.sleep(wait)
        self._read_output()

    def _read_output(self):
        try:
            data = self.child.read_nonblocking(size=65536, timeout=0.2)
            if data:
                if isinstance(data, bytes):
                    data = data.decode('utf-8', errors='replace')
                self.all_output += data
        except:
            pass

    def get_screen_text(self):
        """Get current meaningful screen content"""
        return extract_text(self.all_output[-10000:])  # Last 10k chars

    def check_for_errors(self, context):
        recent = self.all_output[-5000:].lower()
        if 'panic' in recent:
            self.errors.append(f"PANIC detected in {context}")
            log(f"!!! PANIC detected in {context} !!!")
            return True
        if 'thread.*panicked' in recent:
            self.errors.append(f"Thread panic in {context}")
            log(f"!!! Thread panic in {context} !!!")
            return True
        return False

    def show_screen(self, label):
        lines = self.get_screen_text()
        if lines:
            log(f"Screen [{label}]:")
            for line in lines[-20:]:  # Last 20 lines
                print(f"    {line[:120]}")
        else:
            log(f"Screen [{label}]: (no content captured)")

    def terminate(self):
        if self.child and self.child.isalive():
            self.child.terminate()

def test_osu_sync():
    log("=" * 60)
    log("osu-sync TUI Integration Tests")
    log("=" * 60)

    tester = TUITester()

    try:
        tester.start()

        # Wait for scan
        log("\n[1] Waiting for initial scan...")
        time.sleep(6)
        tester._read_output()
        tester.show_screen("after scan")
        tester.check_for_errors("initial scan")

        # Enter main menu
        log("\n[2] Entering main menu...")
        tester.send(ENTER, wait=1)
        tester.show_screen("main menu")

        # Test Sync screen
        log("\n[3] Testing Sync screen...")
        tester.send(ENTER, wait=2)
        tester.show_screen("sync")
        tester.check_for_errors("sync screen")
        tester.send(ESC, wait=1)

        # Test Collections
        log("\n[4] Testing Collections...")
        tester.send(DOWN, wait=0.3)
        tester.send(DOWN, wait=0.3)
        tester.send(ENTER, wait=2)
        tester.show_screen("collections")
        tester.check_for_errors("collections")
        tester.send(ESC, wait=1)

        # Test Backup
        log("\n[5] Testing Backup...")
        tester.send(DOWN, wait=0.3)
        tester.send(ENTER, wait=2)
        tester.show_screen("backup")
        tester.check_for_errors("backup")
        tester.send(ESC, wait=1)

        # Test Media
        log("\n[6] Testing Media Extraction...")
        tester.send(DOWN, wait=0.3)
        tester.send(ENTER, wait=2)
        tester.show_screen("media")
        tester.check_for_errors("media")
        tester.send(ESC, wait=1)

        # Test Replays
        log("\n[7] Testing Replays...")
        tester.send(DOWN, wait=0.3)
        tester.send(ENTER, wait=2)
        tester.show_screen("replays")
        tester.check_for_errors("replays")
        tester.send(ESC, wait=1)

        # Test Settings
        log("\n[8] Testing Settings...")
        tester.send(DOWN, wait=0.3)
        tester.send(ENTER, wait=2)
        tester.show_screen("settings")
        tester.check_for_errors("settings")
        tester.send(ESC, wait=1)

        # Test Rescan
        log("\n[9] Testing Rescan (r key)...")
        tester.send('r', wait=5)
        tester.show_screen("after rescan")
        tester.check_for_errors("rescan")

        # Exit
        log("\n[10] Exiting...")
        for _ in range(10):
            tester.send(ESC, wait=0.2)

    except wexpect.TIMEOUT:
        log("ERROR: Application timeout!")
        tester.errors.append("Timeout")
    except wexpect.EOF:
        log("Application exited (EOF)")
    except Exception as e:
        log(f"ERROR: {e}")
        import traceback
        traceback.print_exc()
        tester.errors.append(str(e))
    finally:
        tester.terminate()

    # Results
    log("\n" + "=" * 60)
    if tester.errors:
        log("FAILURES:")
        for e in tester.errors:
            log(f"  - {e}")
        return False
    else:
        log("All tests PASSED!")
        return True

if __name__ == '__main__':
    success = test_osu_sync()
    sys.exit(0 if success else 1)
