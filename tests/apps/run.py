# Runs the installers of request.json on the engine (Node), side by side, and leaves for each
# <name>.json (the result) and <name>.png (the last screen) in out/. A .msi runs through
# Windows Installer (msiexec /i). clicks: [seconds, x, y] on the 1024x768 screen.
# "drive": "C:\\Some\\Folder" unpacks the download (7z) into that folder of drive C: and runs
# "exe" there "runs" times in a row, each run going on from the files the last one left
# (a program that updates itself and restarts, like Steam).
import time, shutil, json, os, subprocess, sys, urllib.request
here = os.path.dirname(os.path.abspath(__file__))
inksoc = os.path.dirname(os.path.dirname(here))
out = os.path.join(here, 'out'); os.makedirs(out, exist_ok=True)
jobs = []

def failure_result(name, status, error):
    message = str(error).replace(chr(10), ' | ')[:400]
    print('RESULT', name, status, message)
    json.dump({'name': name, 'status': status, 'error': message},
              open(os.path.join(out, name + '-result.json'), 'w'))

def drive_job(r, d, f):
    root = os.path.join(d, 'c'); os.makedirs(root, exist_ok=True)
    subprocess.run(['7z', 'x', '-y', '-o' + root, f], stdout=subprocess.DEVNULL, check=True)
    exes = [os.path.relpath(os.path.join(a, x), root) for a, _, xs in os.walk(root) for x in xs if x.lower().endswith('.exe')]
    print('EXES', r['name'], exes[:20])
    if not os.path.exists(os.path.join(root, r['exe'].replace('\\', '/'))):
        failure_result(r['name'], 'missing_executable', r['exe'])
        return
    fs = []
    fs += [os.path.join(inksoc, 'bin/wine', x) for x in json.load(open(os.path.join(inksoc, 'bin/wine/list.json')))]
    fs += [os.path.join(inksoc, 'bin/wine32', x) for x in sorted(os.listdir(os.path.join(inksoc, 'bin/wine32'))) if x.endswith(('.dll', '.nls', '.drv'))]
    j = json.dumps({'files': fs, 'exe': r['drive'] + '\\' + r['exe'], 'args': r.get('args', '')})
    log = open(os.path.join(out, r['name'] + '.log'), 'w')
    for i in range(r.get('runs', 1)):
        env = dict(os.environ, LIMIT=str(r.get('limit', 600)), DRIVE=root + '=' + r['drive'], OUTDIR=root, WINEDEBUG='1',
                   SHOT=os.path.join(out, '%s-%d.ppm' % (r['name'], i + 1)))
        log.write('\n=== run %d ===\n' % (i + 1)); log.flush()
        subprocess.run(['node', os.path.join(inksoc, 'compat/run.mjs'), '--one', j], env=env, stdout=log, stderr=subprocess.STDOUT, cwd=inksoc)
        log.flush()
    log.close()
    # the summary goes to the job log too (readable without downloading the artifact)
    text = open(os.path.join(out, r['name'] + '.log')).read().strip().splitlines()
    try: j = json.loads(text[-1]); print('RESULT', r['name'], j['status'], j['error'][:600].replace('\n', ' | '))
    except Exception: j = None; print('RESULT', r['name'], 'crashed', ' | '.join(text[-5:])[:600])
    summary = {'name': r['name'], 'status': 'crashed', 'error': ' | '.join(text[-3:])[:400]}
    if j: summary = {'name': r['name'], 'status': j['status'], 'exit': j.get('exitCode'), 'error': j['error'][:400], 'wins': j.get('wins'), 'mips': j.get('mips'), 'missing': j.get('missing', [])}
    json.dump(summary, open(os.path.join(out, r['name'] + '-result.json'), 'w'))
    notes = [l for l in text if (l.startswith('[') and not l.startswith('[20')) or ':err:' in l]
    if notes: print('--- engine notes (last 40) ---'); print('\n'.join(n[:300] for n in notes[-40:]))
    b = os.path.join(root, 'logs/bootstrap_log.txt')
    if os.path.exists(b): print('--- bootstrap_log (end) ---'); print('\n'.join(open(b, errors='replace').read().splitlines()[-30:]))
    for x in ['logs/bootstrap_log.txt', 'logs/stderr.txt', 'logs/cef_log.txt']:
        if os.path.exists(os.path.join(root, x)): shutil.copy(os.path.join(root, x), os.path.join(out, r['name'] + '-' + os.path.basename(x)))

def drive_job_safe(r, d, f):
    try:
        drive_job(r, d, f)
    except Exception as ex:
        failure_result(r['name'], 'crashed', f'{type(ex).__name__}: {ex}')

drives = []
# REQUEST=<file> runs another list (compat/apps.json); ONLY=<name> runs one entry of it
only = os.environ.get('ONLY')
for r in json.load(open(os.environ.get('REQUEST') or os.path.join(here, 'request.json'))):
    if only and r['name'] != only: continue
    d = os.path.join('/tmp/apps', r['name']); os.makedirs(d, exist_ok=True)
    f = os.path.join(d, r.get('file') or r['url'].split('/')[-1].split('?')[0])
    if not os.path.exists(f):
        try:
            urllib.request.urlretrieve(r['url'], f)
        except Exception as ex:
            try: os.remove(f)  # avoid treating a partial download as complete next time
            except OSError: pass
            failure_result(r['name'], 'download_failed', f'{type(ex).__name__}: {ex}')
            continue
    if r.get('drive'):
        import threading
        t = threading.Thread(target=drive_job_safe, args=(r, d, f)); t.start(); drives.append(t)
        continue
    exe, args = os.path.basename(f), r.get('args', '')
    if f.lower().endswith('.msi'):
        with open(os.path.join(inksoc, 'bin/wine/wine-msiexec.exe'), 'rb') as a, open(os.path.join(d, 'msiexec.exe'), 'wb') as b: b.write(a.read())
        exe, args = 'msiexec.exe', ('/i "%s" ' % os.path.basename(f) + args).strip()
    clicks = []
    for t, x, y in r.get('clicks', []): clicks += [[t * 1000, 1, x, y, 0], [t * 1000 + 50, 2, x, y, 0], [t * 1000 + 150, 3, x, y, 0]]
    env = dict(os.environ, LIMIT=str(r.get('limit', 600)), INPUT=json.dumps(clicks), SHOT=os.path.join(out, r['name'] + '.ppm'))
    if r.get('ophist'): env['WASM'] = os.path.join(here, 'ophist.wasm')
    if r.get('prof'): env['PROF'] = os.path.join(out, r['name'] + '-prof')
    log = open(os.path.join(out, r['name'] + '.log'), 'w')
    try:
        p = subprocess.Popen([os.path.join(inksoc, 'compat/app.sh'), d, exe, args],
                             env=env, stdout=log, stderr=subprocess.STDOUT, cwd=inksoc)
    except Exception as ex:
        log.close()
        failure_result(r['name'], 'crashed', f'{type(ex).__name__}: {ex}')
        continue
    jobs.append((r['name'], time.time(), p, log))
for t in drives: t.join()
for f in sorted(os.listdir(out)):
    if f.endswith('.ppm') and shutil.which('convert'):
        subprocess.run(['convert', os.path.join(out, f), os.path.join(out, f[:-4] + '.png')]); os.remove(os.path.join(out, f))
for name, t0, p, log in jobs:
    p.wait(); log.close(); secs = int(time.time() - t0)
    text = open(os.path.join(out, name + '.log')).read().strip().splitlines()
    res = text[-1] if text else ''
    try: j = json.loads(res); print('RESULT', name, j['status'], j['error'][:300].replace('\n', ' | '), len(j['files']), 'files,', j.get('mips'), 'MIPS,', secs, 's')
    except Exception: j = None; print('RESULT', name, 'crashed', ' | '.join(text[-5:])[:300])
    # one line per app for the compatibility table (compat/apps-results.md)
    summary = {'name': name, 'status': 'crashed', 'error': ' | '.join(text[-3:])[:400], 'secs': secs}
    if j: summary = {'name': name, 'status': j['status'], 'exit': j.get('exitCode'), 'error': j['error'][:400], 'wins': j.get('wins'), 'files': len(j['files']), 'mips': j.get('mips'), 'secs': secs, 'missing': j.get('missing', [])}
    json.dump(summary, open(os.path.join(out, name + '-result.json'), 'w'))
    for l in text:
        if l.startswith(('OPHIST', 'OPRIP')): print(l)
    # what the engine and Wine said (readable in the job log without the artifact)
    notes = [l for l in text[:-1] if (l.startswith('[') and not l.startswith('[20')) or ':err:' in l or 'not available' in l]
    if notes: print('--- engine notes (last 40) ---'); print('\n'.join(n[:300] for n in notes[-40:]))
    if j and (j['status'] != 'exit' or j.get('exitCode')) and j.get('out'): print('--- program output (end) ---'); print(j['out'][-1500:])
    # where the time went (node --cpu-prof): functions by own time, wasm ones by name
    pd = os.path.join(out, name + '-prof')
    for pf in (os.listdir(pd) if os.path.isdir(pd) else []):
        pr = json.load(open(os.path.join(pd, pf))); self_t = {}
        cnt = {}
        for s_ in pr['samples']: cnt[s_] = cnt.get(s_, 0) + 1
        tot = len(pr['samples']) or 1
        for n in pr['nodes']:
            f = n['callFrame']['functionName'] or '(' + n['callFrame']['url'].split('/')[-1] + ')'
            self_t[f] = self_t.get(f, 0) + cnt.get(n['id'], 0)
        for f, c in sorted(self_t.items(), key=lambda x: -x[1])[:30]: print('PROF %5.1f%% %s' % (100 * c / tot, f[:120]))
    ppm = os.path.join(out, name + '.ppm')
    # the last screen: PNG when ImageMagick is there, else the raw PPM
    if os.path.exists(ppm) and shutil.which('convert'): subprocess.run(['convert', ppm, os.path.join(out, name + '.png')]); os.remove(ppm)
    elif os.path.exists(ppm): shutil.move(ppm, os.path.join(out, name + '.ppm'))
