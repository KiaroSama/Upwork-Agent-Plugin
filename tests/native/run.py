"""Run reviewed Linux-native acceptance probes against the packaged final binary."""
import argparse
import datetime
import hashlib
import json
import logging
import os
from pathlib import Path
import runpy
import subprocess
import sys
import tarfile
import time

SCRIPTS = Path(__file__).resolve().parent
ROOT = SCRIPTS.parent.parent
EXTENDED = 'valid_json_control mixed_error_json mixed_error_sse mixed_error_null_json mixed_error_null_sse mixed_method_json mixed_method_sse invalid_jsonrpc_json invalid_jsonrpc_sse top_array_json top_array_sse invalid_utf8_json invalid_utf8_sse malformed_then_valid_sse error_then_valid_sse invalid_version_then_valid_sse valid_sse_lf valid_sse_crlf valid_sse_comments oversize_json'.split()


def worker():
    script, output = sys.argv[2:4]
    assert script in {'cli_probe.py', 'protocol_probe.py', 'extended_protocol_probe.py', 'terminal_sse_probe.py', 'credential_coordination_probe.py', 'initialization_ping_probe.py', 'state_reader_probe.py', 'import_scale_probe.py', 'route_revision_probe.py', 'http_media_probe.py'}
    # -I excludes ambient import paths; only reviewed, tracked sibling fixtures are added.
    sys.path.insert(0, str(SCRIPTS))
    sys.dont_write_bytecode = True
    stamp = datetime.datetime.now(datetime.timezone.utc).strftime('%Y-%m-%d_%H-%M-%S_UTC')
    logfile = Path(output).parent / (Path(script).stem + '_' + stamp + '.log')
    logging.basicConfig(filename=logfile, encoding='utf-8', level=logging.DEBUG,
                        format='[%(asctime)s UTC] [%(levelname)s] [native-fixture] %(message)s',
                        datefmt='%Y-%m-%d %H:%M:%S')
    logging.Formatter.converter = time.gmtime
    sys.argv = [str(SCRIPTS / script), *sys.argv[4:]]
    try:
        runpy.run_path(str(SCRIPTS / script), run_name='__main__')
    finally:
        logging.shutdown()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--archive', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    assert sys.platform == 'linux', 'Native certificate fixtures require Linux'
    output = args.output.resolve()
    assert output.is_relative_to(ROOT / '.ci-work')
    output.mkdir(parents=True, exist_ok=False)
    archive = args.archive.resolve()
    expected = archive.with_name(archive.name + '.sha256').read_text(encoding='utf-8-sig').split()[0]
    assert hashlib.sha256(archive.read_bytes()).hexdigest() == expected
    with tarfile.open(archive, 'r:gz') as bundle:
        members = bundle.getmembers()
        assert not any(m.issym() or m.islnk() for m in members)
        bins = [m for m in members if m.name.endswith('/bin/upwork-connector')]
        assert len(bins) == 1 and bins[0].size < 40 * 1024 * 1024
        binary = output / 'upwork-connector'
        binary.write_bytes(bundle.extractfile(bins[0]).read())
        binary.chmod(0o700)
    jobs = [('cli_probe.py', 'cli', []), ('protocol_probe.py', 'protocol', ['--assert-fixed']),
            ('extended_protocol_probe.py', 'extended', ['--cases', *EXTENDED]),
            ('terminal_sse_probe.py', 'sign-in', ['--mode', 'sign-in']),
            ('terminal_sse_probe.py', 'bridge', ['--mode', 'bridge']),
            ('credential_coordination_probe.py', 'credential-coordination', []),
            ('initialization_ping_probe.py', 'initialization-ping', []),
            ('state_reader_probe.py', 'state-reader', []),
            ('import_scale_probe.py', 'import-scale', []),
            ('route_revision_probe.py', 'route-revision', []),
            ('http_media_probe.py', 'http-media', [])]
    results = []
    for script, name, extra in jobs:
        receipt = output / (name + '.json')
        command = [sys.executable, '-I', str(Path(__file__).resolve()), '--worker', script, str(receipt),
                   '--binary', str(binary), '--output', str(receipt), *extra]
        env = os.environ.copy()
        env.update({'TEMP': str(output), 'TMP': str(output), 'TMPDIR': str(output), 'PYTHONUTF8': '1'})
        child = subprocess.Popen(command, stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                 stderr=subprocess.STDOUT, start_new_session=True, env=env)
        started = time.monotonic()
        try:
            captured, _ = child.communicate(timeout=180)
        except subprocess.TimeoutExpired:
            import signal
            os.killpg(child.pid, signal.SIGKILL)
            captured, _ = child.communicate(timeout=5)
            raise RuntimeError('Native fixture deadline exceeded: ' + name)
        finally:
            if child.poll() is None:
                import signal
                os.killpg(child.pid, signal.SIGKILL)
                child.wait(timeout=5)
        (output / (name + '-controller.log')).write_bytes(captured)
        results.append({'suite': name, 'exit': child.returncode, 'seconds': round(time.monotonic()-started, 3), 'reaped': True})
        (output / 'summary.json').write_text(json.dumps({'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
                                                        'suites': results}, indent=2), encoding='utf-8')
        print(name + ': exit=' + str(child.returncode), flush=True)
        if child.returncode:
            raise SystemExit(child.returncode)
    binary.unlink()


if __name__ == '__main__':
    if len(sys.argv) > 1 and sys.argv[1] == '--worker':
        worker()
    else:
        main()
