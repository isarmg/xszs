"""XSZS assertions for XSSI's real installed lifecycle gate (no database fixtures)."""
import json

PRODUCT = 'xszs'
SERVICES = ('xszs.service',)
ACCOUNT = 'isarmg-xszs'
RETAINED_FILES = ('state/xszs.sqlite3', 'state/secrets.json')


def configure(config, root, shared, port):
    config.update(bind=f'127.0.0.1:{port}', development=False)


def api(request, path, method='GET', body=None, csrf=None):
    headers = {'Content-Type': 'application/json'}
    if csrf:
        headers['X-CSRF-Token'] = csrf
    status, response = request(path, method, None if body is None else json.dumps(body).encode(), headers)
    if status not in (200, 201):
        raise RuntimeError('Unexpected business API status')
    return json.loads(response)


def exercise(request, session, sensitive, require):
    instance = api(request, '/api/v1/admin/instances', 'POST',
                   {'name': 'Installed lifecycle instance'}, session['csrf_token'])
    require(instance['name'] == 'Installed lifecycle instance', 'Instance creation differs')
    code = instance['authorization_code']
    require(isinstance(code, str) and bool(code), 'Instance authorization is missing')
    sensitive.append(code)
    overview = api(request, '/api/v1/admin/overview')
    instances = [item for user in overview['users'] for item in user['instances']]
    require(instance in instances, 'Created instance missing from administrator overview')
    logs = api(request, '/api/v1/admin/logs')
    events = [event for event in logs['logs']
              if event['action'] == 'device.instance.create' and event['entity_id'] == instance['id']]
    require(len(events) == 1, 'Instance creation audit event missing')
    # Pin the UTC date so a restart across midnight checks the same audit window.
    return {'overview': overview, 'events': events,
            'logs_path': '/api/v1/admin/logs?date=' + logs['date']}


def persisted(request, session, state, require):
    current = api(request, '/api/v1/auth/session')
    require(current['authenticated'] is True and current['user_id'] == session['user_id'],
            'Administrator session did not survive restart')
    require(api(request, '/api/v1/admin/overview') == state['overview'],
            'Instance state did not survive restart')
    logs = api(request, state['logs_path'])
    require(all(event in logs['logs'] for event in state['events']),
            'Instance creation audit event did not survive restart')


def retained(root, shared, state, require):
    require(all((root / name).is_file() for name in RETAINED_FILES),
            'Operator uninstall did not retain the database and instance secrets')
