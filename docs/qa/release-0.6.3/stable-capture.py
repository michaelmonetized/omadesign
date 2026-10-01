import os,subprocess,time,json,signal
from pathlib import Path
base=Path.home()/'.local/state/omadesign/qa/release-0.6.3';out=base/'stable';out.mkdir()
fleet=Path.home()/'.local/state/omadesign/qa/fleet-performance-0.6.1';prefix=fleet/'sway-prefix';runtime=Path('/run/user')/str(os.getuid())/f'oma063-stable-{os.getpid()}';runtime.mkdir(mode=0o700)
env=os.environ.copy()
for key in ['DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','LD_LIBRARY_PATH','LD_PRELOAD','ORT_DYLIB_PATH','OMADESIGN_ORT_LIBRARY']:env.pop(key,None)
for key,child in [('XDG_CONFIG_HOME','config'),('XDG_DATA_HOME','data'),('XDG_CACHE_HOME','cache'),('XDG_STATE_HOME','state')]:
 p=out/child;p.mkdir();env[key]=str(p)
senv=env|{'XDG_RUNTIME_DIR':str(runtime),'WLR_BACKENDS':'headless','WLR_RENDERER':'gles2','WLR_RENDER_DRM_DEVICE':'/dev/dri/renderD128','LD_LIBRARY_PATH':str(prefix/'usr/lib')}
sway=None;app=None
try:
 with (out/'display.log').open('w') as log:sway=subprocess.Popen([str(prefix/'usr/bin/sway'),'-c',str(fleet/'sway-headless.conf')],env=senv,stdout=log,stderr=log,start_new_session=True)
 for _ in range(100):
  sockets=list(runtime.glob('sway-ipc.*.sock'));displays=[p for p in runtime.glob('wayland-*') if not p.name.endswith('.lock')]
  if sockets and displays:break
  time.sleep(.05)
 assert sockets and displays
 env.update(XDG_RUNTIME_DIR=str(runtime),WAYLAND_DISPLAY=displays[0].name,SWAYSOCK=str(sockets[0]),WINIT_UNIX_BACKEND='wayland')
 binary=base/'package/prefix/bin/omadesign';seed=base/'package/roundtrip.oma'
 with (out/'app.log').open('w') as log:app=subprocess.Popen(['unshare','-Urn',str(binary),'--desktop-child',str(seed)],env=env,stdout=log,stderr=log,start_new_session=True)
 time.sleep(8)
 assert app.poll() is None
 subprocess.run(['grim','-g','80,50 1440x900',str(out/'native-settled.png')],env=env,check=True,timeout=10)
 subprocess.run([str(prefix/'usr/bin/swaymsg'),'-s',str(sockets[0]),'[app_id="omadesign"] kill'],env=senv,check=True,capture_output=True,timeout=5)
 try:app.wait(timeout=5)
 except subprocess.TimeoutExpired:os.killpg(app.pid,signal.SIGTERM);app.wait(timeout=5)
finally:
 if app and app.poll() is None:os.killpg(app.pid,signal.SIGTERM);app.wait(timeout=5)
 if sway and sway.poll() is None:
  subprocess.run([str(prefix/'usr/bin/swaymsg'),'-s',str(sockets[0]),'exit'],env=senv,capture_output=True,timeout=5);sway.wait(timeout=5)
 (out/'cleanup.json').write_text(json.dumps({'application_exited':app is not None and app.poll() is not None,'private_compositor_exited':sway is not None and sway.poll() is not None,'scope':'Unmodified installed production UI after eight seconds, isolated profile and private GPU display; network disabled'},indent=2)+'\n')
