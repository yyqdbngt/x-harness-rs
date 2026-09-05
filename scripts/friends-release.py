"""Fork-owned release contract; no private material is read by this module."""
import datetime
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys

REPOSITORY = 'yyqdbngt/x-harness-rs'
PREFIX = 'friends-v'


def version(value):
    if not re.fullmatch(r'(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)', value):
        raise ValueError('Expected plain major.minor.patch version')
    parts = tuple(map(int, value.split('.')))
    if max(parts) > 65535:
        raise ValueError('Version exceeds Windows installer limits')
    return parts


def plan(repository, tag, releases):
    if repository != REPOSITORY:
        raise ValueError('This channel belongs only to ' + REPOSITORY)
    if not tag.startswith(PREFIX):
        raise ValueError('Wrong release tag prefix')
    target = tag[len(PREFIX):]
    parts = version(target)
    if any(r['tagName'] == tag for r in releases):
        raise ValueError('Release already exists; never overwrite a published or draft version')
    published = [r for r in releases if not r['isDraft'] and r['tagName'].startswith(PREFIX)]
    if any(version(r['tagName'][len(PREFIX):]) >= parts for r in published):
        raise ValueError('Release must be newer than every published channel version')
    versions = [target]
    if not published:
        if parts[2] == 0:
            raise ValueError('First release needs patch >= 1 for a lower bootstrap version')
        versions.insert(0, f'{parts[0]}.{parts[1]}.{parts[2] - 1}')
    return {'version': target, 'versions': versions,
            'endpoint': f'https://github.com/{repository}/releases/latest/download/latest.json'}


def releases():
    return json.loads(subprocess.check_output(
        ['gh', 'release', 'list', '--repo', REPOSITORY, '--limit', '1000',
         '--json', 'tagName,isDraft'], text=True))


def manifest(repository, tag, root):
    if repository != REPOSITORY or not tag.startswith(PREFIX):
        raise ValueError('Wrong release destination')
    target = tag[len(PREFIX):]
    version(target)
    name = f'XHarness_{target}_x64-setup.exe'
    package = root / name
    if not package.is_file() or package.stat().st_size == 0:
        raise ValueError('Missing installer')
    signature = (root / (name + '.sig')).read_text(encoding='utf-8').strip()
    if not signature:
        raise ValueError('Missing installer signature')
    return {'version': target,
            'notes': '同学分发通道。更新包已签名；未购买 Windows 发布者证书。安装前请保存任务，确认重启会停止 Agent 和后台命令。',
            'pub_date': datetime.datetime.now(datetime.timezone.utc).strftime('%Y-%m-%dT%H:%M:%SZ'),
            'platforms': {'windows-x86_64': {
                'signature': signature,
                'url': f'https://github.com/{repository}/releases/download/{tag}/{name}'}}}


if __name__ == '__main__':
    command, tag = sys.argv[1:3]
    repository = os.environ['GITHUB_REPOSITORY']
    if command == 'plan':
        result = plan(repository, tag, releases())
        with open(os.environ['GITHUB_ENV'], 'a', encoding='utf-8') as output:
            output.write('RELEASE_VERSION=' + result['version'] + '\n')
            output.write('BUILD_VERSIONS=' + ','.join(result['versions']) + '\n')
            output.write('XHARNESS_UPDATER_ENDPOINT=' + result['endpoint'] + '\n')
    elif command == 'manifest':
        root = Path(sys.argv[3])
        result = manifest(repository, tag, root)
        (root / 'latest.json').write_text(json.dumps(result, ensure_ascii=False, indent=2) + '\n', encoding='utf-8')
        files = sorted(p for p in root.iterdir() if p.is_file() and p.name != 'SHA256SUMS')
        (root / 'SHA256SUMS').write_text(''.join(hashlib.sha256(p.read_bytes()).hexdigest() + '  ' + p.name + '\n' for p in files), encoding='ascii')
    else:
        raise ValueError('Unknown command')
