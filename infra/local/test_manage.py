"""Integrity and resource-ownership gates; these tests never start Docker resources."""
import copy
import importlib.util
import json
import pathlib
import re
import subprocess
import tempfile
import unittest
from unittest import mock

SPEC = importlib.util.spec_from_file_location('local_manage', pathlib.Path(__file__).with_name('manage.py'))
manage = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(manage)


class LifecycleTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = pathlib.Path(self.temp.name).resolve()
        self.root_patch = mock.patch.object(manage, 'ROOT', self.root)
        self.root_patch.start()
        self.path, self.owner = manage.init('original', 8880, 1, 'relay:test', 'fdb:test')
        self.owner['images'] = {key: 'sha256:' + str(i) * 64 for i, key in enumerate(manage.image_keys(self.owner))}
        manage.write_json(self.path / 'owner.json', self.owner)
        self.source = self.path / 'backups' / 'snapshot'
        self.source.mkdir(parents=True, mode=0o700)
        (self.source / 'private').mkdir(mode=0o700)
        self.manifest = {'schema': 1, 'purpose': manage.PURPOSE, 'source_instance': copy.deepcopy(self.owner), 'cold': True, 'images': self.owner['images'], 'volumes': [], 'private': []}
        for name in manage.volume_names(self.owner):
            p = self.source / (name + '.tar')
            p.write_bytes(('actual-owned-archive-' + name).encode())
            p.chmod(0o600)
            self.manifest['volumes'].append({'name': name, 'bytes': p.stat().st_size, 'sha256': manage.sha256(p)})
        for name in manage.PRIVATE_FILES:
            p = self.source / 'private' / name
            p.write_bytes((self.path / name).read_bytes())
            p.chmod(0o600)
            self.manifest['private'].append({'name': name, 'bytes': p.stat().st_size, 'sha256': manage.sha256(p)})
        manage.write_json(self.source / 'manifest.json', self.manifest)

    def tearDown(self):
        self.root_patch.stop()
        self.temp.cleanup()

    def test_vault_key_bitrot_rejected_before_resources_created(self):
        p = self.source / 'private' / 'relay.env'
        data = bytearray(p.read_bytes())
        data[-3] ^= 1
        p.write_bytes(data)
        with mock.patch.object(manage, 'init') as create, mock.patch.object(manage, 'inspect') as inspect:
            with self.assertRaisesRegex(ValueError, 'integrity mismatch'):
                manage.restore(self.source, 'restored', 8890)
            create.assert_not_called()
            inspect.assert_not_called()

    def test_absent_exact_image_rejected_before_resources_created(self):
        with mock.patch.object(manage, 'init') as create, mock.patch.object(manage, 'inspect', return_value=None):
            with self.assertRaisesRegex(ValueError, 'image absent'):
                manage.restore(self.source, 'restored', 8890)
            create.assert_not_called()

    def test_image_tag_is_never_accepted_as_restore_identity(self):
        images = dict(self.owner['images'])
        images['postgres'] = 'postgres:17-bookworm'
        with mock.patch.object(manage, 'inspect', side_effect=lambda _kind, identifier: {'Id': identifier}):
            with self.assertRaisesRegex(ValueError, 'immutable'):
                manage.installed_images(images, self.owner)

    def test_backup_retains_all_services_and_secret_inventory(self):
        with mock.patch.object(manage, 'inspect', side_effect=lambda _kind, identifier: {'Id': identifier}):
            result = manage.validate_backup(self.source, self.manifest)
        self.assertEqual(result['images'], self.owner['images'])
        damaged = copy.deepcopy(self.manifest)
        damaged['private'].pop()
        with self.assertRaisesRegex(ValueError, 'secret inventory'):
            manage.validate_backup(self.source, damaged)

    def test_changed_current_image_does_not_relabel_historical_backup(self):
        current = copy.deepcopy(self.owner)
        current['images']['relay'] = 'sha256:' + 'f' * 64
        manage.write_json(self.path / 'owner.json', current)
        with mock.patch.object(manage, 'inspect', side_effect=lambda _kind, identifier: {'Id': identifier}):
            result = manage.validate_backup(self.source, self.manifest)
        self.assertEqual(result['images']['relay'], self.owner['images']['relay'])
        self.assertNotEqual(result['images']['relay'], current['images']['relay'])

    def test_foreign_container_prevents_lifecycle_before_compose(self):
        def output(args, **_kwargs):
            if args[len(manage.DOCKER)] == 'ps':
                return 'foreign-id\n'
            return json.dumps([{'Config': {'Labels': {'app': 'zoen.local', 'owner': 'another-owner'}}}])
        with mock.patch.object(manage, 'command', side_effect=output) as calls:
            with self.assertRaisesRegex(ValueError, 'Foreign container'):
                manage.owned(self.path, self.owner)
        self.assertEqual(calls.call_count, 2)

    def test_both_compose_files_have_all_image_and_owner_variables(self):
        self.owner['nodes'] = 2
        self.owner['images']['bus'] = 'sha256:' + 'a' * 64
        manage.config(self.path, self.owner)
        defined = dict(line.split('=', 1) for line in (self.path / 'compose.env').read_text().splitlines())
        for name in ['compose.yaml', 'compose.cluster.yaml']:
            template = pathlib.Path(__file__).with_name(name).read_text()
            for variable in re.findall(r'\$\{([A-Z_]+)\}', template):
                self.assertIn(variable, defined, name + ': ' + variable)
                self.assertTrue(defined[variable])
        self.assertEqual(defined['BUS_IMAGE'], self.owner['images']['bus'])

    def test_timed_out_archive_removes_only_its_owned_helper(self):
        labels = {'app': 'zoen.local', 'owner': self.owner['nonce'], 'purpose': 'archive'}
        with mock.patch.object(manage, 'inspect', side_effect=[None, {'Id': 'owned-id', 'Config': {'Labels': labels}}]), mock.patch.object(manage.secrets, 'token_hex', return_value='a' * 12), mock.patch.object(manage, 'command', side_effect=[subprocess.TimeoutExpired('docker', 180), '']) as calls:
            with self.assertRaises(subprocess.TimeoutExpired):
                manage.archive(self.owner, 'source:/source:ro', 'target:/backup', ['-cf', '/backup/data.tar', '-C', '/source', '.'])
        self.assertEqual(calls.call_args_list[1].args[0], manage.DOCKER + ['rm', '-f', 'owned-id'])

    def test_archive_timeout_cannot_remove_a_foreign_container(self):
        labels = {'app': 'zoen.local', 'owner': 'someone-else', 'purpose': 'archive'}
        with mock.patch.object(manage, 'inspect', side_effect=[None, {'Id': 'owned-id', 'Config': {'Labels': labels}}]), mock.patch.object(manage, 'command', side_effect=subprocess.TimeoutExpired('docker', 180)) as calls:
            with self.assertRaisesRegex(ValueError, 'foreign archive helper'):
                manage.archive(self.owner, 'source:/source:ro', 'target:/backup', ['-cf', '/backup/data.tar'])
        self.assertEqual(calls.call_count, 1)

    def test_cluster_checks_protected_secondary_port_before_docker(self):
        self.owner['port'] = 8789
        with mock.patch.object(manage, 'inspect') as inspect:
            with self.assertRaisesRegex(ValueError, 'Protected second port'):
                manage.cluster(self.path, self.owner)
            inspect.assert_not_called()


if __name__ == '__main__':
    unittest.main()
