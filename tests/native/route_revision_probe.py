"""Real bridge revocation across a fully paused observation interval.

All services and credentials are synthetic. TLS terminates on loopback and no
request is forwarded. SIGSTOP/observed-stop/SIGCONT removes scheduling guesses.
"""
from __future__ import annotations
import argparse
import json
import logging
import os
from pathlib import Path
import queue
import signal
import subprocess
import sys
import tempfile
import threading
import time
import urllib.parse

sys.dont_write_bytecode = True
import protocol_probe as protocol
from terminal_sse_probe import execute

LOG = logging.getLogger('route-revision')

class Handler(protocol.Handler):
    def do_POST(self):
        if urllib.parse.urlsplit(self.path).path != '/mcp':
            return super().do_POST()
        length = int(self.headers.get('Content-Length', '0'))
        if not 0 < length <= 1024 * 1024:
            return self.reply(None, 400)
        request = json.loads(self.rfile.read(length))
        method = request.get('method')
        if method == 'initialize':
            return self.reply({'jsonrpc':'2.0','id':request['id'],'result':{
                'protocolVersion':'2025-11-25','capabilities':{},'serverInfo':{'name':'revision-fixture','version':'1'}}})
        if method == 'notifications/initialized':
            self.reply(None, 202)
            self.server.ready.set()
            return
        if method == 'ping':
            self.server.ping_calls += 1
            return self.reply({'jsonrpc':'2.0','id':request['id'],'result':{}})
        return self.reply(None, 400)

class Fixture(protocol.Fixture):
    def __init__(self, root):
        super().__init__(root, 'valid')
        self.RequestHandlerClass = Handler
        self.ready = threading.Event()
        self.ping_calls = 0

def command(binary, root, args, payload=None):
    completed = subprocess.run([str(binary),'--root',str(root),*args], input=payload,
                               stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=5)
    if completed.returncode:
        raise RuntimeError('An isolated route command failed.')

def receive(lines, child, request_id, budget):
    end = time.monotonic() + budget
    while time.monotonic() < end:
        try:
            line = lines.get(timeout=.02)
        except queue.Empty:
            if child.poll() is not None:
                return False
            continue
        try:
            message = json.loads(line)
        except ValueError:
            continue
        if message.get('id') == request_id and 'result' in message:
            return True
    return False

def paused(child):
    os.kill(child.pid, signal.SIGSTOP)
    deadline = time.monotonic() + 3
    while time.monotonic() < deadline:
        pid, status = os.waitpid(child.pid, os.WNOHANG | os.WUNTRACED)
        if pid:
            if os.WIFSTOPPED(status):
                return
            raise RuntimeError('Bridge exited before the observation barrier.')
        time.sleep(.005)
    raise RuntimeError('The owned bridge did not reach its stop barrier.')

def run_case(binary, ca, output, kind):
    with tempfile.TemporaryDirectory(prefix='route-revision-', dir=output) as tmp:
        root = Path(tmp)
        (root/'config').mkdir()
        server = Fixture(ca)
        thread = threading.Thread(target=server.serve_forever, kwargs={'poll_interval':.02}, daemon=True)
        thread.start()
        (root/'config/connections.json').write_text(json.dumps({'version':1,'mode':'proxy','selected':'fixture',
            'profiles':[{'name':'fixture','url':f'http://127.0.0.1:{server.server_port}'}]}), encoding='utf-8')
        child = None
        readers = []
        stopped = False
        try:
            preparation = execute(binary, root, ca, 'sign-in')
            if preparation['returncode'] != 0 or not preparation['completion_claimed']:
                raise RuntimeError('Synthetic authorization preparation failed.')
            server.ready.clear()
            env = os.environ.copy()
            env.update(SSL_CERT_FILE=str(ca/'ca.pem'), SSL_CERT_DIR=str(ca/'empty'))
            for key in ('TEMP','TMP','TMPDIR'):
                env[key] = str(root)
            child = subprocess.Popen([str(binary),'--root',str(root),'bridge'], stdin=subprocess.PIPE,
                                     stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True, encoding='utf-8', env=env)
            lines = queue.Queue()
            def read(stream, forward):
                for line in stream:
                    if forward:
                        lines.put(line)
            readers = [threading.Thread(target=read,args=(child.stdout,True),daemon=True),
                       threading.Thread(target=read,args=(child.stderr,False),daemon=True)]
            for reader in readers:
                reader.start()
            child.stdin.write(json.dumps({'jsonrpc':'2.0','id':71,'method':'initialize','params':{
                'protocolVersion':'2025-11-25','capabilities':{},'clientInfo':{'name':'revision-fixture','version':'1'}}})+'\n')
            child.stdin.flush()
            if not receive(lines, child, 71, 5):
                raise RuntimeError('Synthetic bridge initialization failed.')
            child.stdin.write(json.dumps({'jsonrpc':'2.0','method':'notifications/initialized'})+'\n')
            child.stdin.flush()
            if not server.ready.wait(3):
                raise RuntimeError('Bridge readiness was not observed.')
            paused(child)
            stopped = True
            if kind == 'block_then_restore':
                command(binary, root, ['block'])
                command(binary, root, ['select','fixture'])
            else:
                command(binary, root, ['add'], json.dumps({'name':'inactive','url':'http://127.0.0.1:1'}).encode())
            child.stdin.write(json.dumps({'jsonrpc':'2.0','id':73,'method':'ping'})+'\n')
            child.stdin.flush()
            os.kill(child.pid, signal.SIGCONT)
            stopped = False
            answered = receive(lines, child, 73, 3)
            exited = child.poll() is not None
            if kind == 'block_then_restore' and not exited:
                try:
                    child.wait(timeout=3)
                    exited = True
                except subprocess.TimeoutExpired:
                    pass
            result = {'case':kind,'stop_barrier_observed':True,'queued_ping_answered':answered,
                      'outbound_ping_requests':server.ping_calls,'bridge_exited':exited,
                      'exit_nonzero':child.returncode is not None and child.returncode != 0}
            result['passed'] = (exited and result['exit_nonzero'] and not answered and server.ping_calls == 0
                                if kind == 'block_then_restore' else not exited and answered and server.ping_calls == 1)
            LOG.info('Case=%s passed=%s outbound_ping_requests=%d',kind,result['passed'],server.ping_calls)
            return result
        finally:
            if child is not None:
                if stopped and child.poll() is None:
                    os.kill(child.pid, signal.SIGCONT)
                if child.poll() is None:
                    child.kill()
                child.wait(timeout=3)
                for reader in readers:
                    reader.join(timeout=1)
                for stream in (child.stdin,child.stdout,child.stderr):
                    if stream and not stream.closed:
                        stream.close()
            server.shutdown()
            server.server_close()
            thread.join(timeout=1)

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--binary',type=Path,required=True)
    parser.add_argument('--output',type=Path,required=True)
    parser.add_argument('--expect-baseline',action='store_true')
    args = parser.parse_args()
    logging.basicConfig(level=logging.INFO,format='%(asctime)s [%(levelname)s] [route-revision] %(message)s')
    args.output.parent.mkdir(parents=True,exist_ok=True)
    if sys.platform != 'linux':
        raise RuntimeError('The stop-barrier native fixture requires Linux.')
    LOG.debug('Using an observed process-stop barrier, local TLS and disposable state.')
    with tempfile.TemporaryDirectory(prefix='route-cert-',dir=args.output.parent) as tmp:
        ca = Path(tmp)
        protocol.certificates(ca)
        (ca/'empty').mkdir()
        results = [run_case(args.binary.resolve(),ca,args.output.parent,kind)
                   for kind in ('block_then_restore','inactive_profile_edit')]
    args.output.write_text(json.dumps({'cases':len(results),'passed':sum(row['passed'] for row in results),'results':results},indent=2)+'\n',encoding='utf-8')
    accepted = all(row['passed'] for row in results)
    if args.expect_baseline:
        cycle, control = results
        accepted = (not cycle['passed'] and cycle['stop_barrier_observed'] and not cycle['bridge_exited']
                    and cycle['queued_ping_answered'] and cycle['outbound_ping_requests'] == 1 and control['passed'])
    LOG.warning('This fixture never forwards requests to Upwork and does not use real credentials.')
    if not accepted:
        LOG.error('Route transition contract failed; no unexecuted case is counted.')
        raise SystemExit(1)
    LOG.info('Baseline unobserved-cycle defect reproduced.' if args.expect_baseline else 'Both native route contracts passed.')

if __name__ == '__main__':
    try:
        main()
    except Exception:
        LOG.error('Bounded route fixture aborted; no private payload is logged.')
        raise
