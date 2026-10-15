"""Bounded real-process checks for persisted JSON input boundaries; no live service."""
import argparse
import hashlib
import json
import logging
import os
from pathlib import Path
import socket
import stat
import subprocess
import tempfile
import time
import urllib.parse

LOG = logging.getLogger('state-input')
LIMIT = 16 * 1024 * 1024
CONNECTIONS = {'version': 1, 'mode': 'blocked', 'selected': None, 'profiles': [
    {'name':'state-sentinel', 'url':'socks5h://127.0.0.1:25195'}]}
LOCAL = {'url': 'socks5h://127.0.0.1:25195'}


def run(binary, work, target, variant, baseline):
    with tempfile.TemporaryDirectory(prefix='state-input-', dir=work) as tmp:
        # Absolute targets are required: symlinks are interpreted relative to their parent.
        root = Path(tmp).resolve()
        (root / 'config').mkdir()
        (root / '.state/rust').mkdir(parents=True)
        # Any unexpected networking can reach only this closed loopback port.
        with socket.socket() as reservation:
            reservation.bind(('127.0.0.1', 0))
            port = reservation.getsockname()[1]
        state = dict(CONNECTIONS)
        if target == 'credentials':
            state.update(mode='proxy', selected='fixture', profiles=[{'name':'fixture', 'url':f'http://127.0.0.1:{port}'}])
        (root / 'config/connections.json').write_text(json.dumps(state), encoding='utf-8')
        if target == 'config':
            path, command, payload = root/'config/connections.json', 'status', CONNECTIONS
        elif target == 'local':
            path, command, payload = root/'config/local-proxy.json', 'local-settings', LOCAL
        else:
            path, command, payload = root/'.state/rust/credentials.json', 'bridge', {}
        if path.exists():
            path.unlink()
        if variant == 'fifo':
            os.mkfifo(path)
        elif variant == 'linked_fifo':
            source = root/'source-fifo'
            os.mkfifo(source)
            path.symlink_to(source)
            assert path.resolve(strict=True) == source
            assert stat.S_ISFIFO(path.stat().st_mode)
        elif variant == 'dangling':
            path.symlink_to(root/'missing-target')
            assert path.is_symlink() and not path.exists()
        elif variant != 'missing':
            source = root/'source.json' if variant == 'regular_link' else path
            with source.open('wb') as output:
                output.write(json.dumps(payload).encode())
                if variant in ('large', 'aggregate'):
                    size = LIMIT + 1 if variant == 'large' else 1024 * 1024 + 500
                    left = size - output.tell()
                    while left:
                        chunk = min(left, 65536)
                        output.write(b' ' * chunk)
                        left -= chunk
            if variant == 'regular_link':
                path.symlink_to(source)
                assert path.resolve(strict=True) == source
                assert path.is_file()
        before = path.lstat() if path.is_symlink() or path.exists() else None
        digest = hashlib.sha256(path.read_bytes()).hexdigest() if path.is_file() else None
        env = os.environ.copy()
        env.update({'TEMP':str(root), 'TMP':str(root), 'TMPDIR':str(root)})
        started = time.monotonic()
        child = subprocess.Popen([str(binary), '--root', str(root), command], stdin=subprocess.DEVNULL,
                                 stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)
        timed_out = False
        stdout = b''
        try:
            stdout, _ = child.communicate(timeout=3)
        except subprocess.TimeoutExpired:
            timed_out = True
            child.kill()
            child.communicate(timeout=2)
        finally:
            if child.poll() is None:
                child.kill()
                child.wait(timeout=2)
        after = path.lstat() if path.is_symlink() or path.exists() else None
        preserved = (before is None and after is None) or (before is not None and after is not None and
                    (before.st_ino, before.st_mode, before.st_size) == (after.st_ino, after.st_mode, after.st_size))
        if digest is not None:
            preserved = preserved and path.is_file() and hashlib.sha256(path.read_bytes()).hexdigest() == digest
        poison = variant in ('fifo', 'linked_fifo', 'dangling', 'large')
        expected_timeout = baseline and variant in ('fifo', 'linked_fifo')
        passed = preserved and timed_out == expected_timeout
        if not expected_timeout:
            expected_failure = poison and not baseline
            passed = passed and ((child.returncode != 0) if expected_failure else (child.returncode == 0))
            if not expected_failure and child.returncode == 0:
                output = json.loads(stdout)
                uses_default = variant in ('missing', 'dangling')
                if target == 'config':
                    names = [profile['name'] for profile in output['profiles']]
                    passed = passed and names == ([] if uses_default else ['state-sentinel'])
                elif target == 'local':
                    actual_port = urllib.parse.urlsplit(output['url']).port
                    passed = passed and actual_port == (1595 if uses_default else 25195)
        receipt = {'case':target+'_'+variant, 'passed':bool(passed), 'timed_out':timed_out,
                   'exit':child.returncode, 'preserved':preserved, 'seconds':round(time.monotonic()-started, 3), 'reaped':True}
        LOG.info('Case %s passed=%s timed_out=%s.', receipt['case'], receipt['passed'], timed_out)
        return receipt


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', required=True, type=Path)
    parser.add_argument('--output', required=True, type=Path)
    parser.add_argument('--expect-baseline', action='store_true')
    args = parser.parse_args()
    logging.basicConfig(level=logging.DEBUG, format='%(asctime)s [%(levelname)s] [state-input] %(message)s')
    logging.Formatter.converter = time.gmtime
    args.output.parent.mkdir(parents=True, exist_ok=True)
    cases = [(target, variant) for target in ('config','local') for variant in
             ('fifo','linked_fifo','dangling','large','missing','regular','regular_link','aggregate')]
    cases.extend([('credentials','fifo'), ('credentials','linked_fifo')])
    LOG.debug('Only synthetic project roots and loopback refusal routes are used.')
    results = []
    try:
        for target, variant in cases:
            results.append(run(args.binary.resolve(), args.output.parent, target, variant, args.expect_baseline))
            args.output.write_text(json.dumps({'cases':len(results),'passed':sum(r['passed'] for r in results),
                                              'baseline':args.expect_baseline,'results':results},indent=2)+'\n',encoding='utf-8')
        LOG.warning('A baseline pass confirms the explicitly expected defects, not healthy behavior.')
        if not all(result['passed'] for result in results):
            LOG.error('State-input contract failed; inspect the sanitized receipt.')
            raise SystemExit(1)
        LOG.info('All %d native state-input cases matched the required contract.', len(results))
    except Exception:
        LOG.error('Bounded state-input fixture failed; no complete result claimed.')
        raise

if __name__ == '__main__':
    main()
