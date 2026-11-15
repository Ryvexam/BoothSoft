#!/usr/bin/env python3
# SPDX-License-Identifier: CPAL-1.0
# Copyright (c) 2026 Dôm'Booth and the BoothSoft contributors. See LICENSE (Exhibits A and B) and NOTICE.
"""Launch BoothSoft on Xvfb, drive the guest flow, sample CPU% and RSS per step.
usage: monitor.py BINARY STEPS.json OUT_PREFIX [extra app args...]
STEPS: [{"name":..., "click":[x,y] | null, "key": "ctrl+shift+d" | null, "scroll": +n down / -n up (at "at"), "dwell": seconds, "shot": bool}]"""
import json, os, subprocess, sys, time, shutil, tempfile
binary, steps_file, out = sys.argv[1], sys.argv[2], sys.argv[3]
args = sys.argv[4:]
HZ = os.sysconf('SC_CLK_TCK')
env = dict(os.environ, DISPLAY=':99', LIBGL_ALWAYS_SOFTWARE='1',
           XDG_DATA_HOME=os.environ.get('BS_DATA') or tempfile.mkdtemp(prefix='bsdata-'))
cwd = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(binary))))
p = subprocess.Popen([binary, *([] if os.environ.get('NODEMO') else ['--demo']), '--windowed', *args], env=env, cwd=cwd,
                     stdout=subprocess.DEVNULL, stderr=open(out + '.stderr', 'w'))
def cpu():
    f = open(f'/proc/{p.pid}/stat').read().rsplit(')', 1)[1].split()
    return (int(f[11]) + int(f[12])) / HZ
def mem():
    d = {}
    for l in open(f'/proc/{p.pid}/status'):
        k, v = l.split(':', 1)
        if k in ('VmRSS', 'VmHWM', 'Threads'): d[k] = v.strip()
    return d
def threads():
    out = {}
    for t in os.listdir(f'/proc/{p.pid}/task'):
        try:
            name = open(f'/proc/{p.pid}/task/{t}/comm').read().strip()
            f = open(f'/proc/{p.pid}/task/{t}/stat').read().rsplit(')', 1)[1].split()
        except OSError:
            continue
        name = 'llvmpipe (GL raster)' if name.startswith('llvmpipe') else ('ui+gl driver (main)' if name == 'boothsoft' else name)
        out[name] = out.get(name, 0) + (int(f[11]) + int(f[12])) / HZ
    return out
breakdown = []
def x(*a): subprocess.run(['xdotool', *a], env=env, check=False)
time.sleep(4)  # startup
rows = []
startup_cpu = cpu(); m = mem()
rows.append(('startup (4 s)', startup_cpu / 4 * 100, m['VmRSS'], m['VmHWM']))
for i, s in enumerate(json.load(open(steps_file))):
    c0, t0, th0 = cpu(), time.time(), threads()
    if s.get('click'): x('mousemove', *map(str, s['click']), 'click', '1')
    if s.get('key'): x('key', s['key'])
    if s.get('scroll'): x('mousemove', *map(str, s.get('at', [700, 400])), 'click', '--repeat', str(abs(s['scroll'])), '--delay', '30', '5' if s['scroll'] > 0 else '4')
    time.sleep(s['dwell'])
    c1, t1, th1 = cpu(), time.time(), threads()
    if s.get('breakdown'):
        breakdown.append((s['name'], {k: (v - th0.get(k, 0)) / (t1 - t0) * 100 for k, v in th1.items() if v - th0.get(k, 0) > 0.001}))
    m = mem()
    rows.append((s['name'], (c1 - c0) / (t1 - t0) * 100, m['VmRSS'], m['VmHWM']))
    if s.get('shot'):
        subprocess.run(['import', '-window', 'root', '-crop', '1280x800+0+0', f'{out}-{i:02d}.png'], env=env)
p.terminate(); p.wait()
w = max(len(r[0]) for r in rows)
lines = [f"{'step'.ljust(w)} | CPU % (avg over step) | RSS | peak RSS"]
lines += [f"{r[0].ljust(w)} | {r[1]:6.1f} | {r[2]} | {r[3]}" for r in rows]
for name, parts in breakdown:
    lines.append(f'  breakdown [{name}]: ' + ', '.join(f'{k} {v:.1f}%' for k, v in sorted(parts.items(), key=lambda kv: -kv[1])))
open(out + '.txt', 'w').write('\n'.join(lines) + '\n'); print('\n'.join(lines))
