"""Native bulk-import regression and measured scaling with synthetic local data.

No proxy is contacted. Only disposable project-contained roots are modified.
Timing is diagnostic; the deterministic Rust probe-count test gates complexity.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import logging
import os
from pathlib import Path
import resource
import subprocess
import tempfile
import time

LOG = logging.getLogger('import-scale')


def invoke(binary: Path, root: Path, source: Path, deadline: float) -> dict:
    env = os.environ.copy()
    for key in ('TEMP', 'TMP', 'TMPDIR'):
        env[key] = str(root)
    started = time.monotonic()
    cpu = resource.getrusage(resource.RUSAGE_CHILDREN)
    child = subprocess.Popen([str(binary), '--root', str(root), 'import', str(source)],
                             stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.PIPE, env=env)
    timed_out = False
    try:
        stdout, _ = child.communicate(timeout=deadline)
    except subprocess.TimeoutExpired:
        timed_out = True
        child.kill()
        stdout, _ = child.communicate(timeout=5)
    finally:
        if child.poll() is None:
            child.kill()
            child.wait(timeout=5)
    after = resource.getrusage(resource.RUSAGE_CHILDREN)
    return {'seconds': round(time.monotonic() - started, 6),
            'cpu_seconds': round(after.ru_utime + after.ru_stime - cpu.ru_utime - cpu.ru_stime, 6),
            'exit': child.returncode, 'timeout': timed_out, 'reaped': child.poll() is not None,
            'stdout': stdout.decode('utf-8', errors='replace')}


def run(binary: Path, output: Path, count: int, deadline: float) -> dict:
    with tempfile.TemporaryDirectory(prefix='import-scale-', dir=output) as directory:
        root = Path(directory)
        (root/'config').mkdir()
        path = root/'config/connections.json'
        anchor = {'name': 'anchor', 'url': 'http://127.0.0.1:65534', 'label': 'Preserve active route'}
        path.write_text(json.dumps({'version':1, 'mode':'proxy', 'selected':'anchor', 'profiles':[anchor]}), encoding='utf-8')
        source = root/'synthetic-proxies.txt'
        payload = ''.join(f'http://127.0.0.1:{1000 + index}:u:p:pool\n' for index in range(count)).encode()
        if len(payload) > 1024 * 1024:
            raise ValueError('Fixture exceeds the supported per-import byte limit.')
        source.write_bytes(payload)
        result = invoke(binary, root, source, deadline)
        result['case'] = f'colliding-labels-{count}'
        result['profiles_requested'] = count
        result['input_bytes'] = len(payload)
        result['expected_baseline_name_probes'] = count * (count + 1) // 2
        result['expected_fixed_name_probes'] = count
        output_ok = result.pop('stdout').strip() == f'Imported {count} proxy profiles; skipped 0 duplicates. Active route unchanged.'
        passed = result['exit'] == 0 and not result['timeout'] and result['reaped'] and output_ok
        if passed:
            state = json.loads(path.read_text(encoding='utf-8'))
            profiles = state['profiles']
            passed &= (state['mode'] == 'proxy' and state['selected'] == 'anchor'
                       and profiles[0] == anchor and len(profiles) == count + 1)
            passed &= all(profile['name'] == ('pool' if index == 1 else f'pool-{index}')
                          and profile['label'] == 'pool' for index, profile in enumerate(profiles[1:], 1))
            passed &= source.read_bytes() == payload
            original = path.read_bytes()
            repeated = invoke(binary, root, source, deadline)
            repeated_ok = (repeated['exit'] == 0 and not repeated['timeout'] and repeated['reaped']
                           and repeated.pop('stdout').strip() == f'Imported 0 proxy profiles; skipped {count} duplicates. Active route unchanged.'
                           and path.read_bytes() == original)
            result['duplicate_reimport_unchanged'] = repeated_ok
            passed &= repeated_ok
        else:
            result['duplicate_reimport_unchanged'] = False
        result['passed'] = bool(passed)
        LOG.log(logging.INFO if passed else logging.ERROR,
                'Count=%d bytes=%d elapsed=%.6f cpu=%.6f bounded=%s contract=%s',
                count, len(payload), result['seconds'], result['cpu_seconds'], not result['timeout'], passed)
        return result


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--sizes', type=int, nargs='+', default=[8192])
    parser.add_argument('--deadline', type=float, default=60)
    args = parser.parse_args()
    if any(size <= 0 or size > 30_000 for size in args.sizes) or not 1 <= args.deadline <= 120:
        parser.error('Fixture sizes or child deadline out of bounds.')
    logging.basicConfig(level=logging.INFO, format='%(asctime)s [%(levelname)s] [import-scale] %(message)s')
    logging.Formatter.converter = time.gmtime
    args.output.parent.mkdir(parents=True, exist_ok=True)
    binary = args.binary.resolve()
    rows = []
    for count in args.sizes:
        rows.append(run(binary, args.output.parent, count, args.deadline))
        receipt = {'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
                   'cases': len(rows), 'passed': sum(row['passed'] for row in rows), 'results': rows}
        args.output.write_text(json.dumps(receipt, indent=2) + '\n', encoding='utf-8')
    LOG.debug('Only timing, counts and fixed synthetic case labels are exported; no input or credential payloads.')
    LOG.warning('Elapsed times are measurements, not portable performance guarantees or test thresholds.')
    if not all(row['passed'] for row in rows):
        raise SystemExit(1)
    LOG.info('All naming, duplicate, input-preservation and active-route contracts passed.')


if __name__ == '__main__':
    try:
        main()
    except Exception:
        LOG.error('Bounded bulk-import fixture aborted; no unexecuted cases are counted.')
        raise
