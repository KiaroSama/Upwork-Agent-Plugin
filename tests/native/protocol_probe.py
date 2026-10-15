"""Exercise the real native executable against a synthetic TLS-only loopback proxy.

No request is forwarded to the internet. Trust overrides apply only to each test child.
Generated credentials and certificate keys are disposable and are never logged.
"""
from __future__ import annotations
import argparse
import datetime as dt
import http.client
import http.server
import json
import logging
import os
from pathlib import Path
import queue
import ssl
import subprocess
import tempfile
import threading
import time
import urllib.parse
from cryptography import x509
from cryptography.x509.oid import NameOID
from cryptography.hazmat.primitives import hashes, serialization
from cryptography.hazmat.primitives.asymmetric import rsa

LOG = logging.getLogger('protocol-probe')
ENDPOINT = 'https://mcp.upwork.com/mcp'
ISSUER = 'https://mcp.upwork.com'

def certificates(root: Path) -> tuple[Path, Path]:
    key = rsa.generate_private_key(public_exponent=65537, key_size=2048)
    name = x509.Name([x509.NameAttribute(NameOID.COMMON_NAME, 'Disposable Upwork Audit Fixture')])
    now = dt.datetime.now(dt.timezone.utc)
    ca = (x509.CertificateBuilder().subject_name(name).issuer_name(name).public_key(key.public_key())
          .serial_number(x509.random_serial_number()).not_valid_before(now-dt.timedelta(days=1))
          .not_valid_after(now+dt.timedelta(days=2)).add_extension(x509.BasicConstraints(ca=True,path_length=None),critical=True)
          .sign(key,hashes.SHA256()))
    leaf_key = rsa.generate_private_key(public_exponent=65537,key_size=2048)
    leaf = (x509.CertificateBuilder().subject_name(x509.Name([x509.NameAttribute(NameOID.COMMON_NAME,'mcp.upwork.com')]))
            .issuer_name(name).public_key(leaf_key.public_key()).serial_number(x509.random_serial_number())
            .not_valid_before(now-dt.timedelta(days=1)).not_valid_after(now+dt.timedelta(days=2))
            .add_extension(x509.SubjectAlternativeName([x509.DNSName('mcp.upwork.com')]),critical=False)
            .add_extension(x509.BasicConstraints(ca=False,path_length=None),critical=True)
            .add_extension(x509.ExtendedKeyUsage([x509.oid.ExtendedKeyUsageOID.SERVER_AUTH]),critical=False)
            .sign(key,hashes.SHA256()))
    ca_path=root/'ca.pem'; ca_path.write_bytes(ca.public_bytes(serialization.Encoding.PEM))
    cert=root/'leaf.pem'; cert.write_bytes(leaf.public_bytes(serialization.Encoding.PEM))
    private=root/'leaf.key'; private.write_bytes(leaf_key.private_bytes(serialization.Encoding.PEM,serialization.PrivateFormat.PKCS8,serialization.NoEncryption())); private.chmod(0o600)
    return ca_path, private

class Fixture(http.server.ThreadingHTTPServer):
    daemon_threads=True
    block_on_close=False
    def __init__(self, root: Path, case: str):
        self.case=case; self.events=[]; self.initialized=0; self.ping_replies=0
        self.context=ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
        self.context.load_cert_chain(root/'leaf.pem',root/'leaf.key')
        super().__init__(('127.0.0.1',0),Handler)

class Handler(http.server.BaseHTTPRequestHandler):
    protocol_version='HTTP/1.1'
    def log_message(self, *_args): pass
    def do_CONNECT(self):
        if self.path!='mcp.upwork.com:443':
            self.send_error(403); return
        self.send_response(200); self.end_headers(); self.wfile.flush()
        self.connection=self.server.context.wrap_socket(self.connection,server_side=True)
        self.rfile=self.connection.makefile('rb'); self.wfile=self.connection.makefile('wb')
        self.close_connection=False
    def reply(self, value=None, status=200, kind='application/json'):
        body=b'' if value is None else json.dumps(value).encode()
        self.send_response(status); self.send_header('Content-Type',kind)
        self.send_header('Content-Length',str(len(body))); self.end_headers()
        self.wfile.write(body); self.wfile.flush()
    def do_GET(self):
        path=urllib.parse.urlsplit(self.path).path
        self.server.events.append(('GET',path))
        if 'oauth-protected-resource' in path:
            self.reply({'resource':ENDPOINT,'authorization_servers':[ISSUER]})
        elif 'oauth-authorization-server' in path or 'openid-configuration' in path:
            self.reply({'issuer':ISSUER,'authorization_endpoint':ISSUER+'/authorize','token_endpoint':ISSUER+'/token','registration_endpoint':ISSUER+'/register','response_types_supported':['code'],'grant_types_supported':['authorization_code','refresh_token'],'code_challenge_methods_supported':['S256'],'token_endpoint_auth_methods_supported':['none']})
        elif path=='/mcp': self.reply(None,405)
        else: self.reply({'error':'fixture route unavailable'},404)
    def do_DELETE(self): self.reply(None,204)
    def do_POST(self):
        path=urllib.parse.urlsplit(self.path).path
        data=self.rfile.read(int(self.headers.get('Content-Length','0')))
        self.server.events.append(('POST',path))
        if path=='/register':
            request=json.loads(data)
            self.reply({'client_id':'fixture-client','redirect_uris':request['redirect_uris'],'token_endpoint_auth_method':'none'},201)
        elif path=='/token':
            self.reply({'access_token':'disposable-fixture-token','token_type':'Bearer','expires_in':3600,'refresh_token':'disposable-fixture-refresh'})
        elif path=='/mcp':
            request=json.loads(data); method=request.get('method')
            if method=='notifications/initialized':
                self.server.initialized+=1; self.reply(None,500 if self.server.case=='notification_fail' else 202); return
            if method is None:
                self.server.ping_replies+=1; self.reply(None,202); return
            result={'protocolVersion':'2025-11-25','capabilities':{},'serverInfo':{'name':'fixture','version':'1'}}
            if self.server.case=='null_info':result['serverInfo']=None
            elif self.server.case=='wrong_version':result['protocolVersion']='2099-01-01'
            elif self.server.case=='missing_version':result.pop('protocolVersion')
            elif self.server.case=='missing_capabilities':result.pop('capabilities')
            elif self.server.case=='bad_info':result['serverInfo']={}
            elif self.server.case=='missing_info':result.pop('serverInfo')
            elif self.server.case=='numeric_info':result['serverInfo']=42
            elif self.server.case=='bad_name':result['serverInfo']['name']=False
            elif self.server.case=='bad_capabilities':result['capabilities']={'tools':{'listChanged':'invalid'}}
            elif self.server.case=='null_capabilities':result['capabilities']=None
            elif self.server.case=='array_capabilities':result['capabilities']=[]
            elif self.server.case=='null_version':result['protocolVersion']=None
            elif self.server.case=='empty_version':result['protocolVersion']=''
            elif self.server.case=='numeric_version':result['protocolVersion']=1
            elif self.server.case=='stateless_version':result['protocolVersion']='2026-07-28'
            elif self.server.case.startswith('version_'):result['protocolVersion']=self.server.case.removeprefix('version_')
            elif self.server.case=='bad_instructions':result['instructions']=123
            elif self.server.case=='optional_metadata':
                result['instructions']='fixture';result['serverInfo']['title']='Fixture';result['capabilities']={'tools':{},'experimental':{'fixture':{}}}
            response={'jsonrpc':'2.0','id':request['id'],'result':result}
            if self.server.case=='wrong_id':response['id']=999
            elif self.server.case=='string_id':response['id']='1'
            elif self.server.case=='wrong_string_id':response['id']='999'
            elif self.server.case=='missing_id':response.pop('id')
            elif self.server.case=='null_result':response['result']=None
            elif self.server.case=='empty_result':response['result']={}
            elif self.server.case=='invalid_jsonrpc':response['jsonrpc']='1.0'
            elif self.server.case=='error':response={'jsonrpc':'2.0','id':request['id'],'error':{'code':-32602,'message':'fixture rejection'}}
            if self.server.case in ('log_first','ping_first'):
                first=({'jsonrpc':'2.0','method':'notifications/message','params':{'level':'info','data':'fixture'}} if self.server.case=='log_first' else {'jsonrpc':'2.0','id':'fixture-ping','method':'ping'})
                payload=''.join('event: message\ndata: '+json.dumps(v)+'\n\n' for v in [first,response]).encode()
                self.send_response(200);self.send_header('Content-Type','text/event-stream');self.send_header('Content-Length',str(len(payload)));self.end_headers();self.wfile.write(payload);self.wfile.flush()
            else:self.reply(response)
        else:self.reply(None,404)

def run_case(binary: Path, ca_dir: Path, case: str, work: Path) -> dict:
    with tempfile.TemporaryDirectory(prefix='protocol-',dir=work) as tmp:
        root=Path(tmp);(root/'config').mkdir()
        server=Fixture(ca_dir,case)
        thread=threading.Thread(target=server.serve_forever,kwargs={'poll_interval':0.05},daemon=True);thread.start()
        (root/'config/connections.json').write_text(json.dumps({'version':1,'mode':'proxy','selected':'fixture','profiles':[{'name':'fixture','url':f'http://127.0.0.1:{server.server_port}'}]}),encoding='utf-8')
        env=os.environ.copy();env['SSL_CERT_FILE']=str(ca_dir/'ca.pem');env['SSL_CERT_DIR']=str(ca_dir/'empty');(ca_dir/'empty').mkdir(exist_ok=True)
        for key in ('TEMP','TMP','TMPDIR'):env[key]=str(root)
        lines=queue.Queue(); out=[];err=[]
        child=subprocess.Popen([str(binary),'--root',str(root),'sign-in'],stdin=subprocess.DEVNULL,stdout=subprocess.PIPE,stderr=subprocess.PIPE,text=True,encoding="utf-8",env=env)
        def collect(stream, records, forward=False):
            for line in stream:
                records.append(line)
                if forward:lines.put(line)
        readers=[threading.Thread(target=collect,args=(child.stdout,out,True),daemon=True),threading.Thread(target=collect,args=(child.stderr,err),daemon=True)]
        for reader in readers:reader.start()
        callback_sent=False;timed_out=False;started=time.monotonic()
        try:
            deadline=time.monotonic()+8
            while time.monotonic()<deadline and child.poll() is None:
                try:line=lines.get(timeout=.1)
                except queue.Empty:continue
                if line.startswith('https://'):
                    params=urllib.parse.parse_qs(urllib.parse.urlsplit(line.strip()).query)
                    redirect=urllib.parse.urlsplit(params['redirect_uri'][0])
                    assert redirect.hostname in ('localhost','127.0.0.1')
                    connection=http.client.HTTPConnection(redirect.hostname,redirect.port,timeout=3)
                    query=urllib.parse.urlencode({'code':'fixture-code','state':params['state'][0],'iss':ISSUER})
                    connection.request('GET',redirect.path+'?'+query)
                    response=connection.getresponse();response.read();connection.close();callback_sent=True;break
            try:child.wait(timeout=5)
            except subprocess.TimeoutExpired:timed_out=True;child.kill();child.wait(timeout=3)
        finally:
            if child.poll() is None:child.kill();child.wait(timeout=3)
            for reader in readers:reader.join(timeout=1)
            server.shutdown();server.server_close();thread.join(timeout=1)
        result={'case':case,'returncode':child.returncode,'callback_sent':callback_sent,'timeout':timed_out,'initialized_sent':server.initialized,'ping_replies':server.ping_replies,'completion_claimed':any('Sign-in complete.' in line for line in out),'elapsed_seconds':round(time.monotonic()-started,3),'request_counts':len(server.events)}
        # Do not export authorization links, state, headers, tokens, or upstream diagnostics.
        result['safe_error_class']='none' if child.returncode==0 else 'native_safe_failure'
        if not callback_sent:result['safe_error_class']='fixture_setup_or_authentication_failure'
        LOG.info('Case %s: exit=%s callback=%s initialized=%s completion=%s',case,child.returncode,callback_sent,server.initialized,result['completion_claimed'])
        return result

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--binary',type=Path,required=True);parser.add_argument('--output',type=Path,required=True);parser.add_argument('--cases',nargs='+',default=['valid','wrong_id','wrong_version','missing_version','missing_capabilities','null_info','bad_info','error','log_first','ping_first','string_id','wrong_string_id','missing_info','numeric_info','bad_name','bad_capabilities','null_capabilities','array_capabilities','null_version','empty_version','numeric_version','stateless_version','bad_instructions','optional_metadata','version_2024-11-05','version_2025-03-26','version_2025-06-18','notification_fail','null_result','empty_result'])
    parser.add_argument('--assert-fixed',action='store_true')
    args=parser.parse_args();logging.basicConfig(level=logging.INFO,format='%(asctime)s [%(levelname)s] %(message)s')
    args.output.parent.mkdir(parents=True,exist_ok=True)
    LOG.debug('Using ephemeral certificates and per-process test trust only.')
    try:
        with tempfile.TemporaryDirectory(prefix='protocol-certs-',dir=args.output.parent) as tmp:
            certificates(Path(tmp))
            results=[run_case(args.binary.resolve(),Path(tmp),case,args.output.parent) for case in args.cases]
        args.output.write_text(json.dumps(results,indent=2)+'\n',encoding='utf-8')
        LOG.warning('This synthetic TLS test is not evidence of live Upwork account compatibility.')
        LOG.info('Completed %d bounded cases.',len(results))
        if args.assert_fixed:
            positive={'valid','log_first','ping_first','string_id','optional_metadata','version_2024-11-05','version_2025-03-26','version_2025-06-18'}
            failures=[]
            for result in results:
                name=result['case'];success=name in positive
                expected_ready=1 if success or name=='notification_fail' else 0
                passed=(result['callback_sent'] and not result['timeout'] and (result['returncode']==0)==success and result['completion_claimed']==success and result['initialized_sent']==expected_ready)
                if not passed:failures.append(name)
            if failures:
                LOG.error('Fixed-contract violations in cases: %s', ', '.join(failures));raise SystemExit(1)
            LOG.info('Every final sign-in contract and readiness-count assertion passed.')
    except Exception:
        LOG.error('Fixture execution failed; no successful verification claimed.');raise
if __name__=='__main__':main()
