#!/usr/bin/env python3
"""Offline installer tests. All writes and service actions stay in private fixtures."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

APP = "xszs"
ROOT = Path(__file__).resolve().parents[1]
FAKE_BINARY = r'''#!/usr/bin/env python3
import json,os,pathlib,sys
app="xszs"
prefix=app.upper()
root=pathlib.Path(os.environ[prefix+'_INSTALL_ROOT'])
config=json.loads(pathlib.Path(sys.argv[2]).read_text())
args=sys.argv[3:]
assert args[:1]==['--data-dir']
data=pathlib.Path(args[1]);args=args[2:]
assert str(data).startswith(str(root)+'/')
assert prefix+'_DATA_DIR' not in os.environ
assert prefix+'_BIND' not in os.environ
storage=None
if app=='xszs':
    storage=pathlib.Path(os.environ['XSZS_STORAGE_DIR'])
    assert str(storage).startswith(str(root)+'/')
with (root/'cli-actions').open('a') as out: out.write(' '.join(args)+'\n')
if args[:2]==['config','validate']: sys.exit(0)
if args[0]=='init':
    assert sys.stdin.read()=='correct horse battery\n'
    expected=(root/'expected-username').read_text() if (root/'expected-username').exists() else 'admin'
    assert args[1:]==['--username',expected]
    for name,content in [(app+'.sqlite3',b'database'),('secrets.json',b'stable key')]:
        with (data/name).open('xb') as f: f.write(content)
    if storage is not None:
        storage.mkdir(parents=True,exist_ok=True,mode=0o700)
        (storage/'media-marker').write_bytes(b'unchanged media')
elif args[0]=='check':
    if (root/'fail-check').exists(): sys.exit(1)
    assert (data/'secrets.json').read_bytes()==b'stable key'
    assert (data/(app+'.sqlite3')).read_bytes()==b'database'
    if storage is not None: assert (storage/'media-marker').read_bytes()==b'unchanged media'
else: sys.exit(2)
'''

class InstallationTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.temp = Path(self.temporary.name)
        self.host = self.temp / 'host'
        self.host.mkdir()
        self.release = self.temp / 'release'
        for directory in ('bin', 'config', 'systemd'):
            (self.release / directory).mkdir(parents=True)
        self.binary = self.release / 'bin' / APP
        self.binary.write_text(FAKE_BINARY)
        self.binary.chmod(0o755)
        shutil.copy2(ROOT / 'install.sh', self.release / 'install.sh')
        shutil.copy2(ROOT / 'deploy' / f'{APP}.service', self.release / 'systemd' / f'{APP}.service')
        self.chosen = self.temp / 'chosen.json'
        self.chosen.write_text(json.dumps({'data_dir':'/srv/custom state','log_level':'warn'}))
        self.env = os.environ | {f'{APP.upper()}_INSTALL_TEST':'1',f'{APP.upper()}_INSTALL_ROOT':str(self.host)}
        self.config = self.host / f'etc/isarmg/{APP}/config.json'
        self.state = self.host / 'srv/custom state'
        self.installed = self.host / f'opt/isarmg/{APP}/bin/{APP}'
        self.dropin = self.host / f'etc/systemd/system/{APP}.service.d/state.conf'

    def invoke(self, *args):
        return subprocess.run(['bash',str(self.release/'install.sh'),*args], input='correct horse battery\n',env=self.env,capture_output=True,text=True)

    def setup_installation(self, username='alice'):
        if username is not None: (self.host/'expected-username').write_text(username)
        args=['setup',str(self.chosen)] + ([] if username is None else [username])
        result=self.invoke(*args)
        self.assertEqual(result.returncode,0,result.stderr)
        return result

    def test_setup_initializes_selected_private_root_without_starting(self):
        result=self.setup_installation()
        self.assertIn(f'systemctl enable --now {APP}.service',result.stdout)
        self.assertEqual(self.config.read_bytes(),self.chosen.read_bytes())
        self.assertEqual(self.config.stat().st_mode & 0o777,0o600)
        for path in [self.config.parent,self.state]: self.assertEqual(path.stat().st_mode & 0o777,0o700)
        self.assertEqual((self.host/'cli-actions').read_text().splitlines(),['config validate','init --username alice','check'])
        self.assertEqual((self.host/'service-actions').read_text().splitlines(),['daemon-reload'])
        self.assertFalse((self.host/f'var/lib/isarmg/{APP}').exists())
        self.assertFalse((self.host/'run').exists())
        self.assertIn('ReadWritePaths="/srv/custom state"',self.dropin.read_text())
        original=self.config.read_bytes()
        self.assertNotEqual(self.invoke('setup',str(self.chosen)).returncode,0)
        self.assertEqual(original,self.config.read_bytes())

    def test_default_username_and_data_directory(self):
        self.chosen.write_text('{}')
        self.setup_installation(username=None)
        self.assertIn('init --username admin',(self.host/'cli-actions').read_text())
        self.assertTrue((self.host/f'var/lib/isarmg/{APP}/secrets.json').exists())

    def test_upgrade_preserves_state_and_restarts_only_previously_active(self):
        self.setup_installation()
        for active in [False,True]:
            with self.subTest(active=active):
                if active: (self.host/'service-active').touch()
                paths=[self.config,self.state/'secrets.json',self.state/f'{APP}.sqlite3']
                if APP=='xszs': paths.append(self.state/'media/media-marker')
                before={p:(p.read_bytes(),p.stat().st_mtime_ns,p.stat().st_mode) for p in paths}
                self.binary.write_text(FAKE_BINARY+'\n# next version\n')
                (self.host/'service-actions').write_text('')
                (self.host/'cli-actions').write_text('')
                result=self.invoke('upgrade')
                self.assertEqual(result.returncode,0,result.stderr)
                self.assertEqual(before,{p:(p.read_bytes(),p.stat().st_mtime_ns,p.stat().st_mode) for p in paths})
                self.assertIn('# next version',self.installed.read_text())
                expected=[f'is-active --quiet {APP}.service']
                if active: expected += [f'stop {APP}.service']
                expected += ['daemon-reload']
                if active: expected += [f'start {APP}.service']
                self.assertEqual((self.host/'service-actions').read_text().splitlines(),expected)
                self.assertEqual((self.host/'cli-actions').read_text().splitlines(),['config validate','check'])
                self.assertFalse((self.host/'run').exists())

    def test_failed_upgrade_check_does_not_restart_service(self):
        self.setup_installation()
        (self.host/'service-active').touch()
        (self.host/'fail-check').touch()
        (self.host/'service-actions').write_text('')
        result=self.invoke('upgrade')
        self.assertNotEqual(result.returncode,0)
        actions=(self.host/'service-actions').read_text()
        self.assertIn(f'stop {APP}.service',actions)
        self.assertNotIn(f'start {APP}.service',actions)

    def test_systemd_path_escaping(self):
        path='/srv/state "quoted" %percent \\backslash'
        self.chosen.write_text(json.dumps({'data_dir':path}))
        self.setup_installation()
        expected=path.replace('\\','\\\\').replace('"','\\"').replace('%','%%')
        self.assertIn('ReadWritePaths="'+expected+'"',self.dropin.read_text())

    def test_parent_environment_cannot_override_selected_json(self):
        self.env.update({f'{APP.upper()}_DATA_DIR':'/wrong',f'{APP.upper()}_BIND':'0.0.0.0:1'})
        self.setup_installation()
        self.assertTrue((self.state/'secrets.json').exists())
        self.assertFalse((self.host/'wrong').exists())

    def test_fixture_cannot_alias_the_real_root(self):
        self.env[f'{APP.upper()}_INSTALL_ROOT']='/tmp/..'
        result=self.invoke('setup',str(self.chosen))
        self.assertNotEqual(result.returncode,0)
        self.assertIn('canonical filesystem root',result.stderr)

    def test_upgrade_rejects_extra_arguments_or_missing_installation(self):
        self.assertNotEqual(self.invoke('upgrade',str(self.chosen)).returncode,0)
        self.assertNotEqual(self.invoke('upgrade').returncode,0)
        self.assertNotEqual(self.invoke('setup').returncode,0)
        self.assertEqual(list(self.host.iterdir()),[])

    def test_state_cannot_overlap_program_config_unit_or_runtime(self):
        paths=[f'/opt/isarmg/{APP}/bin',f'//etc/isarmg/{APP}','/etc/systemd/system','/run',f'/run/isarmg/{APP}/child','/']
        fields=['data_dir'] + (['storage_dir'] if APP=='xszs' else [])
        for field in fields:
            for path in paths:
                with self.subTest(field=field,path=path):
                    self.chosen.write_text(json.dumps({'data_dir':'/srv/state',field:path}))
                    result=self.invoke('setup',str(self.chosen))
                    self.assertNotEqual(result.returncode,0)
                    self.assertFalse(self.config.exists())
                    self.assertFalse(self.installed.exists())
                    self.assertFalse((self.host/'cli-actions').exists())

    def test_existing_nonempty_state_is_never_initialized(self):
        self.state.mkdir(parents=True)
        protected=self.state/'existing'
        protected.write_bytes(b'preserve')
        self.assertNotEqual(self.invoke('setup',str(self.chosen)).returncode,0)
        self.assertEqual(protected.read_bytes(),b'preserve')
        self.assertFalse(self.config.exists())
        self.assertFalse((self.host/'cli-actions').exists())

    def test_symlinked_parent_and_hardlinked_config_are_rejected(self):
        (self.host/'opt').symlink_to(self.temp)
        self.assertNotEqual(self.invoke('setup',str(self.chosen)).returncode,0)
        self.assertFalse((self.temp/'isarmg').exists())
        (self.host/'opt').unlink()
        os.link(self.chosen,self.temp/'chosen-alias')
        self.assertNotEqual(self.invoke('setup',str(self.chosen)).returncode,0)
        self.assertFalse(self.config.exists())

    def test_unit_and_examples_use_selected_state_contract(self):
        unit=(ROOT/'deploy'/f'{APP}.service').read_text()
        for line in (f'User=isarmg-{APP}',f'Group=isarmg-{APP}',f'RuntimeDirectory=isarmg/{APP}','RuntimeDirectoryMode=0700','TimeoutStopSec=120s','ProtectSystem=strict','ProtectHome=read-only'):
            self.assertIn(line,unit.splitlines())
        for forbidden in ('--release-root','/current/','EnvironmentFile=','ExecStartPre=','StateDirectory=','WorkingDirectory=','PrivateTmp=','ReadWritePaths='):
            self.assertNotIn(forbidden,unit)
        config=json.loads((ROOT/'config'/f'{APP}.json.example').read_text())
        self.assertFalse(config['development'])
        self.assertEqual(config['data_dir'],f'/var/lib/isarmg/{APP}')
        self.assertTrue({'bind','log_level','trusted_proxies'} <= config.keys())
        self.assertFalse(any('password' in key or 'bootstrap' in key or 'key' in key or key=='database_url' for key in config))

    @unittest.skipUnless(APP=='xszs','separate media storage is xszs-specific')
    def test_external_storage_is_private_allowed_and_preserved(self):
        media_path='/mnt/media "photos" %library'
        self.chosen.write_text(json.dumps({'data_dir':'/srv/custom state','storage_dir':media_path}))
        self.setup_installation()
        media=self.host/media_path.lstrip('/')
        self.assertEqual(media.stat().st_mode & 0o777,0o700)
        expected=media_path.replace('"','\\"').replace('%','%%')
        self.assertIn('ReadWritePaths="'+expected+'"',self.dropin.read_text())
        before=(media/'media-marker').read_bytes()
        self.assertEqual(self.invoke('upgrade').returncode,0)
        self.assertEqual((media/'media-marker').read_bytes(),before)
        self.assertFalse((self.state/'media').exists())

    @unittest.skipUnless(APP=='xszs','separate media storage is xszs-specific')
    def test_nonempty_external_storage_is_rejected_before_install(self):
        media=self.host/'mnt/media'
        media.mkdir(parents=True)
        (media/'existing').write_bytes(b'preserve')
        self.chosen.write_text(json.dumps({'data_dir':'/srv/custom state','storage_dir':'/mnt/media'}))
        self.assertNotEqual(self.invoke('setup',str(self.chosen)).returncode,0)
        self.assertEqual((media/'existing').read_bytes(),b'preserve')
        self.assertFalse(self.config.exists())

if __name__ == '__main__': unittest.main()
