"""跨平台黑盒回归：仅使用临时 HTTP 服务，不写现网记忆。"""
import http.server
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import time

记录 = []
统计延迟 = 0
class 服务(http.server.BaseHTTPRequestHandler):
    def log_message(self, *参数):
        pass
    def do_POST(self):
        数据 = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        记录.append((self.path, 数据))
        if self.path == '/ask':
            if 数据['query'] == 'slow':
                time.sleep(.5)
            响应 = {'contract':'裁决', 'pack':'唯一证据', 'results':[{'id':7,'content':'唯一证据'}]}
        elif self.path == '/memory/supersede':
            响应 = {'ok':False,'error':'not_found'} if 数据['old_id'] == 999 else {'ok':True, **数据}
        else:
            响应 = {'status':'ok','id':7, **数据}
        self.返回(响应, .5 if 数据.get("query") == "slowbody" else 0)
    def do_GET(self):
        记录.append((self.path,None))
        time.sleep(统计延迟)
        self.返回({'path':self.path})
    def 返回(self, 数据, 延迟=0):
        正文=json.dumps(数据,ensure_ascii=False).encode()
        self.send_response(200)
        self.send_header('Content-Length',str(len(正文)))
        self.end_headers()
        time.sleep(延迟)
        try:
            self.wfile.write(正文)
        except (BrokenPipeError,ConnectionResetError):
            pass

实例=http.server.ThreadingHTTPServer(('127.0.0.1',0),服务)
threading.Thread(target=实例.serve_forever,daemon=True).start()
环境={**os.environ,'RXT_NEBULA_URL':f'http://127.0.0.1:{实例.server_port}','RXT_MEM_NO_SSH':'1','RXT_AGENT':'1'}
程序=str(Path(sys.argv[1]).resolve())
def 调用(*参数, 输入=None, 码=0):
    结果=subprocess.run([程序,'mem',*参数],input=输入,stdout=subprocess.PIPE,stderr=subprocess.PIPE,env=环境,timeout=5)
    assert 结果.returncode==码,(参数,结果.returncode,结果.stderr)
    assert not 结果.stdout.startswith(b'\xef\xbb\xbf'),结果.stdout
    return 结果

正文='第一行 "引号" & %PATH%\r\n第二行\n第三行'
assert json.loads(调用('save',正文).stdout)['content']==正文
assert json.loads(调用('save','--stdin',输入=正文.encode()).stdout)['content']==正文
with tempfile.TemporaryDirectory() as 临时:
    文件=Path(临时)/'记忆.txt';文件.write_bytes(b'\xef\xbb\xbf'+正文.encode())
    assert json.loads(调用('save','--file',str(文件)).stdout)['content']==正文
调用('save',码=2)
调用('save','x','--stdin',码=2)
调用('save','--stdin',输入=b'',码=1)
assert json.loads(调用('search','例子','--json').stdout)['pack']=='唯一证据'
assert 调用('search','例子').stdout.decode().count('唯一证据')==1
assert set(json.loads(调用('stats','--json').stdout))=={'health','stats'}
assert json.loads(调用('supersede','7','--by','8','--reason','纠正').stdout)['new_id']==8
assert 记录[-1][1]=={'old_id':7,'new_id':8,'note':'纠正'}
调用('supersede','999',码=1)
调用('supersede','7','--by','7',码=1)
开始=time.monotonic()
结果=调用('search','slow','--timeout','80',码=124)
assert b'MEM_TIMEOUT' in 结果.stderr
assert time.monotonic()-开始 < 1.5
调用('search','slow','--timeout','0',码=2)
调用('search','slowbody','--timeout','80',码=124)
统计延迟 = .15
调用('stats','--json','--timeout','220',码=124)
实例.shutdown()
print('PASS: multiline argv/file/stdin, BOM, JSON, deduplicated text, supersede payload/errors, timeout exit 124')
