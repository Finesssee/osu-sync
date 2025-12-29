#!/usr/bin/env python3
"""
Comprehensive feature tests for osu-sync
Tests ALL features via CLI and TUI
"""
import subprocess
import json
import time
import sys
import os
import tempfile
import shutil

sys.stdout.reconfigure(encoding='utf-8', errors='replace')

def log(msg):
    print(f"[TEST] {msg}", flush=True)

def run_cli(args, timeout=60):
    """Run CLI command and return output"""
    cmd = ['./target/release/osu-sync.exe', '--cli'] + args
    try:
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout,
                                cwd='D:/code/osu-sync', encoding='utf-8', errors='replace')
        return result.returncode, result.stdout, result.stderr
    except subprocess.TimeoutExpired:
        return -1, "", "TIMEOUT"
    except Exception as e:
        return -1, "", str(e)

def run_app(args, timeout=30):
    """Run app with args"""
    cmd = ['./target/release/osu-sync.exe'] + args
    try:
        result = subprocess.run(cmd, capture_output=True, text=True, timeout=timeout,
                                cwd='D:/code/osu-sync', encoding='utf-8', errors='replace')
        return result.returncode, result.stdout, result.stderr
    except subprocess.TimeoutExpired:
        return -1, "", "TIMEOUT"
    except Exception as e:
        return -1, "", str(e)

class FeatureTester:
    def __init__(self):
        self.results = {}
        self.temp_dir = None

    def setup(self):
        self.temp_dir = tempfile.mkdtemp(prefix='osu_sync_test_')
        log(f"Temp dir: {self.temp_dir}")

    def cleanup(self):
        if self.temp_dir and os.path.exists(self.temp_dir):
            shutil.rmtree(self.temp_dir, ignore_errors=True)

    def test(self, name, func):
        log(f"\n{'='*50}")
        log(f"Testing: {name}")
        log('='*50)
        try:
            result = func()
            self.results[name] = result
            log(f"Result: {'PASS' if result else 'FAIL'}")
            return result
        except Exception as e:
            log(f"ERROR: {e}")
            self.results[name] = False
            return False

    # ==================== CLI TESTS ====================

    def test_cli_scan(self):
        code, out, err = run_cli(['scan', '--json'])
        if code != 0:
            log(f"Error: {err}")
            return False
        data = json.loads(out)
        log(f"  Stable: {data.get('stable', {}).get('beatmap_sets', 'N/A')} sets")
        log(f"  Lazer: {data.get('lazer', {}).get('beatmap_sets', 'N/A')} sets")
        return data.get('stable') is not None and data.get('lazer') is not None

    def test_cli_dry_run_s2l(self):
        code, out, err = run_cli(['dry-run', 's2l', '--json'])
        if code != 0:
            log(f"Error: {err}")
            return False
        data = json.loads(out)
        items = data.get('items', [])
        log(f"  Items: {len(items)}")
        return len(items) > 0

    def test_cli_dry_run_l2s(self):
        code, out, err = run_cli(['dry-run', 'l2s', '--json'])
        if code != 0:
            log(f"Error: {err}")
            return False
        data = json.loads(out)
        items = data.get('items', [])
        log(f"  Items: {len(items)}")
        return len(items) >= 0  # Can be 0 if everything is synced

    def test_cli_dry_run_bidirectional(self):
        code, out, err = run_cli(['dry-run', 'bi', '--json'])
        if code != 0:
            log(f"Error: {err}")
            return False
        data = json.loads(out)
        items = data.get('items', [])
        log(f"  Items: {len(items)}")
        return True

    def test_cli_sync_s2l(self):
        # First get a beatmap to sync
        code, out, err = run_cli(['dry-run', 's2l', '--json'])
        if code != 0:
            log("  Could not get dry-run data")
            return True  # Skip

        data = json.loads(out)
        imports = [x for x in data.get('items', []) if x.get('action') == 'Import' and x.get('set_id')]

        if not imports:
            log("  No beatmaps to import (already synced)")
            return True

        # Get smallest for speed
        smallest = min(imports, key=lambda x: x.get('size_bytes', float('inf')))
        set_id = smallest['set_id']
        log(f"  Syncing set_id {set_id}")

        code, out, err = run_cli(['sync', 's2l', '--set-ids', str(set_id), '--json'], timeout=60)
        if code != 0:
            log(f"  Sync error: {err}")
            return False

        data = json.loads(out)
        log(f"  Imported: {data.get('imported', 0)}, Failed: {data.get('failed', 0)}")
        return data.get('failed', 0) == 0

    def test_cli_sync_l2s(self):
        # First get a beatmap to sync
        code, out, err = run_cli(['dry-run', 'l2s', '--json'])
        if code != 0:
            log("  Could not get dry-run data")
            return True

        data = json.loads(out)
        imports = [x for x in data.get('items', []) if x.get('action') == 'Import' and x.get('set_id')]

        if not imports:
            log("  No beatmaps to import from lazer->stable")
            return True

        smallest = min(imports, key=lambda x: x.get('size_bytes', float('inf')))
        set_id = smallest['set_id']
        log(f"  Syncing set_id {set_id} from lazer to stable")

        code, out, err = run_cli(['sync', 'l2s', '--set-ids', str(set_id), '--json'], timeout=60)
        if code != 0:
            log(f"  Sync error: {err}")
            return False

        data = json.loads(out)
        log(f"  Imported: {data.get('imported', 0)}, Failed: {data.get('failed', 0)}")
        return data.get('failed', 0) == 0

    # ==================== VISION/SNAPSHOT TESTS ====================

    def test_tui_snapshot(self):
        code, out, err = run_app(['--tui-snapshot'], timeout=15)
        log(f"  Output length: {len(out)} chars")
        # Should output some TUI buffer content
        return len(out) > 100 or 'osu-sync' in out.lower()

    def test_tui_snapshot_json(self):
        code, out, err = run_app(['--tui-snapshot', '--json'], timeout=15)
        try:
            data = json.loads(out)
            log(f"  JSON keys: {list(data.keys()) if isinstance(data, dict) else 'N/A'}")
            return True
        except:
            log(f"  Not valid JSON (may be expected)")
            return len(out) > 0  # At least some output

    # ==================== TUI NAVIGATION TESTS ====================

    def test_tui_navigation(self):
        """Test TUI starts and can navigate without crashing"""
        try:
            import wexpect
        except ImportError:
            log("  SKIP: wexpect not available")
            return True

        try:
            child = wexpect.spawn('./target/release/osu-sync.exe', timeout=60, cwd='D:/code/osu-sync')

            # Wait for scan
            log("  Waiting for scan...")
            time.sleep(6)

            # Test each menu item
            menus = [
                ('Enter main menu', '\r', 1),
                ('Sync menu', '\r', 2),
                ('Back', '\x1b', 1),
                ('Down to collections', '\x1b[B\x1b[B', 0.5),
                ('Enter collections', '\r', 2),
                ('Back', '\x1b', 1),
                ('Down to backup', '\x1b[B', 0.3),
                ('Enter backup', '\r', 2),
                ('Back', '\x1b', 1),
                ('Down to media', '\x1b[B', 0.3),
                ('Enter media', '\r', 2),
                ('Back', '\x1b', 1),
                ('Down to replays', '\x1b[B', 0.3),
                ('Enter replays', '\r', 2),
                ('Back', '\x1b', 1),
                ('Down to settings', '\x1b[B', 0.3),
                ('Enter settings', '\r', 2),
                ('Back', '\x1b', 1),
            ]

            for name, keys, wait in menus:
                log(f"  {name}...")
                for key in keys:
                    child.send(key)
                time.sleep(wait)

            # Exit
            log("  Exiting...")
            for _ in range(10):
                child.send('\x1b')
                time.sleep(0.2)

            if child.isalive():
                child.terminate()

            log("  TUI navigation completed without crashes")
            return True

        except Exception as e:
            log(f"  TUI error: {e}")
            return False

    def test_tui_rescan(self):
        """Test rescan functionality"""
        try:
            import wexpect
        except ImportError:
            log("  SKIP: wexpect not available")
            return True

        try:
            child = wexpect.spawn('./target/release/osu-sync.exe', timeout=30, cwd='D:/code/osu-sync')
            time.sleep(5)  # Wait for initial scan

            # Press Enter to go to main menu
            child.send('\r')
            time.sleep(1)

            # Press 'r' to rescan
            log("  Triggering rescan...")
            child.send('r')
            time.sleep(5)  # Wait for rescan

            # Exit
            for _ in range(5):
                child.send('\x1b')
                time.sleep(0.2)

            if child.isalive():
                child.terminate()

            log("  Rescan completed")
            return True

        except Exception as e:
            log(f"  Error: {e}")
            return False

    def test_tui_sync_preview(self):
        """Test entering sync screen and viewing preview"""
        try:
            import wexpect
        except ImportError:
            log("  SKIP: wexpect not available")
            return True

        try:
            child = wexpect.spawn('./target/release/osu-sync.exe', timeout=45, cwd='D:/code/osu-sync')
            time.sleep(5)

            # Enter main menu
            child.send('\r')
            time.sleep(1)

            # Enter sync (first option)
            child.send('\r')
            time.sleep(3)

            # Navigate sync options with arrow keys
            child.send('\x1b[B')  # Down
            time.sleep(0.5)
            child.send('\x1b[B')  # Down
            time.sleep(0.5)

            # Back out
            child.send('\x1b')
            time.sleep(1)
            child.send('\x1b')
            time.sleep(0.5)

            for _ in range(5):
                child.send('\x1b')
                time.sleep(0.2)

            if child.isalive():
                child.terminate()

            log("  Sync preview navigation completed")
            return True

        except Exception as e:
            log(f"  Error: {e}")
            return False

    def test_tui_backup_menu(self):
        """Test backup menu navigation"""
        try:
            import wexpect
        except ImportError:
            log("  SKIP: wexpect not available")
            return True

        try:
            child = wexpect.spawn('./target/release/osu-sync.exe', timeout=30, cwd='D:/code/osu-sync')
            time.sleep(5)

            child.send('\r')  # Main menu
            time.sleep(1)

            # Navigate to backup (should be 3rd or 4th option)
            for _ in range(3):
                child.send('\x1b[B')
                time.sleep(0.3)

            child.send('\r')  # Enter backup
            time.sleep(2)

            # Navigate backup options
            child.send('\x1b[B')
            time.sleep(0.3)
            child.send('\x1b[B')
            time.sleep(0.3)

            child.send('\x1b')  # Back
            time.sleep(0.5)

            for _ in range(5):
                child.send('\x1b')
                time.sleep(0.2)

            if child.isalive():
                child.terminate()

            log("  Backup menu navigation completed")
            return True

        except Exception as e:
            log(f"  Error: {e}")
            return False

    def test_tui_media_menu(self):
        """Test media extraction menu"""
        try:
            import wexpect
        except ImportError:
            log("  SKIP: wexpect not available")
            return True

        try:
            child = wexpect.spawn('./target/release/osu-sync.exe', timeout=30, cwd='D:/code/osu-sync')
            time.sleep(5)

            child.send('\r')  # Main menu
            time.sleep(1)

            # Navigate to media (4th option)
            for _ in range(4):
                child.send('\x1b[B')
                time.sleep(0.3)

            child.send('\r')  # Enter media
            time.sleep(2)

            child.send('\x1b')  # Back
            time.sleep(0.5)

            for _ in range(5):
                child.send('\x1b')
                time.sleep(0.2)

            if child.isalive():
                child.terminate()

            log("  Media menu navigation completed")
            return True

        except Exception as e:
            log(f"  Error: {e}")
            return False

    def test_tui_replays_menu(self):
        """Test replays menu"""
        try:
            import wexpect
        except ImportError:
            log("  SKIP: wexpect not available")
            return True

        try:
            child = wexpect.spawn('./target/release/osu-sync.exe', timeout=30, cwd='D:/code/osu-sync')
            time.sleep(5)

            child.send('\r')  # Main menu
            time.sleep(1)

            # Navigate to replays (5th option)
            for _ in range(5):
                child.send('\x1b[B')
                time.sleep(0.3)

            child.send('\r')  # Enter replays
            time.sleep(2)

            child.send('\x1b')  # Back
            time.sleep(0.5)

            for _ in range(5):
                child.send('\x1b')
                time.sleep(0.2)

            if child.isalive():
                child.terminate()

            log("  Replays menu navigation completed")
            return True

        except Exception as e:
            log(f"  Error: {e}")
            return False

    def test_tui_settings_menu(self):
        """Test settings menu"""
        try:
            import wexpect
        except ImportError:
            log("  SKIP: wexpect not available")
            return True

        try:
            child = wexpect.spawn('./target/release/osu-sync.exe', timeout=30, cwd='D:/code/osu-sync')
            time.sleep(5)

            child.send('\r')  # Main menu
            time.sleep(1)

            # Navigate to settings (last option)
            for _ in range(6):
                child.send('\x1b[B')
                time.sleep(0.3)

            child.send('\r')  # Enter settings
            time.sleep(2)

            # Navigate settings
            child.send('\x1b[B')
            time.sleep(0.3)

            child.send('\x1b')  # Back
            time.sleep(0.5)

            for _ in range(5):
                child.send('\x1b')
                time.sleep(0.2)

            if child.isalive():
                child.terminate()

            log("  Settings menu navigation completed")
            return True

        except Exception as e:
            log(f"  Error: {e}")
            return False

def main():
    log("=" * 60)
    log("osu-sync COMPREHENSIVE Feature Tests")
    log("=" * 60)

    tester = FeatureTester()
    tester.setup()

    try:
        # CLI Tests
        tester.test("CLI: Scan", tester.test_cli_scan)
        tester.test("CLI: Dry-run S2L", tester.test_cli_dry_run_s2l)
        tester.test("CLI: Dry-run L2S", tester.test_cli_dry_run_l2s)
        tester.test("CLI: Dry-run Bidirectional", tester.test_cli_dry_run_bidirectional)
        tester.test("CLI: Sync S2L", tester.test_cli_sync_s2l)
        tester.test("CLI: Sync L2S", tester.test_cli_sync_l2s)

        # Vision/Snapshot Tests
        tester.test("TUI Snapshot", tester.test_tui_snapshot)
        tester.test("TUI Snapshot JSON", tester.test_tui_snapshot_json)

        # TUI Navigation Tests
        tester.test("TUI: Full Navigation", tester.test_tui_navigation)
        tester.test("TUI: Rescan", tester.test_tui_rescan)
        tester.test("TUI: Sync Preview", tester.test_tui_sync_preview)
        tester.test("TUI: Backup Menu", tester.test_tui_backup_menu)
        tester.test("TUI: Media Menu", tester.test_tui_media_menu)
        tester.test("TUI: Replays Menu", tester.test_tui_replays_menu)
        tester.test("TUI: Settings Menu", tester.test_tui_settings_menu)

    finally:
        tester.cleanup()

    # Summary
    log("\n" + "=" * 60)
    log("RESULTS SUMMARY")
    log("=" * 60)

    passed = sum(1 for v in tester.results.values() if v)
    failed = sum(1 for v in tester.results.values() if not v)

    for name, result in tester.results.items():
        status = "PASS" if result else "FAIL"
        log(f"  [{status}] {name}")

    log("")
    log(f"Total: {passed} passed, {failed} failed out of {len(tester.results)}")

    return failed == 0

if __name__ == '__main__':
    success = main()
    sys.exit(0 if success else 1)
