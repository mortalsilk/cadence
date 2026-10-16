#!/usr/bin/env python3
"""Real desktop smoke test, isolated from personal cadence data.
Requires tauri-driver (cargo install tauri-driver), WebKitWebDriver, and KDE/X11.
Set TAURI_DRIVER and WEBKIT_DRIVER to override their paths. Run after building:
  python3 scripts/native-smoke.py src-tauri/target/release/cadence
The test opens a temporary app window, records a session, runs a 1-minute
countdown, verifies recovery, and quits.
It does not physically suspend the computer.
"""
import base64
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

binary = str(Path(sys.argv[1] if len(sys.argv) > 1 else 'src-tauri/target/release/cadence').resolve())
root = Path(tempfile.mkdtemp(prefix='cadence-native-smoke-'))
env = dict(os.environ, XDG_DATA_HOME=str(root / 'data'))
endpoint = 'http://127.0.0.1:4446'
session = None
log = (root / 'driver.log').open('w')
driver = subprocess.Popen([os.environ.get('TAURI_DRIVER', 'tauri-driver'), '--port', '4446', '--native-port', '4447', '--native-driver', os.environ.get('WEBKIT_DRIVER', 'WebKitWebDriver')], env=env, stdout=log, stderr=log)


def request(method, path, data=None):
    req = urllib.request.Request(endpoint + path, method=method, data=json.dumps(data).encode() if data is not None else None, headers={'Content-Type': 'application/json'})
    try:
        with urllib.request.urlopen(req, timeout=35) as response:
            result = json.load(response)['value']
    except urllib.error.HTTPError as error:
        raise RuntimeError(error.read().decode()) from error
    if isinstance(result, dict) and 'error' in result:
        raise RuntimeError(result)
    return result


def execute(script, *args):
    return request('POST', f'/session/{session}/execute/sync', {'script': script, 'args': list(args)})


def invoke(command, **args):
    value = request('POST', f'/session/{session}/execute/async', {'script': "const done=arguments[arguments.length-1]; window.__TAURI_INTERNALS__.invoke(arguments[0],arguments[1]).then(value=>done({ok:true,value})).catch(error=>done({ok:false,error:String(error)}));", 'args': [command, args]})
    assert value['ok'], value
    return value.get('value')


def connect():
    global session
    response = request('POST', '/session', {'capabilities': {'alwaysMatch': {'tauri:options': {'application': binary}}}})
    session = response['sessionId']
    for _ in range(40):
        if execute("return !!document.querySelector('.primary') && !document.querySelector('.primary').disabled"):
            return
        time.sleep(.25)
    raise AssertionError('UI did not become ready')


def menu():
    lines = subprocess.check_output(['busctl', '--user', 'list'], text=True).splitlines()
    for line in lines:
        if line.startswith('io.cadence.desktop.SingleInstance '):
            pid = line.split()[1]
            for candidate in lines:
                parts = candidate.split()
                if parts[0].startswith(':') and len(parts) > 1 and parts[1] == pid:
                    tree = subprocess.run(['busctl','--user','--list','tree',parts[0]],capture_output=True,text=True).stdout
                    for path in tree.splitlines():
                        if path.strip().endswith('/Menu'):
                            return parts[0], path.strip(), pid
    raise AssertionError('Tray menu not registered')


def tray_click(label):
    service, path, _ = menu()
    command = ['gdbus','call','--session','--dest',service,'--object-path',path,'--method']
    layout = subprocess.check_output(command + ['com.canonical.dbusmenu.GetLayout','--','0','-1','[]'], text=True)
    matches = re.findall(r"\((\d+), \{([^{}]+)\}",layout)
    for item, props in matches:
        if f"'{label}'" in props:
            subprocess.check_call(command + ['com.canonical.dbusmenu.Event',item,'clicked','<0>','0'], stdout=subprocess.DEVNULL)
            return
    raise AssertionError(f'Menu item missing: {label}: {layout}')


def screenshot(name):
    target=Path('artifacts') / name
    target.parent.mkdir(exist_ok=True)
    target.write_bytes(base64.b64decode(request('GET',f'/session/{session}/screenshot')))


try:
    for _ in range(40):
        try:
            request('GET','/status'); break
        except Exception:
            time.sleep(.25)
    connect()
    state=invoke('get_state')
    assert state['active'] is None and state['active_session'] is None
    assert execute('return document.documentElement.scrollWidth <= innerWidth')
    screenshot('native-timer.png')
    layout=execute("return {width:innerWidth,height:innerHeight,scrollWidth:document.documentElement.scrollWidth,scrollHeight:document.documentElement.scrollHeight,sections:Array.from(document.querySelector('.app-shell').children).map(node=>({tag:node.tagName,className:node.className,top:Math.round(node.getBoundingClientRect().top),bottom:Math.round(node.getBoundingClientRect().bottom),height:Math.round(node.getBoundingClientRect().height)}))}")
    assert layout['scrollHeight'] <= layout['height'], f'Idle timer does not fit the default window: {layout}'
    invoke('start_session',input={'label':'Native session','description':'Real WebKit and SQLite'})
    invoke('start_stint',mode='count_up',label='First session stint',minutes=None)
    time.sleep(1.2)
    tray_click('Pause / resume timing')
    paused=invoke('get_state')['active']; assert paused['phase']=='paused'
    assert invoke('get_state')['active_session']['phase']=='paused'
    time.sleep(.6)
    assert invoke('get_state')['active']['elapsed_ms']==paused['elapsed_ms']
    tray_click('Pause / resume timing')
    assert invoke('get_state')['active']['phase']=='running'
    # Closing via the window manager exercises the native close handler.
    _,_,pid=menu()
    windows=subprocess.check_output(['wmctrl','-lp'],text=True)
    window=next(line.split()[0] for line in windows.splitlines() if line.split()[2]==pid)
    subprocess.check_call(['wmctrl','-ic',window]); time.sleep(.5)
    assert invoke('get_state')['active']['phase']=='running'
    second=subprocess.run([binary],env=env,timeout=10,capture_output=True)
    assert second.returncode==0
    assert menu()[2]==pid, 'Second launch created a second primary instance'
    tray_click('Open cadence')
    invoke('timer_action',action='finish')
    first=invoke('save_stint',label='First session stint',description='')
    assert first['session_id'] is not None
    invoke('start_stint',mode='count_up',label='Second session stint',minutes=None)
    time.sleep(.4)
    invoke('timer_action',action='finish')
    second_record=invoke('save_stint',label='Second session stint',description='')
    assert second_record['session_id']==first['session_id']
    invoke('session_action',action='end')
    saved_session=invoke('save_session',input={'label':'Native session','description':'Real WebKit and SQLite'})
    records=invoke('list_stints'); assert len(records)==2
    sessions=invoke('list_sessions'); assert sessions==[saved_session]
    invoke('export_file',path=str(root/'backup.json'),format='json')
    invoke('export_file',path=str(root/'stints.csv'),format='csv')
    backup=json.loads((root/'backup.json').read_text())
    assert backup['version']==2 and backup['stints']==records and backup['sessions']==sessions
    assert 'record_type' in (root/'stints.csv').read_text()
    assert invoke('preview_import',path=str(root/'backup.json'))['skipped']==3
    invoke('delete_session',id=saved_session['id'])
    assert all(record['session_id'] is None for record in invoke('list_stints'))
    for record in records:
        invoke('delete_stint',id=record['id'])
    assert invoke('confirm_import')['added']==3
    # Native history render after a fresh navigation/reload.
    request('POST',f'/session/{session}/refresh',{})
    time.sleep(.5)
    execute("Array.from(document.querySelectorAll('nav button')).find(b=>b.textContent.includes('History')).click()")
    time.sleep(.2)
    screenshot('native-history.png')
    assert 'Native session' in execute('return document.body.innerText')
    invoke('start_stint',mode='countdown',label='Countdown smoke test',minutes=1)
    print('Native IPC, tray pause/resume, close/reopen, single instance, SQLite, CSV and JSON round trip passed.',flush=True)
    print('Waiting for the real one-minute countdown to expire…',flush=True)
    deadline=time.monotonic()+65
    while time.monotonic()<deadline:
        state=invoke('get_state')['active']
        if state['phase']=='review':break
        time.sleep(.5)
    assert state['phase']=='review' and state['elapsed_ms']==60000
    assert not invoke('get_state').get('warning'), invoke('get_state')
    invoke('save_stint',label='Countdown smoke test',description='')
    invoke('start_session',input={'label':'Recovery session','description':''})
    invoke('start_stint',mode='count_up',label='Recovery smoke test',minutes=None)
    time.sleep(.7)
    tray_click('Quit cadence'); time.sleep(.5)
    try: request('DELETE',f'/session/{session}')
    except Exception: pass
    session=None
    connect()
    recovered=invoke('get_state')['active']
    assert recovered['phase']=='paused' and recovered['label']=='Recovery smoke test'
    recovered_session=invoke('get_state')['active_session']
    assert recovered_session['phase']=='paused' and recovered_session['label']=='Recovery session'
    assert len(invoke('list_stints'))==3
    invoke('timer_action',action='discard')
    invoke('session_action',action='discard')
    tray_click('Quit cadence')
    print('Countdown completion and session/stint quit-restart persistence passed. No physical suspend was performed.',flush=True)
    print(f'Isolated test data and logs: {root}',flush=True)
finally:
    if session:
        try: request('DELETE',f'/session/{session}')
        except Exception: pass
    driver.terminate()
    try: driver.wait(timeout=10)
    except subprocess.TimeoutExpired: driver.kill()
    log.close()
