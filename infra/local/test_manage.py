"""Integrity and resource-ownership gates; these tests never start Docker resources."""
import copy
import errno
import importlib.util
import json
import pathlib
import re
import socket
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

    def test_restart_accepts_a_released_port_with_a_recent_connection(self):
        # An active server-side close leaves TIME_WAIT after its listener exits.
        # No Docker process or active listener owns this ephemeral port now.
        with socket.socket() as listener:
            listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            listener.bind(('127.0.0.1', 0))
            listener.listen(1)
            port = listener.getsockname()[1]
            with socket.create_connection(('127.0.0.1', port), timeout=2) as client:
                connection, _ = listener.accept()
                with connection:
                    connection.sendall(b'owned connection')
                    connection.shutdown(socket.SHUT_WR)
                    self.assertEqual(client.recv(64), b'owned connection')
                    self.assertEqual(client.recv(64), b'')
        owner = {**self.owner, 'port': port}
        with mock.patch.object(manage, 'owned'), mock.patch.object(manage, 'installed_images'), mock.patch.object(manage, 'ready'), mock.patch.object(manage, 'command', return_value='') as command:
            manage.up(self.path, owner)
        self.assertTrue(any('up' in call.args[0] for call in command.call_args_list))

    def test_restart_still_refuses_an_active_listener_that_allows_address_reuse(self):
        with socket.socket() as listener:
            listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
            listener.bind(('127.0.0.1', 0))
            listener.listen(1)
            owner = {**self.owner, 'port': listener.getsockname()[1]}
            with mock.patch.object(manage, 'owned'), mock.patch.object(manage, 'installed_images'), mock.patch.object(manage, 'ready'), mock.patch.object(manage, 'command', return_value='') as command:
                with self.assertRaises(OSError) as raised:
                    manage.up(self.path, owner)
            self.assertEqual(raised.exception.errno, errno.EADDRINUSE)
            self.assertFalse(any('up' in call.args[0] for call in command.call_args_list))

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
            for variable in re.findall(r'\$\{([A-Z_][A-Z0-9_]*)\}', template):
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

    def frontdoor_backup(self):
        self.owner['nodes'] = 2
        self.owner['images']['bus'] = 'sha256:' + 'a' * 64
        self.owner['images']['frontdoor'] = 'sha256:' + 'f' * 64
        data = (manage.REPO / 'infra/local/haproxy.cfg').read_bytes()
        manage.publish_frontdoor_config(self.path, data)
        self.owner['frontdoor'] = {'port': 8882, 'config_sha256': manage.sha256(self.path / 'haproxy.cfg')}
        manage.write_json(self.path / 'owner.json', self.owner)
        manage.config(self.path, self.owner)
        self.manifest['source_instance'] = copy.deepcopy(self.owner)
        self.manifest['images'] = dict(self.owner['images'])
        bus = self.source / 'bus.tar'
        bus.write_bytes(b'owned bus snapshot')
        bus.chmod(0o600)
        self.manifest['volumes'].append({'name': 'bus', 'bytes': bus.stat().st_size, 'sha256': manage.sha256(bus)})
        runtime = self.source / 'runtime'
        runtime.mkdir(mode=0o700)
        config = runtime / 'haproxy.cfg'
        config.write_bytes(data)
        config.chmod(0o600)
        self.manifest['runtime'] = [{'name': 'haproxy.cfg', 'bytes': len(data), 'sha256': manage.sha256(config)}]
        manage.write_json(self.source / 'manifest.json', self.manifest)

    def proxy_value(self):
        info = self.owner['frontdoor']
        return {'Id': 'proxy-id', 'Image': self.owner['images']['frontdoor'], 'Config': {'Labels': {'app': 'zoen.local', 'owner': self.owner['nonce'], 'purpose': 'native-local-frontdoor', 'com.docker.compose.service': 'frontdoor', 'config_sha256': info['config_sha256']}}, 'HostConfig': {'PortBindings': {'8080/tcp': [{'HostIp': '127.0.0.1', 'HostPort': str(info['port'])}]}}, 'State': {'Running': True, 'Health': {'Status': 'healthy'}}}

    def test_disabled_old_owner_and_backup_need_no_proxy_image_or_config(self):
        self.assertIsNone(manage.frontdoor_info(self.owner))
        self.assertNotIn('frontdoor', manage.image_keys(self.owner))
        self.assertNotIn('compose.frontdoor.yaml', ' '.join(manage.compose(self.path, self.owner)))
        with mock.patch.object(manage, 'inspect', side_effect=lambda _kind, identifier: {'Id': identifier}):
            manage.validate_backup(self.source, self.manifest)

    def test_proxy_config_bitrot_refuses_before_docker_or_restore_creation(self):
        self.frontdoor_backup()
        (self.path / 'haproxy.cfg').unlink()  # Source runtime loss cannot excuse a bad backup.
        config = self.source / 'runtime/haproxy.cfg'
        config.write_bytes(config.read_bytes() + b'\ncorrupt')
        with mock.patch.object(manage, 'init') as create, mock.patch.object(manage, 'inspect') as inspect:
            with self.assertRaisesRegex(ValueError, 'integrity mismatch'):
                manage.restore(self.source, 'restored', 8890)
            create.assert_not_called()
            inspect.assert_not_called()

    def test_intact_proxy_backup_restores_after_current_config_bitrot_or_loss(self):
        self.frontdoor_backup()
        current = self.path / 'haproxy.cfg'
        backup = (self.source / 'runtime/haproxy.cfg').read_bytes()
        for fault in ['bitrot', 'missing']:
            with self.subTest(fault=fault):
                if fault == 'bitrot':
                    current.chmod(0o600)
                    current.write_bytes(backup + b'\ncorrupt current runtime')
                    current.chmod(0o444)
                else:
                    current.unlink()
                with self.assertRaisesRegex(ValueError, 'Pinned frontdoor config'):
                    manage.instance('original')  # Ordinary runtime actions remain strict.
                name = 'restored-' + fault
                with mock.patch.object(manage, 'inspect', side_effect=lambda kind, identifier: None if kind == 'volume' else {'Id': identifier}), mock.patch.object(manage, 'command', return_value=''), mock.patch.object(manage, 'archive'), mock.patch.object(manage, 'up') as up:
                    manage.restore(self.source, name, 8890)
                restored_path, restored = manage.instance(name)
                self.assertEqual((restored_path / 'haproxy.cfg').read_bytes(), backup)
                self.assertEqual(restored['frontdoor']['config_sha256'], self.owner['frontdoor']['config_sha256'])
                self.assertEqual(restored['images']['frontdoor'], self.owner['images']['frontdoor'])
                self.assertNotEqual(restored['nonce'], self.owner['nonce'])
                up.assert_called_once()

    def test_missing_exact_proxy_image_refuses_before_restore_creation(self):
        self.frontdoor_backup()
        def image(_kind, identifier):
            return None if identifier == self.owner['images']['frontdoor'] else {'Id': identifier}
        with mock.patch.object(manage, 'init') as create, mock.patch.object(manage, 'inspect', side_effect=image):
            with self.assertRaisesRegex(ValueError, 'frontdoor image absent'):
                manage.restore(self.source, 'restored', 8890)
            create.assert_not_called()

    def test_restore_checks_proxy_port_before_creating_any_state(self):
        self.frontdoor_backup()
        with mock.patch.object(manage, 'init') as create, mock.patch.object(manage, 'inspect', side_effect=lambda _kind, identifier: {'Id': identifier}):
            with self.assertRaisesRegex(ValueError, 'unprotected distinct'):
                manage.restore(self.source, 'restored', 8890, frontdoor_port=8790)
            create.assert_not_called()

    def test_restore_retains_exact_proxy_config_and_remaps_only_listener(self):
        self.frontdoor_backup()
        original = copy.deepcopy(self.manifest)
        with mock.patch.object(manage, 'inspect', side_effect=lambda kind, identifier: None if kind == 'volume' else {'Id': identifier}), mock.patch.object(manage, 'command', return_value=''), mock.patch.object(manage, 'archive'), mock.patch.object(manage, 'up') as up:
            manage.restore(self.source, 'restored', 8890)
        path, restored = manage.instance('restored')
        self.assertEqual(restored['frontdoor']['port'], 8892)
        self.assertEqual(restored['frontdoor']['config_sha256'], self.owner['frontdoor']['config_sha256'])
        self.assertEqual((path / 'haproxy.cfg').read_bytes(), (self.source / 'runtime/haproxy.cfg').read_bytes())
        self.assertEqual(restored['images']['frontdoor'], self.owner['images']['frontdoor'])
        self.assertEqual(self.manifest, original)
        up.assert_called_once()

    def test_cold_backup_captures_exact_gateway_image_and_configuration(self):
        self.frontdoor_backup()
        def command(args, **_kwargs):
            if 'ps' in args:
                return args[-1] + '-id\n'
            service = args[-1].removesuffix('-id')
            return json.dumps([{'Image': self.owner['images']['relay' if service == 'relay2' else service]}])
        def archive(_owner, _source, target, _args, owned_output=None):
            output = pathlib.Path(target.removesuffix(':/backup')) / pathlib.Path(owned_output).name
            output.write_bytes(b'owned volume archive')
            output.chmod(0o600)
        with mock.patch.object(manage, 'owned'), mock.patch.object(manage, 'installed_images'), mock.patch.object(manage, 'stop') as stop, mock.patch.object(manage, 'up') as up, mock.patch.object(manage, 'command', side_effect=command), mock.patch.object(manage, 'inspect', return_value={'Name': 'owned'}), mock.patch.object(manage, 'archive', side_effect=archive):
            manage.backup(self.path, self.owner)
        saved = next(p for p in (self.path / 'backups').iterdir() if p.name != 'snapshot')
        manifest = json.loads((saved / 'manifest.json').read_text())
        self.assertEqual(manifest['images']['frontdoor'], self.owner['images']['frontdoor'])
        self.assertEqual(manifest['runtime'], [{'name': 'haproxy.cfg', 'bytes': (saved / 'runtime/haproxy.cfg').stat().st_size, 'sha256': self.owner['frontdoor']['config_sha256']}])
        with mock.patch.object(manage, 'inspect', side_effect=lambda _kind, identifier: {'Id': identifier}):
            manage.validate_backup(saved, manifest)
        stop.assert_called_once()
        up.assert_called_once()

    def test_frontdoor_activation_validates_ownership_before_enable(self):
        def output(args, **_kwargs):
            if args[len(manage.DOCKER)] == 'ps':
                return 'foreign-id\n'
            return json.dumps([{'Config': {'Labels': {'app': 'zoen.local', 'owner': 'someone-else'}}}])
        before = (self.path / 'owner.json').read_bytes()
        with mock.patch.object(manage, 'command', side_effect=output):
            with self.assertRaisesRegex(ValueError, 'Foreign container'):
                manage.frontdoor(self.path, self.owner)
        self.assertEqual((self.path / 'owner.json').read_bytes(), before)
        self.assertFalse((self.path / 'haproxy.cfg').exists())

    def test_ambiguous_gateway_start_retains_exact_enabled_retry_and_never_targets_native(self):
        self.owner['nodes'] = 2
        self.owner['images']['bus'] = 'sha256:' + 'a' * 64
        manage.write_json(self.path / 'owner.json', self.owner)
        manage.config(self.path, self.owner)
        commands, exists = [], False
        def command(args, **_kwargs):
            nonlocal exists
            commands.append(args)
            if 'up' in args:
                if not exists:
                    exists = True  # Docker may have created it before a lost reply.
                    raise subprocess.TimeoutExpired('docker compose up', 90)
                return ''
            if args[-1] == 'frontdoor' and 'ps' in args:
                return 'proxy-id\n' if exists else ''
            return ''
        def inspect(kind, _identifier):
            return {'Id': 'sha256:' + 'f' * 64, 'RepoDigests': [manage.IMAGE_REFS['frontdoor']]} if kind == 'image' else self.proxy_value()
        stable = [{'service': str(i), 'id': str(i), 'pid': i, 'config_sha256': 'stable'} for i in range(6)]
        with mock.patch.object(manage, 'owned'), mock.patch.object(manage, 'installed_images'), mock.patch.object(manage, 'native_snapshot', return_value=stable), mock.patch.object(manage, 'ready'), mock.patch.object(manage, 'check_frontdoor_parser'), mock.patch.object(manage.socket, 'socket'), mock.patch.object(manage, 'inspect', side_effect=inspect), mock.patch.object(manage, 'command', side_effect=command):
            with self.assertRaises(subprocess.TimeoutExpired):
                manage.frontdoor(self.path, self.owner)
            path, enabled = manage.instance('original')
            self.assertEqual(enabled['frontdoor']['port'], 8882)
            self.assertEqual(manage.sha256(path / 'haproxy.cfg'), enabled['frontdoor']['config_sha256'])
            manage.frontdoor(path, enabled)
        starts = [args for args in commands if 'up' in args]
        self.assertEqual(len(starts), 2)
        self.assertTrue(all('--no-deps' in args and args[-1] == 'frontdoor' for args in starts))
        self.assertTrue((self.path / 'frontdoor-runtime.json').is_file())

    def test_parser_refusal_leaves_no_durable_enabled_descriptor(self):
        self.owner['nodes'] = 2
        self.owner['images']['bus'] = 'sha256:' + 'a' * 64
        manage.write_json(self.path / 'owner.json', self.owner)
        before = (self.path / 'owner.json').read_bytes()
        image = {'Id': 'sha256:' + 'f' * 64, 'RepoDigests': [manage.IMAGE_REFS['frontdoor']]}
        with mock.patch.object(manage, 'owned'), mock.patch.object(manage, 'installed_images'), mock.patch.object(manage, 'native_snapshot'), mock.patch.object(manage, 'ready'), mock.patch.object(manage.socket, 'socket'), mock.patch.object(manage, 'inspect', return_value=image), mock.patch.object(manage, 'check_frontdoor_parser', side_effect=RuntimeError('actual parser refusal')), mock.patch.object(manage, 'command') as command:
            with self.assertRaisesRegex(RuntimeError, 'parser refusal'):
                manage.frontdoor(self.path, self.owner)
            command.assert_not_called()
        self.assertEqual((self.path / 'owner.json').read_bytes(), before)
        self.assertFalse((self.path / 'haproxy.cfg').exists())
        self.assertFalse(list(self.path.glob('frontdoor-prepare-*')))

    def test_occupied_gateway_port_leaves_owner_disabled_before_parser(self):
        self.owner['nodes'] = 2
        self.owner['images']['bus'] = 'sha256:' + 'a' * 64
        manage.write_json(self.path / 'owner.json', self.owner)
        before = (self.path / 'owner.json').read_bytes()
        image = {'Id': 'sha256:' + 'f' * 64, 'RepoDigests': [manage.IMAGE_REFS['frontdoor']]}
        with mock.patch.object(manage, 'owned'), mock.patch.object(manage, 'installed_images'), mock.patch.object(manage, 'native_snapshot'), mock.patch.object(manage, 'ready'), mock.patch.object(manage, 'inspect', return_value=image), mock.patch.object(manage, 'check_frontdoor_parser') as parser, mock.patch.object(manage.socket, 'socket') as socket:
            socket.return_value.__enter__.return_value.bind.side_effect = OSError('address in use')
            with self.assertRaisesRegex(OSError, 'address in use'):
                manage.frontdoor(self.path, self.owner)
            parser.assert_not_called()
        self.assertEqual((self.path / 'owner.json').read_bytes(), before)
        self.assertFalse((self.path / 'haproxy.cfg').exists())

    def test_all_optional_compose_variables_have_pinned_values(self):
        self.frontdoor_backup()
        defined = dict(line.split('=', 1) for line in (self.path / 'compose.env').read_text().splitlines())
        template = pathlib.Path(__file__).with_name('compose.frontdoor.yaml').read_text()
        for variable in re.findall(r'\$\{([A-Z_][A-Z0-9_]*)\}', template):
            self.assertTrue(defined[variable])
        self.assertEqual(defined['FRONTDOOR_IMAGE'], self.owner['images']['frontdoor'])
        self.assertEqual(defined['FRONTDOOR_CONFIG_SHA256'], self.owner['frontdoor']['config_sha256'])

    def test_foreign_or_wrong_binding_proxy_cannot_be_stopped(self):
        self.frontdoor_backup()
        value = self.proxy_value()
        value['HostConfig']['PortBindings']['8080/tcp'][0]['HostIp'] = '0.0.0.0'
        with mock.patch.object(manage, 'command', return_value='proxy-id\n') as command, mock.patch.object(manage, 'inspect', return_value=value):
            with self.assertRaisesRegex(ValueError, 'binding differs'):
                manage.stop_frontdoor(self.path, self.owner, remove=True)
        self.assertEqual(command.call_count, 1)

    def test_down_removes_gateway_before_compose_network_removal(self):
        self.frontdoor_backup()
        order = []
        with mock.patch.object(manage, 'owned'), mock.patch.object(manage, 'stop_frontdoor', side_effect=lambda *args, **kwargs: order.append(('proxy', kwargs))), mock.patch.object(manage, 'command', side_effect=lambda args, **kwargs: order.append(('compose', args))):
            manage.down(self.path, self.owner)
        self.assertEqual(order[0], ('proxy', {'remove': True}))
        self.assertIn('down', order[1][1])


if __name__ == '__main__':
    unittest.main()
