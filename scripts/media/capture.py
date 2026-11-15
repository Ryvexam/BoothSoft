#!/usr/bin/env python3
# SPDX-License-Identifier: CPAL-1.0
# Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
"""Re-shoot the README screenshots (docs/media/<lang>/) on Xvfb :99, 1280x800.
usage: capture.py BINARY LANG    (LANG = en | fr; Xvfb :99 must be running)
Simulated camera ("guests"), Party theme, fixed settings: same pictures every time."""
import json, os, shutil, subprocess, sys, tempfile
HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(os.path.dirname(HERE))
binary, lang = os.path.abspath(sys.argv[1]), sys.argv[2]
out_dir = os.path.join(ROOT, 'docs', 'media', lang)
os.makedirs(out_dir, exist_ok=True)
monitor = os.path.join(ROOT, 'scripts', 'perf', 'monitor.py')
SHUTTER, PRINT, DONE = [640, 688], [1100, 736], [845, 736]
TAPS = [{"name": "tap", "click": [50, 50], "dwell": 0.15} for _ in range(5)]
PIN = [[542, 384], [640, 384], [738, 384], [542, 482], [640, 482], [738, 482]]
CODE = [{"name": "pin", "click": c, "dwell": 0.15} for c in PIN] + [{"name": "ok", "click": [738, 678], "dwell": 0.8}]
RUNS = {
    # name: (settings overrides, steps, {step index: output file})
    'flow': ({}, [
        {"name": "live", "dwell": 5, "shot": True},
        {"name": "sepia", "click": [942, 688], "dwell": 1.5, "shot": True},
        {"name": "natural", "click": [190, 688], "dwell": 0.5},
        {"name": "shutter", "click": SHUTTER, "dwell": 2.6, "shot": True},
        {"name": "review", "dwell": 6.5, "shot": True},
        {"name": "copies", "click": PRINT, "dwell": 1.2, "shot": True},
    ], {0: '02-live', 1: '03-filters', 3: '04-countdown', 4: '06-review', 5: '07-copies'}),
    'thanks': ({"print_enabled": False}, [
        {"name": "live", "dwell": 4},
        {"name": "shutter", "click": SHUTTER, "dwell": 10},
        {"name": "done", "click": [1100, 736], "dwell": 1.2, "shot": True},
    ], {2: '08-thanks'}),
    'burst': ({}, [
        {"name": "live", "dwell": 4},
        {"name": "burst", "click": [628, 58], "dwell": 0.8},
        {"name": "shutter", "click": SHUTTER, "dwell": 19, "shot": True},
    ], {2: '05-burst-picker'}),
    'welcome': ({"sleep_minutes": 1, "wake_on_motion": False}, [
        {"name": "sleep", "dwell": 63, "shot": True},
    ], {0: '01-welcome'}),
    'admin': ({}, [{"name": "live", "dwell": 4}] + TAPS + [{"name": "pin", "dwell": 1}] + CODE + CODE + [
        {"name": "overview", "click": [140, 122], "dwell": 1.2, "shot": True},
        {"name": "events", "click": [140, 172], "dwell": 1.2, "shot": True},
    ], {}),
}
base = {"defaults_version": 3, "event_name": "Léa & Hugo", "print_enabled": True, "guestbook_enabled": True,
        "language": lang, "burst_enabled": True}
for name, (overrides, steps, outputs) in RUNS.items():
    data = tempfile.mkdtemp(prefix='bs-media-')
    os.makedirs(os.path.join(data, 'BoothSoft'))
    json.dump({**base, **overrides}, open(os.path.join(data, 'BoothSoft', 'settings.json'), 'w'))
    steps_file = os.path.join(data, 'steps.json')
    json.dump(steps, open(steps_file, 'w'))
    prefix = os.path.join(data, 'shot')
    # The welcome screen needs a still scene: moving guests keep the booth awake.
    camera = 'static' if name == 'welcome' else 'guests'
    env = dict(os.environ, BS_DATA=data, NODEMO='1', BOOTHSOFT_SYNTHETIC_CAMERA=camera, DISPLAY=':99')
    subprocess.run([sys.executable, monitor, binary, steps_file, prefix], env=env, check=True, stdout=subprocess.DEVNULL)
    if name == 'admin':
        shots = sorted(f for f in os.listdir(data) if f.startswith('shot-') and f.endswith('.png'))
        outputs = {int(shots[-2][5:7]): '09-maintenance', int(shots[-1][5:7]): '10-events'}
    for index, target in outputs.items():
        shutil.copy(f'{prefix}-{index:02d}.png', os.path.join(out_dir, f'{target}.png'))
    shutil.rmtree(data)
    print(name, 'done')
