#!/usr/bin/env python3
"""Exercise the real native S3 create-only contract, then stop/start its owned storage."""
import argparse
import importlib.util
import json
import pathlib
import secrets
import time

SPEC = importlib.util.spec_from_file_location('local_manage', pathlib.Path(__file__).with_name('manage.py'))
m = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(m)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--instance', default='default')
    parser.add_argument('--tag', default='current')
    parser.add_argument('--retained', type=pathlib.Path, help='Verify an existing owned proof after a separate full restart/restore')
    args = parser.parse_args()
    m.require(bool(args.tag) and all(c in 'abcdefghijklmnopqrstuvwxyz0123456789-.' for c in args.tag), 'Invalid image tag')
    path, owner = m.instance(args.instance)
    m.owned(path, owner)
    m.installed_images(owner['images'], owner)
    image = m.inspect('image', 'zoen-local-s3-proof:' + args.tag)
    m.require(image is not None, 'Build the native S3 proof image first')
    nonce = secrets.token_hex(16)
    out = path / 'proofs' / ('s3-' + nonce)
    out.mkdir(parents=True, mode=0o700)
    name = owner['project'] + '-proof-' + nonce[:8]
    m.require(m.inspect('container', name) is None, 'Proof container name already exists')
    base = m.DOCKER + ['run', '--name', name, '--rm', '--pull', 'never', '--network', owner['project'] + '_private', '--label', 'purpose=s3-proof', '--label', 'app=zoen.local', '--label', 'owner=' + owner['nonce'], '--platform', 'linux/amd64', '--read-only', '--tmpfs', '/tmp', '--memory', '128m', '--cpus', '1', '--env-file', str(path / 'storage.env'), '-e', 'ZOEN_S3_BUCKET=encrypted', '-e', 'AWS_ENDPOINT_URL_S3=http://storage:8333', '-e', 'AWS_ALLOW_HTTP=true', '-e', 'AWS_REGION=us-east-1', '-v', str(out) + ':/proof', image['Id']]
    receipt = {'schema': 1, 'purpose': 'native-s3-contract', 'instance_nonce': owner['nonce'], 'proof_image': image['Id'], 'started_ms': int(time.time() * 1000)}
    try:
        if args.retained:
            retained = args.retained.resolve()
            m.require(retained.is_relative_to(m.ROOT.resolve()) and not retained.is_symlink(), 'Use an owned retained proof directory')
            m.require(retained.parent.name == 'proofs', 'Use the exact instance proof directory')
            _, original_owner = m.instance(retained.parent.parent.name)
            before = json.loads((retained / 'lifecycle.json').read_text())
            m.require(before['instance_nonce'] == original_owner['nonce'], 'Retained proof owner differs')
            m.require(set(before['files']) == {'plan.json', 'create.json', 'verify.json'}, 'Retained proof inventory differs')
            m.require(before['purpose'] == 'native-s3-contract' and before.get('passed') is True, 'Retained proof must already have passed')
            for filename, digest in before['files'].items():
                m.require(filename in ['plan.json', 'create.json', 'verify.json'] and m.sha256(retained / filename) == digest, 'Retained proof hash differs')
            for filename in ['plan.json', 'create.json']:
                (out / filename).write_bytes((retained / filename).read_bytes())
            m.command(base + ['verify', '/proof/plan.json', '/proof/create.json', '/proof/verify.json'], timeout=60)
            receipt['retained_from'] = str(retained)
        else:
            m.command(base + ['create', nonce, '/proof/plan.json', '/proof/create.json'], timeout=60)
            ids = m.command(m.compose(path, owner) + ['ps', '-a', '-q', 'storage']).strip().splitlines()
            m.require(len(ids) == 1, 'Expected one owned storage process')
            before = m.inspect('container', ids[0])
            m.require(before['Image'] == owner['images']['storage'] and before['State']['Running'], 'Storage image/process differs')
            m.command(m.compose(path, owner) + ['stop', '--timeout', '45', 'storage'], capture=False, timeout=60)
            stopped = m.inspect('container', ids[0])
            m.require(not stopped['State']['Running'] and stopped['State']['ExitCode'] == 0, 'Storage must stop cleanly before restart')
            m.command(m.compose(path, owner) + ['up', '-d', '--no-deps', '--wait', '--wait-timeout', '90', 'storage'], capture=False, timeout=120)
            after_ids = m.command(m.compose(path, owner) + ['ps', '-a', '-q', 'storage']).strip().splitlines()
            m.require(len(after_ids) == 1, 'Expected one restarted storage process')
            after = m.inspect('container', after_ids[0])
            m.owned(path, owner)
            m.require(after['Image'] == owner['images']['storage'], 'Restart changed the exact storage image')
            mounts_before = [row['Name'] for row in before['Mounts'] if row['Type'] == 'volume' and row['Destination'] == '/data']
            mounts_after = [row['Name'] for row in after['Mounts'] if row['Type'] == 'volume' and row['Destination'] == '/data']
            m.require(mounts_before == mounts_after == [owner['project'] + '_objects'], 'Restart must retain the same owned object volume')
            m.require(after['State']['Running'] and after['State']['StartedAt'] != before['State']['StartedAt'], 'No real storage restart observed')
            receipt['storage'] = {'id_before': ids[0], 'id_after': after_ids[0], 'volume': mounts_after[0], 'image': before['Image'], 'started_before': before['State']['StartedAt'], 'stopped_at': stopped['State']['FinishedAt'], 'exit_code': stopped['State']['ExitCode'], 'started_after': after['State']['StartedAt']}
            m.command(base + ['verify', '/proof/plan.json', '/proof/create.json', '/proof/verify.json'], timeout=60)
        receipt['files'] = {name: m.sha256(out / name) for name in ['plan.json', 'create.json', 'verify.json']}
        receipt['passed'] = True
    except Exception as error:
        receipt['passed'] = False
        receipt['error'] = str(error)
        raise
    finally:
        existing = m.inspect('container', name)
        if existing:
            m.remove_owned_helper(existing, owner, 's3-proof')
        receipt['finished_ms'] = int(time.time() * 1000)
        m.write_json(out / 'lifecycle.json', receipt)
        print('S3 evidence: ' + str(out))


if __name__ == '__main__':
    main()
