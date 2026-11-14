# Runs the installers of request.json on the engine (Node), side by side, and leaves for each
# <name>.json (the result) and <name>.png (the last screen) in out/. A .msi runs through
# Windows Installer (msiexec /i). clicks: [seconds, x, y] on the 1024x768 screen.
# "drive": "C:\\Some\\Folder" unpacks the download (7z) into that folder of drive C: and runs
# "exe" there "runs" times in a row, each run going on from the files the last one left
# (a program that updates itself and restarts, like Steam).
import shutil, json, os, subprocess, sys, urllib.request
here = os.path.dirname(os.path.abspath(__file__))
inksoc = os.path.dirname(os.path.dirname(here))
out = os.path.join(here, 'out'); os.makedirs(out, exist_ok=True)
jobs = []
def drive_job(r, d, f):
    root = os.path.join(d, 'c'); os.makedirs(root, exist_ok=True)
    subprocess.run(['7z', 'x', '-y', '-o' + root, f], stdout=subprocess.DEVNULL, check=True)
    fs = []
    for w in ['bin/wine', 'bin/wine32']:
        fs += [os.path.join(inksoc, w, x) for x in sorted(os.listdir(os.path.join(inksoc, w))) if x.endswith(('.dll', '.nls', '.drv'))]
    fs.append(os.path.join(inksoc, 'bin/wine/roots.der'))
    j = json.dumps({'files': fs, 'exe': r['drive'] + '\\' + r['exe'], 'args': r.get('args', '')})
    log = open(os.path.join(out, r['name'] + '.log'), 'w')
    for i in range(r.get('runs', 1)):
        env = dict(os.environ, LIMIT=str(r.get('limit', 600)), DRIVE=root + '=' + r['drive'], OUTDIR=root, WINEDEBUG='1',
                   SHOT=os.path.join(out, '%s-%d.ppm' % (r['name'], i + 1)))
        log.write('\n=== run %d ===\n' % (i + 1)); log.flush()
        subprocess.run(['node', os.path.join(inksoc, 'compat/run.mjs'), '--one', j], env=env, stdout=log, stderr=subprocess.STDOUT, cwd=inksoc)
        log.flush()
    log.close()
    for x in ['logs/bootstrap_log.txt', 'logs/stderr.txt', 'logs/cef_log.txt']:
        if os.path.exists(os.path.join(root, x)): shutil.copy(os.path.join(root, x), os.path.join(out, r['name'] + '-' + os.path.basename(x)))

drives = []
for r in json.load(open(os.path.join(here, 'request.json'))):
    d = os.path.join('/tmp/apps', r['name']); os.makedirs(d, exist_ok=True)
    f = os.path.join(d, r.get('file') or r['url'].split('/')[-1].split('?')[0])
    if not os.path.exists(f): urllib.request.urlretrieve(r['url'], f)
    if r.get('drive'):
        import threading
        t = threading.Thread(target=drive_job, args=(r, d, f)); t.start(); drives.append(t)
        continue
    exe, args = os.path.basename(f), r.get('args', '')
    if f.lower().endswith('.msi'):
        with open(os.path.join(inksoc, 'bin/wine/wine-msiexec.exe'), 'rb') as a, open(os.path.join(d, 'msiexec.exe'), 'wb') as b: b.write(a.read())
        exe, args = 'msiexec.exe', ('/i "%s" ' % os.path.basename(f) + args).strip()
    clicks = []
    for t, x, y in r.get('clicks', []): clicks += [[t * 1000, 1, x, y, 0], [t * 1000 + 50, 2, x, y, 0], [t * 1000 + 150, 3, x, y, 0]]
    env = dict(os.environ, LIMIT=str(r.get('limit', 600)), INPUT=json.dumps(clicks), SHOT=os.path.join(out, r['name'] + '.ppm'))
    log = open(os.path.join(out, r['name'] + '.log'), 'w')
    jobs.append((r['name'], subprocess.Popen([os.path.join(inksoc, 'compat/app.sh'), d, exe, args], env=env, stdout=log, stderr=subprocess.STDOUT), log))
for t in drives: t.join()
for f in sorted(os.listdir(out)):
    if f.endswith('.ppm') and shutil.which('convert'):
        subprocess.run(['convert', os.path.join(out, f), os.path.join(out, f[:-4] + '.png')]); os.remove(os.path.join(out, f))
for name, p, log in jobs:
    p.wait(); log.close()
    text = open(os.path.join(out, name + '.log')).read().strip().splitlines()
    res = text[-1] if text else ''
    try: j = json.loads(res); print('RESULT', name, j['status'], j['error'][:300].replace('\n', ' | '), len(j['files']), 'files')
    except Exception: print('RESULT', name, 'crashed', ' | '.join(text[-5:])[:300])
    ppm = os.path.join(out, name + '.ppm')
    # the last screen: PNG when ImageMagick is there, else the raw PPM
    if os.path.exists(ppm) and shutil.which('convert'): subprocess.run(['convert', ppm, os.path.join(out, name + '.png')]); os.remove(ppm)
    elif os.path.exists(ppm): shutil.move(ppm, os.path.join(out, name + '.ppm'))
