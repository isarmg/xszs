#!/usr/bin/env python3
"""Adapter contract regressions, not substitutes for the real installed CI gate."""
import importlib.util
import json
from pathlib import Path
import unittest
import tempfile

spec = importlib.util.spec_from_file_location('adapter', Path(__file__).with_name('installed-lifecycle-adapter.py'))
adapter = importlib.util.module_from_spec(spec)
spec.loader.exec_module(adapter)


def require(condition, message):
    if not condition:
        raise RuntimeError(message)


class AdapterContract(unittest.TestCase):
    def test_retained_files_match_product_database_and_secrets(self):
        self.assertEqual(adapter.RETAINED_FILES, ('state/xszs.sqlite3', 'state/secrets.json'))
        source = Path(__file__).resolve().parents[1] / 'crates/server/src'
        self.assertIn('data_dir.join("xszs.sqlite3")', (source / 'config.rs').read_text())
        self.assertIn('data_file("secrets.json")', (source / 'installation_definition.rs').read_text())
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'state').mkdir()
            for relative in adapter.RETAINED_FILES:
                (root / relative).write_bytes(b'private test data')
            adapter.retained(root, root, {}, require)
            (root / 'state/secrets.json').unlink()
            with self.assertRaisesRegex(RuntimeError, 'database and instance secrets'):
                adapter.retained(root, root, {}, require)

    def test_production_configuration(self):
        config = {'development': True}
        adapter.configure(config, Path('/root'), Path('/shared'), 12345)
        self.assertEqual(config, {'development': False, 'bind': '127.0.0.1:12345'})

    def test_business_and_persisted_session_contract(self):
        instance = {'id': 'id-1', 'name': 'Installed lifecycle instance', 'authorization_code': 'private-code'}
        overview = {'users': [{'instances': [instance]}]}
        event = {'sequence': 1, 'action': 'device.instance.create', 'entity_id': 'id-1', 'occurred_at': '2026-10-14T00:00:00Z'}
        logs = {'date': '2026-10-14', 'logs': [event]}
        session = {'csrf_token': 'csrf', 'authenticated': True, 'user_id': 'admin-id'}
        calls = []

        def request(path, method, body, headers):
            calls.append((path, method))
            if method == 'POST':
                self.assertEqual(path, '/api/v1/admin/instances')
                self.assertEqual(json.loads(body), {'name': 'Installed lifecycle instance'})
                self.assertEqual(headers['X-CSRF-Token'], session['csrf_token'])
                result = instance
            elif path == '/api/v1/auth/session':
                result = session
            elif path == '/api/v1/admin/overview':
                result = overview
            else:
                self.assertTrue(path.startswith('/api/v1/admin/logs'))
                result = logs
            return (201 if method == 'POST' else 200), json.dumps(result).encode()

        sensitive = []
        state = adapter.exercise(request, session, sensitive, require)
        adapter.persisted(request, session, state, require)
        self.assertEqual(sensitive, ['private-code'])
        self.assertIn(('/api/v1/admin/logs?date=2026-10-14', 'GET'), calls)
        session['authenticated'] = False
        with self.assertRaisesRegex(RuntimeError, 'session did not survive'):
            adapter.persisted(request, session, state, require)
        session['authenticated'] = True
        overview['users'] = []
        with self.assertRaisesRegex(RuntimeError, 'Instance state did not survive'):
            adapter.persisted(request, session, state, require)
        overview['users'] = [{'instances': [instance]}]
        logs['logs'] = []
        with self.assertRaisesRegex(RuntimeError, 'audit event did not survive'):
            adapter.persisted(request, session, state, require)

    def test_business_routes_match_current_server(self):
        root = Path(__file__).resolve().parents[1]
        rust = (root / 'crates/server/tests/runtime_smoke.rs').read_text()
        for path in ('/api/v1/admin/instances', '/api/v1/admin/overview', '/api/v1/admin/logs'):
            self.assertIn(path, rust)
        source = (root / 'crates/server/src/admin.rs').read_text()
        self.assertIn('device.instance.create', source)
        self.assertIn('entity_id: String', source)


if __name__ == '__main__':
    unittest.main()
