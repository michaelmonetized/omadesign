"""Run offline authoring QA on the target, recording resource use and desktop pressure."""
import json,os,pathlib,subprocess,time
root=pathlib.Path(__file__).resolve().parent
def state():
    memory={}
    for line in pathlib.Path('/proc/meminfo').read_text().splitlines():
        key,value=line.split(':',1)
        if key in ['MemTotal','MemAvailable','SwapTotal','SwapFree']:memory[key+'_KiB']=int(value.split()[0])
    return {'memory':memory,'pressure':{key:pathlib.Path('/proc/pressure/'+key).read_text().strip() for key in ['cpu','memory','io']}}
before=state();start=time.monotonic()
args=['unshare','-Urn',str(root/'bin'/'authoring_qa'),str(root/'input'/'infographic-seed.oma'),str(root/'input'/'type-tasks.json'),str(root/'repeat')]
with (root/'repeat.log').open('w') as log:
    p=subprocess.Popen(args,stdout=log,stderr=subprocess.STDOUT)
    _,status,usage=os.wait4(p.pid,0);p.returncode=os.waitstatus_to_exitcode(status)
report={'returncode':p.returncode,'wall_seconds':time.monotonic()-start,'peak_rss_KiB':usage.ru_maxrss,'user_seconds':usage.ru_utime,'system_seconds':usage.ru_stime,'major_page_faults':usage.ru_majflt,'before':before,'after':state(),'limits':'default user-session limits; no per-test memory or CPU ceiling','network':'disabled with unshare -Urn'}
(root/'repeat-resources.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(report),flush=True)
raise SystemExit(p.returncode)
