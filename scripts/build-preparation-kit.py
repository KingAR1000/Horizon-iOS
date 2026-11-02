#!/usr/bin/env python3
"""Compile Horizon-owned exporters against verified Mojang inputs, outside the distributed app."""
import concurrent.futures,hashlib,json,os,pathlib,subprocess,urllib.request
root=pathlib.Path(__file__).resolve().parent.parent
pin=json.loads((root/'engine/research-lock.json').read_text())['minecraft']
version=pin['version']
inputs=root/'build/compilation-inputs'
def get(url):
    with urllib.request.urlopen(url,timeout=60) as response:return response.read()
manifest=get(f"https://piston-meta.mojang.com/v1/packages/{pin['manifest_sha1']}/{version}.json")
if hashlib.sha1(manifest).hexdigest()!=pin['manifest_sha1']:raise SystemExit('Version metadata checksum mismatch')
data=json.loads(manifest)
version_dir=inputs/'versions'/version
version_dir.mkdir(parents=True,exist_ok=True)
(version_dir/(version+'.json')).write_bytes(manifest)
tasks=[]
for side in ('client','server'):
    d=data['downloads'][side]
    if d['sha1']!=pin[side+'_sha1']:raise SystemExit('Pinned Minecraft jar changed')
    tasks.append((d,version_dir/(side+'.jar')))
for library in data['libraries']:
    artifact=library.get('downloads',{}).get('artifact')
    if not artifact:continue
    relative=pathlib.PurePosixPath(artifact['path'])
    if relative.is_absolute() or '..' in relative.parts:raise SystemExit('Invalid library path')
    tasks.append((artifact,inputs/'libraries'/relative))
index=data['assetIndex'];tasks.append((index,inputs/'assets/indexes'/(index['id']+'.json')))
def download(task):
    entry,path=task
    if path.is_file() and path.stat().st_size==entry['size'] and hashlib.sha1(path.read_bytes()).hexdigest()==entry['sha1']:return
    content=get(entry['url'])
    if len(content)!=entry['size'] or hashlib.sha1(content).hexdigest()!=entry['sha1']:raise RuntimeError('Compilation input checksum mismatch')
    path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(content)
with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:list(pool.map(download,tasks))
env=dict(os.environ)
# This is a host build tool; the app itself is subsequently compiled against the iOS SDK.
if os.uname().sysname=='Darwin':env['SDKROOT']=subprocess.check_output(['xcrun','--sdk','macosx','--show-sdk-path'],text=True).strip()
subprocess.run(['cargo','run','--locked','--release','-p','horizon-ios-kit','--',str(root/'engine'),str(inputs),str(root/'build/PrepareKit')],cwd=root,env=env,check=True)
# Fail closed if anything other than the original classes/sources and their pin data leaked in.
for path in (root/'build/PrepareKit').rglob('*'):
    if not path.is_file():continue
    relative=path.relative_to(root/'build/PrepareKit').as_posix()
    allowed=(relative=='research-lock.json' or relative=='crates/horizon-prepare/src/exporters.rs' or relative.startswith('mods/horizon/src/test/java/dev/horizonlab/parity/') and path.suffix=='.java' or relative=='exporters/manifest.json' or any(relative.startswith('exporters/'+side+'/dev/horizonlab/parity/') for side in ('client','server')) and path.suffix=='.class')
    if not allowed:raise SystemExit('Unexpected file in preparation kit: '+relative)
