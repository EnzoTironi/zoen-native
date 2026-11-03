import copy
import json
import pathlib
import re
import subprocess
from unittest import mock

from manage_test_support import manage


class GatewayCases:
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
