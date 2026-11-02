#!/usr/bin/env python3
"""Fetch checksum-pinned MIT dependencies and apply recorded iOS compatibility fixes."""
import hashlib
import io
import json
import pathlib
import shutil
import subprocess
import tarfile
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
PIN = json.loads((ROOT / 'bootstrap/runtime.json').read_text())


def materialize(name, version, checksum, patch_name):
    dest = ROOT / 'build/vendor' / name
    patch = ROOT / 'patches' / patch_name
    expected = {'crate': checksum, 'patch': hashlib.sha256(patch.read_bytes()).hexdigest()}
    marker = dest / '.horizon-patch.json'
    if marker.is_file() and json.loads(marker.read_text()) == expected:
        return
    url = f'https://static.crates.io/crates/{name}/{name}-{version}.crate'
    with urllib.request.urlopen(url, timeout=60) as response:
        archive = response.read()
    if hashlib.sha256(archive).hexdigest() != checksum:
        raise SystemExit(f'{name} checksum mismatch')
    if dest.exists():
        assert dest.parent == ROOT / 'build/vendor' and not dest.is_symlink()
        shutil.rmtree(dest)
    dest.mkdir(parents=True)
    with tarfile.open(fileobj=io.BytesIO(archive), mode='r:gz') as tar:
        for member in tar:
            if not member.isfile():
                continue
            parts = pathlib.PurePosixPath(member.name).parts
            if parts[0] != f'{name}-{version}' or any(p in ('..', '/') for p in parts):
                raise SystemExit('Invalid dependency archive path')
            path = dest.joinpath(*parts[1:])
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(tar.extractfile(member).read())
    subprocess.run(['git', 'apply', f'--directory=build/vendor/{name}', str(patch)],
                   cwd=ROOT, check=True)
    marker.write_text(json.dumps(expected, sort_keys=True))


materialize('ristretto_intrinsics', '0.34.0', PIN['intrinsics_sha256'], 'ristretto.patch')
materialize('sysinfo', '0.39.6', PIN['sysinfo_sha256'], 'sysinfo.patch')
