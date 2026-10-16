"""Bounded real-executable probes for internal state; never touch user state."""
from __future__ import annotations
import argparse
import hashlib
import json
import logging
import os
import subprocess
import tempfile
from pathlib import Path

LOG = logging.getLogger('state-reader')
LIMIT = 16 * 1024 * 1024

def seed(root):
    (root/'config').mkdir()
    (root/'.state/rust').mkdir(parents=True)
    (root/'config/connections.json').write_text(json.dumps({'version':1,'mode':'proxy','selected':'fixture',
        'profiles':[{'name':'fixture','url':'http://127.0.0.1:1'}]}), encoding='utf-8')

def invoke(binary, root, command):
    env = os.environ.copy()
    for key in ('TEMP','TMP','TMPDIR'):
        env[key] = str(root)
    child = subprocess.Popen([str(binary),'--root',str(root),command], stdin=subprocess.DEVNULL,
                             stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)
    timeout = False
    try:
        child.communicate(timeout=1.5)
    except subprocess.TimeoutExpired:
        timeout = True
        child.kill()
        child.communicate(timeout=3)
    finally:
        if child.poll() is None:
            child.kill()
            child.wait(timeout=3)
    return {'timeout':timeout,'exit_nonzero':child.returncode != 0,'reaped':child.poll() is not None}

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--expect-baseline', action='store_true')
    args = parser.parse_args()
    logging.basicConfig(level=logging.INFO, format='%(asctime)s [%(levelname)s] [state-reader] %(message)s')
    args.output.parent.mkdir(parents=True, exist_ok=True)
    binary = args.binary.resolve()
    rows = []
    targets = [('config/connections.json','status'),('config/local-proxy.json','local-settings'),
               ('.state/rust/credentials.json','bridge')]
    kinds = ['fifo'] if args.expect_baseline else ['fifo','directory','oversized','invalid_utf8']
    for kind in kinds:
        for relative, command in targets:
            with tempfile.TemporaryDirectory(prefix='state-boundary-', dir=args.output.parent) as tmp:
                root = Path(tmp)
                seed(root)
                path = root/relative
                if path.exists():
                    path.unlink()
                if kind == 'fifo':
                    os.mkfifo(path)
                elif kind == 'directory':
                    path.mkdir()
                elif kind == 'oversized':
                    with path.open('wb') as stream:
                        stream.truncate(LIMIT + 1)
                else:
                    path.write_bytes(b'\xff')
                row = invoke(binary, root, command)
                row.update(case=kind+'-'+command)
                row['passed'] = row['reaped'] and row['exit_nonzero'] and not row['timeout']
                rows.append(row)
                LOG.info('Case=%s bounded_rejection=%s', row['case'], row['passed'])
    with tempfile.TemporaryDirectory(prefix='registration-boundary-', dir=args.output.parent) as tmp:
        root = Path(tmp)
        seed(root)
        path = root/'.mcp.json'
        path.write_bytes(b'{"padding":"'+b'x'*LIMIT+b'"}')
        before = hashlib.sha256(path.read_bytes()).hexdigest()
        row = invoke(binary, root, 'register')
        row.update(case='oversized-valid-registration', original_preserved=hashlib.sha256(path.read_bytes()).hexdigest() == before)
        row['passed'] = row['reaped'] and row['exit_nonzero'] and not row['timeout'] and row['original_preserved']
        rows.append(row)
    args.output.write_text(json.dumps({'cases':len(rows),'passed':sum(row['passed'] for row in rows),'results':rows}, indent=2)+'\n', encoding='utf-8')
    LOG.debug('Only booleans and fixed case names are exported; state bytes are private to temporary roots.')
    LOG.warning('These probes do not model stalled network filesystems or arbitrary kernel device behavior.')
    accepted = all((row['timeout'] and row['reaped']) if row['case'].startswith('fifo-')
                   else not row['exit_nonzero'] and not row['original_preserved']
                   for row in rows) if args.expect_baseline else all(row['passed'] for row in rows)
    if not accepted:
        LOG.error('Internal-state contract failed; inspect the sanitized receipt.')
        raise SystemExit(1)
    LOG.info('Baseline blocking/unbounded publication reproduced.' if args.expect_baseline else 'Every native internal-state rejection preserved bounded execution.')

if __name__ == '__main__':
    try:
        main()
    except Exception:
        LOG.error('State fixture aborted; no completion claim.')
        raise
