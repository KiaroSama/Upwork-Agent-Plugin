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
        stdout, stderr = child.communicate(timeout=1.5)
    except subprocess.TimeoutExpired:
        timeout = True
        child.kill()
        stdout, stderr = child.communicate(timeout=3)
    finally:
        if child.poll() is None:
            child.kill()
            child.wait(timeout=3)
    rejection = ('state' if b'Internal state must be a regular file within 16 MiB;' in stderr else
                 'json' if b'Invalid JSON configuration or protocol message.' in stderr else
                 'auth_storage' if b'OAuth failed.' in stderr else
                 'utf8' if b'Connection state must be valid UTF-8;' in stderr else
                 'metadata' if b'Internal state metadata unavailable.' in stderr else 'other')
    marker = ('signin' if b'Upwork sign-in required.' in stderr else
              'registered' if b'Native Upwork-Agent-mcp MCP registered.' in stdout else
              'status' if b'"profiles"' in stdout else 'settings' if b'"url"' in stdout else 'other')
    return {'timeout':timeout,'exit_nonzero':child.returncode != 0,'exit_code':child.returncode,
            'reaped':child.poll() is not None,'rejection':rejection,'marker':marker}

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
    # Load-bearing dispatch controls reject an executable that simply exits1.
    for command in ['status','local-settings','bridge','register']:
        with tempfile.TemporaryDirectory(prefix='state-control-', dir=args.output.parent) as tmp:
            root = Path(tmp)
            seed(root)
            row = invoke(binary, root, command)
            expected = {'status':(0,'status'),'local-settings':(0,'settings'),
                        'bridge':(3,'signin'),'register':(0,'registered')}[command]
            row.update(case='valid-'+command, control=True)
            row['passed'] = row['reaped'] and not row['timeout'] and (row['exit_code'],row['marker']) == expected
            rows.append(row)
    kinds = ['fifo'] if args.expect_baseline else ['fifo','directory','oversized','invalid_utf8','ignored_utf8','dangling']
    for kind in kinds:
        for relative, command in targets:
            if kind == 'ignored_utf8' and command != 'status':
                continue
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
                elif kind == 'dangling':
                    path.symlink_to(root/'absent-target')
                    if command == 'bridge':
                        # Force restore to inspect the broken canonical entry,
                        # not the separate no-session availability shortcut.
                        legacy = root/'.state/auth/mcp-remote-v1'
                        legacy.mkdir(parents=True)
                        name = hashlib.md5(b'https://mcp.upwork.com/mcp').hexdigest()+'_tokens.json'
                        (legacy/name).write_text('{}', encoding='utf-8')
                elif kind == 'oversized':
                    with path.open('wb') as stream:
                        stream.truncate(LIMIT + 1)
                elif kind == 'ignored_utf8':
                    path.write_bytes(b'{"version":1,"mode":"direct","selected":null,"profiles":[],"ignored":"\xff"}')
                else:
                    path.write_bytes(b'\xff')
                row = invoke(binary, root, command)
                expected = ('auth_storage' if command == 'bridge' else 'metadata' if kind == 'dangling' else
                            'utf8' if command == 'status' and kind in {'invalid_utf8','ignored_utf8'} else
                            'json' if kind == 'invalid_utf8' else 'state')
                row.update(case=kind+'-'+command, control=False)
                row['passed'] = (row['reaped'] and row['exit_code'] == 1 and not row['timeout']
                                 and row['rejection'] == expected)
                rows.append(row)
                LOG.info('Case=%s bounded_rejection=%s', row['case'], row['passed'])
    with tempfile.TemporaryDirectory(prefix='registration-boundary-', dir=args.output.parent) as tmp:
        root = Path(tmp)
        seed(root)
        path = root/'.mcp.json'
        path.write_bytes(b'{"padding":"'+b'x'*LIMIT+b'"}')
        before = hashlib.sha256(path.read_bytes()).hexdigest()
        row = invoke(binary, root, 'register')
        row.update(case='oversized-valid-registration', control=False, original_preserved=hashlib.sha256(path.read_bytes()).hexdigest() == before)
        row['passed'] = (row['reaped'] and row['exit_code'] == 1 and not row['timeout']
                         and row['rejection'] == 'state' and row['original_preserved'])
        rows.append(row)
    args.output.write_text(json.dumps({'cases':len(rows),'passed':sum(row['passed'] for row in rows),'results':rows}, indent=2)+'\n', encoding='utf-8')
    LOG.debug('Only booleans and fixed case names are exported; state bytes are private to temporary roots.')
    LOG.warning('These probes do not model stalled network filesystems or arbitrary kernel device behavior.')
    def accepted_row(row):
        if not args.expect_baseline or row['control']:
            return row['passed']
        if row['case'].startswith('fifo-'):
            return row['timeout'] and row['reaped']
        return not row['exit_nonzero'] and not row['original_preserved']
    accepted = all(accepted_row(row) for row in rows)
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
