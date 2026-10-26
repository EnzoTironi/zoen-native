"""Real bootstrap/helper shell execution through explicit fake Docker ports."""
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import textwrap
import unittest


ROOT = Path(__file__).resolve().parents[2]
HELPER = ROOT / "scripts/ci-pull-postgres.sh"
DIGEST = "sha256:266d3c257594e195b309ae9d1805e54c788f1d7b375f56bf6c367aac6c917e7b"
REPOSITORIES = ("docker.io/library/postgres", "public.ecr.aws/docker/library/postgres",
                "mirror.gcr.io/library/postgres")
REFERENCES = tuple(f"{repository}:17-alpine@{DIGEST}" for repository in REPOSITORIES)
FAKE = r'''
import json, os
from pathlib import Path
import sys
tool = Path(sys.argv[0]).name
args = sys.argv[1:]
root = Path(os.environ['PG_PULL_FAKE'])
mode = os.environ.get('PG_PULL_MODE', 'success')
with (root/'calls.jsonl').open('a') as out: out.write(json.dumps([tool, *args])+'\n')
if tool == 'timeout':
    assert args[0] in ('60s', '5s'), args
    if mode == 'timeout_primary' and args[1:3] == ['docker', 'pull'] and 'docker.io/' in args[-1]:
        sys.exit(124)
    binary = root/'bin'/args[1]
    os.execv(binary, [str(binary), *args[2:]])
elif tool == 'docker':
    primary = 'docker.io/' in args[-1]
    if args[0] == 'pull':
        assert args[1] == '--quiet'
        if mode == 'fail_all' or primary and mode == 'primary_failed' or mode == 'first_two_failed' and not args[-1].startswith('mirror.gcr.io/'):
            print('synthetic registry refusal', file=sys.stderr)
            sys.exit(19)
        print('docker pull chatter must not contaminate selected image output')
    elif args[:2] == ['image', 'inspect']:
        assert args[2:4] == ['--format', '{{.Id}}']
        if mode == 'all_inspect_failed' or primary and mode == 'inspect_failed': sys.exit(23)
        print('' if mode == 'all_empty' or primary and mode == 'empty_primary' else
              'invalid' if primary and mode == 'invalid_primary' else 'sha256:'+'a'*64)
    elif args[0] == 'run':
        print('synthetic-container-id')
    else:
        assert args[:1] == ['exec'] and args[2:] == ['pg_isready', '-U', 'zoen'], args
elif tool == 'seq':
    print('1')
elif tool != 'sleep':
    raise AssertionError((tool,args))
'''


def run_block(path, title):
    lines = (ROOT / path).read_text().splitlines()
    start = lines.index(f"      - name: {title}") + 1
    while lines[start] != "        run: |":
        if not lines[start].startswith("        "):
            raise AssertionError(f"missing run body for {title}")
        start += 1
    body = []
    for line in lines[start + 1:]:
        if line and not line.startswith("          "):
            break
        body.append(line[10:] if line else "")
    return "\n".join(body)


class PullPostgresTest(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix="zoen-pg-pull-")
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        (self.root / 'bin').mkdir()
        (self.root / 'scripts').mkdir()
        for name in ('docker', 'timeout', 'seq', 'sleep'):
            p = self.root / 'bin' / name
            p.write_text(f'#!{sys.executable}\n'+FAKE)
            p.chmod(0o755)
        copied = self.root / 'scripts/ci-pull-postgres.sh'
        copied.write_bytes(HELPER.read_bytes())
        copied.chmod(0o755)
        for name in ('fdb', 'nats'):
            p = self.root / f'scripts/{name}.sh'
            p.write_text(textwrap.dedent(f'''\
                #!/bin/bash
                printf '%s %s\\n' '{name}' "$1" >> "$PG_PULL_FAKE/services.log"
                if [[ "$1" == env ]]; then
                  printf '%s\\n' 'FDB_CLUSTER_FILE=/tmp/synthetic-cluster' 'LIBRARY_PATH=/tmp/synthetic-lib' 'LD_LIBRARY_PATH=/tmp/synthetic-lib' 'ZOEN_NATS_URL=nats://synthetic.invalid'
                fi
                '''))
            p.chmod(0o755)
        self.env = dict(os.environ, PG_PULL_FAKE=str(self.root),
                        GITHUB_ENV=str(self.root / 'github-env'),
                        PATH=f'{self.root}/bin:/usr/bin:/bin')

    def calls(self):
        p = self.root / 'calls.jsonl'
        return [json.loads(line) for line in p.read_text().splitlines()] if p.exists() else []

    def run_helper(self, mode='success'):
        return subprocess.run(['/bin/bash', str(HELPER)], text=True, capture_output=True,
                              env=dict(self.env, PG_PULL_MODE=mode), timeout=10)

    def test_official_source_is_first_and_stdout_is_only_its_exact_pinned_reference(self):
        result = self.run_helper()
        self.assertEqual(0, result.returncode, result.stderr)
        self.assertEqual(REFERENCES[0]+'\n', result.stdout)
        self.assertIn('docker pull chatter', result.stderr)
        self.assertEqual([['docker', 'pull', '--quiet', REFERENCES[0]]],
                         [call for call in self.calls() if call[:2] == ['docker', 'pull']])
        self.assertIn(['timeout', '60s', 'docker', 'pull', '--quiet', REFERENCES[0]], self.calls())
        self.assertFalse(any(call[:2] == ['docker', 'run'] for call in self.calls()))

    def test_failed_or_unusable_primary_falls_back_without_success_contamination(self):
        for mode in ('primary_failed', 'inspect_failed', 'empty_primary', 'invalid_primary', 'timeout_primary'):
            with self.subTest(mode=mode):
                result = self.run_helper(mode)
                self.assertEqual(0, result.returncode, result.stderr)
                self.assertEqual(REFERENCES[1]+'\n', result.stdout)

    def test_last_mirror_reports_its_exact_reference_after_both_other_sources_fail(self):
        result = self.run_helper('first_two_failed')
        self.assertEqual(0, result.returncode, result.stderr)
        self.assertEqual(REFERENCES[2]+'\n', result.stdout)
        self.assertEqual([['docker', 'pull', '--quiet', ref] for ref in REFERENCES],
                         [call for call in self.calls() if call[:2] == ['docker', 'pull']])

    def test_all_sources_fail_once_with_no_empty_success_or_container(self):
        for mode in ('fail_all', 'all_empty', 'all_inspect_failed'):
            with self.subTest(mode=mode):
                (self.root/'calls.jsonl').unlink(missing_ok=True)
                result = self.run_helper(mode)
                self.assertEqual(1, result.returncode)
                self.assertEqual('', result.stdout)
                self.assertEqual([['docker', 'pull', '--quiet', ref] for ref in REFERENCES],
                                 [call for call in self.calls() if call[:2] == ['docker', 'pull']])
                self.assertIn('Unable to acquire', result.stderr)
                self.assertFalse(any(call[:2] == ['docker', 'run'] for call in self.calls()))

    def test_actual_workflow_bootstraps_preserve_options_and_readiness(self):
        cases = [('.github/workflows/ci.yml', 'Postgres, FoundationDB and NATS', 'pg', ['-c', 'max_connections=300'], 1),
                 ('.github/workflows/ci.yml', 'Postgres and FoundationDB', 'pg', [], 1),
                 ('.github/workflows/expo.yml', 'Disposable Postgres and FoundationDB', 'expo-pg', [], 2)]
        for path, title, name, extra, health_count in cases:
            with self.subTest(title=title):
                (self.root / 'calls.jsonl').unlink(missing_ok=True)
                block = run_block(path, title)
                self.assertEqual(1, block.count('pg="$(scripts/ci-pull-postgres.sh)"'))
                self.assertNotIn('docker pull', block)
                result = subprocess.run(['/bin/bash', '-e', '-o', 'pipefail', '-c', block],
                                        cwd=self.root, env=self.env, text=True, capture_output=True, timeout=10)
                self.assertEqual(0, result.returncode, result.stderr)
                expected = ['docker', 'run', '-d', '--name', name, '-p', '5432:5432', '-e',
                            'POSTGRES_USER=zoen', '-e', 'POSTGRES_HOST_AUTH_METHOD=trust', REFERENCES[0], *extra]
                self.assertEqual([expected], [call for call in self.calls() if call[:2] == ['docker', 'run']])
                self.assertEqual(health_count, self.calls().count(['docker', 'exec', name, 'pg_isready', '-U', 'zoen']))
                self.assertIn('FDB_CLUSTER_FILE=/tmp/synthetic-cluster', (self.root/'github-env').read_text())

    def test_workflow_failure_never_starts_container_or_other_services(self):
        for path, title in [('.github/workflows/ci.yml', 'Postgres, FoundationDB and NATS'),
                            ('.github/workflows/ci.yml', 'Postgres and FoundationDB'),
                            ('.github/workflows/expo.yml', 'Disposable Postgres and FoundationDB')]:
            with self.subTest(title=title):
                (self.root/'calls.jsonl').unlink(missing_ok=True)
                block = run_block(path, title)
                result = subprocess.run(['/bin/bash', '-e', '-o', 'pipefail', '-c', block], cwd=self.root,
                                        env=dict(self.env, PG_PULL_MODE='fail_all'), text=True, capture_output=True, timeout=10)
                self.assertEqual(1, result.returncode)
                self.assertFalse(any(call[:2] == ['docker', 'run'] for call in self.calls()))
                self.assertFalse((self.root/'services.log').exists())
                self.assertFalse((self.root/'github-env').exists())


if __name__ == '__main__':
    unittest.main()
