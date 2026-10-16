import assert from 'node:assert/strict';
import { test } from 'node:test';
import { actionLabel, lastSeen, pairingLabel } from '../src/display-labels.ts';

test('pairing states and audit actions use display labels while unknown values have a fallback', () => {
  assert.equal(pairingLabel('pending'), 'Awaiting pairing');
  assert.equal(pairingLabel('paired'), 'Paired');
  assert.equal(pairingLabel('cancelled'), 'Cancelled');
  assert.equal(pairingLabel('revoked'), 'Revoked');
  assert.equal(pairingLabel('future_state'), 'Unknown');
  assert.equal(pairingLabel('__proto__'), 'Unknown');
  assert.equal(actionLabel('device.authorization.rotate'), 'Change password');
  assert.equal(actionLabel('asset.trash'), 'Move to trash');
  assert.equal(actionLabel('future.action'), 'Unrecognized action');
  assert.equal(actionLabel('constructor'), 'Unrecognized action');
});

test('last-seen display formats explicit instants in UTC and rejects unzoned timestamps', () => {
  const expected = `${new Date('2026-10-08T08:00:00Z').toLocaleString('en', { timeZone: 'UTC', hourCycle: 'h23' })} UTC`;
  assert.equal(lastSeen('2026-10-08 08:00:00'), 'Unknown');
  assert.equal(lastSeen('2026-10-08T08:00:00Z'), expected);
  assert.equal(lastSeen('2026-10-08T16:00:00+08:00'), expected);
  assert.equal(lastSeen(''), 'Not paired yet');
  assert.equal(lastSeen('invalid-date'), 'Unknown');
});
