#!/usr/bin/env bash
set -euo pipefail

readonly archive_arg="${1:-${XSZS_RELEASE_ARCHIVE:-}}"
readonly package="xszs-1.0.3-x86_64-unknown-linux-gnu"
readonly version="1.0.3"
project_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd -P)"
readonly project_dir

test_root=""
server_pid=""

fail() {
  printf 'deployment test failed: %s\n' "$*" >&2
  exit 1
}

cleanup() {
  if [[ -n "$server_pid" ]]; then
    kill -TERM "$server_pid" 2>/dev/null || true
    wait "$server_pid" 2>/dev/null || true
  fi
  if [[ -n "$test_root" && -d "$test_root" && ! -L "$test_root" ]]; then
    chmod -R u+rwX "$test_root" 2>/dev/null || true
    rm -rf -- "$test_root"
  fi
}
trap cleanup EXIT

[[ -n "$archive_arg" ]] ||
  fail "usage: test-deployment.sh /absolute/path/xszs-1.0.3-x86_64-unknown-linux-gnu.tar.gz"
[[ "$archive_arg" = /* && -f "$archive_arg" && ! -L "$archive_arg" ]] ||
  fail "release archive must be an absolute regular non-symlink file"
[[ "$(stat -c '%h' -- "$archive_arg")" == "1" ]] || fail "release archive has a hard-link alias"
archive="$(cd "$(dirname "$archive_arg")" && pwd -P)/$(basename "$archive_arg")"
[[ "$(basename "$archive")" == "$package.tar.gz" ]] || fail "unexpected release archive name"

test_root="$(mktemp -d)"
extract_dir="$test_root/extracted"
mkdir -m 0755 "$extract_dir"
if tar -tzf "$archive" | awk -F/ -v package="$package" '
  $1 != package || $0 ~ /(^|\/)\.\.?(\/|$)/ || $0 ~ /^\// { exit 1 }
'; then
  :
else
  fail "archive contains an unexpected root or non-normal path"
fi
tar --no-same-owner -xzf "$archive" -C "$extract_dir"
relocated_releases="$test_root/extracted-again"
mkdir -p "$relocated_releases"
release_root="$relocated_releases/$version"
mv -T -- "$extract_dir/$package" "$release_root"
real_binary="$release_root/bin/xszs"
setup_script="$release_root/install.sh"
unit_source="$release_root/systemd/xszs.service"
[[ -d "$release_root" && ! -L "$release_root" && -x "$real_binary" ]] ||
  fail "archive does not contain the real server layout"

identity_file="$test_root/identity.json"
env -i PATH="$PATH" "$real_binary" release-identity >"$identity_file"
python3 - "$identity_file" <<'PY'
import json
from pathlib import Path
import re
import sys

identity = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
expected_keys = {
    "product", "version", "source_revision", "target", "api_version",
    "storage_encoding", "server_schema_revision", "server_schema_sha256",
    "web_assets_sha256",
    "release_contract_sha256",
}
assert isinstance(identity, dict) and set(identity) == expected_keys
assert identity["product"] == "xszs"
assert identity["version"] == "1.0.3"
assert re.fullmatch(r"[0-9a-f]{40}", identity["source_revision"])
assert identity["target"] == "x86_64-unknown-linux-gnu"
assert identity["api_version"] == "v1"
assert identity["storage_encoding"] == "plain-v1"
assert identity["server_schema_revision"] == 1
assert identity["server_schema_sha256"] == "0e37f8a3992b1904215d5f4c9ac428448752718506611f818482d890322d300d"
PY
contract="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["release_contract_sha256"])' "$identity_file")"
source_revision="$(python3 -c 'import json,sys; print(json.load(open(sys.argv[1]))["source_revision"])' "$identity_file")"
verification="$($real_binary release-verify "$release_root")"
[[ "$verification" == XSZS_RELEASE_VERIFIED_V1$'\txszs\t'"$version"$'\t'"$source_revision"$'\tx86_64-unknown-linux-gnu\t'"$contract" ]] ||
  fail "real binary did not verify the extracted archive identity"
archive_digest="$(sha256sum "$archive" | awk '{print $1}')"
if "$project_dir/scripts/build-server-release.sh" "$real_binary" "$source_revision" \
  "$(dirname "$archive")" >/dev/null 2>&1; then
  fail "release builder reused an existing archive path"
fi
[[ "$(sha256sum "$archive" | awk '{print $1}')" == "$archive_digest" ]] ||
  fail "release builder changed an existing archive"
"$real_binary" web-assets >"$test_root/compiled-web-assets.json"
cmp --silent "$test_root/compiled-web-assets.json" "$release_root/share/web-assets.json" ||
  fail "release inventory differs from the actual executable"
python3 - "$release_root/share/web-assets.json" "$identity_file" <<'PYWEB'
import hashlib,json,sys
from pathlib import Path
raw=Path(sys.argv[1]).read_bytes(); manifest=json.loads(raw); identity=json.load(open(sys.argv[2]))
assert hashlib.sha256(raw).hexdigest()==identity['web_assets_sha256']
assert manifest['format']=='web-assets-v1'
paths=[entry['path'] for entry in manifest['files']]
assert paths==sorted(set(paths))
assert {'index.html','assets/admin.js','assets/admin.css','assets/MapleMono-OFL.txt','assets/CJK-LICENSE.txt'}<=set(paths)
assert any(path.endswith('.woff2') for path in paths)
PYWEB
python3 - "$release_root" <<'PY'
from pathlib import Path
import re
import sys

root = Path(sys.argv[1])
readme = (root / "README.md").read_text(encoding="utf-8")
references = set(re.findall(r"scripts/[A-Za-z0-9._-]+\.sh", readme))
assert "install.sh setup" in readme
assert (root / "install.sh").is_file()
for reference in references:
    assert (root / reference).is_file(), f"release README references missing {reference}"
PY

# A source-bound binary runs from an ordinary installation without a manifest.
smoke_root="$test_root/smoke"
mkdir -m 0700 "$smoke_root"
smoke_port="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')"
installed_binary="$test_root/ordinary-bin/xszs"
mkdir -m 0755 "$(dirname "$installed_binary")"
install -m 0755 "$real_binary" "$installed_binary"
python3 - "$test_root/smoke.json" "$smoke_root" "$smoke_port" <<'PYCONFIG'
import json,os,sys
path,data,port=sys.argv[1:]
with open(path,'w') as output:
    json.dump({'data_dir':data,'bind':f'127.0.0.1:{port}','development':False},output)
os.chmod(path,0o600)
PYCONFIG
printf '%s\n' deployment-smoke-password | "$installed_binary" init --config "$test_root/smoke.json" --username admin --json
(
  cd /
  exec "$installed_binary" run --config "$test_root/smoke.json"
) >"$test_root/server.log" 2>&1 &
server_pid="$!"
smoke_ready=0
for _ in {1..120}; do
  if curl --silent --fail "http://127.0.0.1:$smoke_port/readyz" >/dev/null; then
    smoke_ready=1
    break
  fi
  if ! kill -0 "$server_pid" 2>/dev/null; then
    cat "$test_root/server.log" >&2
    fail "real release binary exited during HTTP smoke"
  fi
  sleep 0.1
done
[[ "$smoke_ready" == "1" ]] || {
  cat "$test_root/server.log" >&2
  fail "real release binary did not become healthy"
}
[[ "$(curl --silent --output /dev/null --write-out '%{http_code}' "http://127.0.0.1:$smoke_port/admin")" == "200" ]] ||
  fail "real release binary did not serve its embedded admin web"
python3 - "$release_root/share/web-assets.json" "http://127.0.0.1:$smoke_port/admin" <<'PYHTTP'
import hashlib,json,sys,urllib.request,urllib.error
inventory=json.load(open(sys.argv[1]))
for asset in inventory['files']:
    url=sys.argv[2]+'/'+asset['path'] if asset['path']!='index.html' else sys.argv[2]
    with urllib.request.urlopen(url) as response:
        body=response.read()
        assert len(body)==asset['size'] and hashlib.sha256(body).hexdigest()==asset['sha256'],asset['path']
        assert response.headers['content-type']==asset['content_type'],asset['path']
        assert response.headers['x-content-type-options']=='nosniff'
        etag=response.headers['etag']
        if asset['path']=='index.html':
            assert 'no-store' in response.headers['cache-control']
            assert "script-src 'self'" in response.headers['content-security-policy']
    with urllib.request.urlopen(urllib.request.Request(url,method='HEAD')) as response:
        assert not response.read() and int(response.headers['content-length'])==asset['size']
    if not asset['path'].endswith('.html'):
        try: urllib.request.urlopen(urllib.request.Request(url,headers={'If-None-Match':etag}))
        except urllib.error.HTTPError as error: assert error.code==304 and not error.read()
        else: raise AssertionError('no 304 response for '+asset['path'])
try: urllib.request.urlopen(sys.argv[2]+'/assets/missing-resource.js')
except urllib.error.HTTPError as error: assert error.code==404
else: raise AssertionError('missing asset not 404')
PYHTTP
kill -TERM "$server_pid"
wait "$server_pid" || true
server_pid=""
[[ -f "$smoke_root/xszs.sqlite3" ]] || fail "real release smoke did not create SQLite state"

expect_invalid_source() {
  local label="$1"
  local candidate="$2"
  if "$real_binary" release-verify "$candidate" >/dev/null 2>&1; then
    fail "$label was accepted by the trusted real verifier"
  fi

}

negative_root="$test_root/negative"
fresh_negative() {
  if [[ -e "$negative_root" ]]; then
    rm -rf -- "$negative_root"
  fi
  cp -a -- "$release_root" "$negative_root"
}

fresh_negative
printf '#!/usr/bin/env bash\nexit 0\n' >"$negative_root/bin/xszs"
chmod 0755 "$negative_root/bin/xszs"
expect_invalid_source fake-binary "$negative_root"

fresh_negative
printf '\ntampered\n' >>"$negative_root/README.md"
expect_invalid_source tampered-file "$negative_root"

fresh_negative
touch "$negative_root/EXTRA"
chmod 0644 "$negative_root/EXTRA"
expect_invalid_source extra-file "$negative_root"

fresh_negative
rm "$negative_root/share/web-assets.json"
expect_invalid_source missing-file "$negative_root"

fresh_negative
printf '\ntampered\n' >>"$negative_root/share/web-assets.json"
expect_invalid_source tampered-web-inventory "$negative_root"

# A self-consistent replacement manifest cannot authorize bytes absent from the binary.
fresh_negative
printf '\ntampered\n' >>"$negative_root/share/web-assets.json"
rm "$negative_root/release-manifest.json"
python3 "$project_dir/scripts/write-release-manifest.py" "$negative_root" "$source_revision"
expect_invalid_source rehashed-web-inventory "$negative_root"

# Even a fully self-consistent substituted inventory and identity cannot change the executing binary's assets.
fresh_negative
printf '\ntampered\n' >>"$negative_root/share/web-assets.json"
rm "$negative_root/release-manifest.json"
python3 "$project_dir/scripts/write-release-manifest.py" "$negative_root" "$source_revision"
python3 - "$negative_root" <<'PYFORGE'
import hashlib,json,sys
from pathlib import Path
root=Path(sys.argv[1]); path=root/'release-manifest.json'; manifest=json.loads(path.read_text()); identity=manifest['identity']
identity['web_assets_sha256']=hashlib.sha256((root/'share/web-assets.json').read_bytes()).hexdigest()
contract=(f'product={identity["product"]}\nversion={identity["version"]}\napi_version={identity["api_version"]}\nstorage_encoding={identity["storage_encoding"]}\nserver_schema_revision={identity["server_schema_revision"]}\nserver_schema_sha256={identity["server_schema_sha256"]}\nweb_assets_sha256={identity["web_assets_sha256"]}\n')
identity['release_contract_sha256']=hashlib.sha256(contract.encode()).hexdigest()
path.write_text(json.dumps(manifest)+'\n')
PYFORGE
expect_invalid_source forged-web-identity "$negative_root"

fresh_negative
rm "$negative_root/share/web-assets.json"
expect_invalid_source missing-web-inventory "$negative_root"

fresh_negative
chmod 0664 "$negative_root/README.md"
expect_invalid_source writable-payload "$negative_root"

fresh_negative
chmod 4755 "$negative_root/bin/xszs"
expect_invalid_source privileged-mode-payload "$negative_root"

fresh_negative
ln "$negative_root/README.md" "$negative_root/docs/README.alias"
expect_invalid_source hard-linked-payload "$negative_root"

fresh_negative
rm "$negative_root/share/web-assets.json"
ln -s /etc/passwd "$negative_root/share/web-assets.json"
expect_invalid_source symlinked-payload "$negative_root"

fresh_negative
rm "$negative_root/share/web-assets.json"
mkfifo "$negative_root/share/web-assets.json"
expect_invalid_source special-payload "$negative_root"

fresh_negative
python3 - "$negative_root/release-manifest.json" version <<'PY'
import json, pathlib, sys
path = pathlib.Path(sys.argv[1]); value = json.loads(path.read_text())
value["identity"][sys.argv[2]] = "9.9.9"
path.write_text(json.dumps(value) + "\n")
PY
expect_invalid_source wrong-version "$negative_root"

fresh_negative
python3 - "$negative_root/release-manifest.json" product <<'PY'
import json, pathlib, sys
path = pathlib.Path(sys.argv[1]); value = json.loads(path.read_text())
value["identity"][sys.argv[2]] = "not-xszs"
path.write_text(json.dumps(value) + "\n")
PY
expect_invalid_source wrong-product "$negative_root"

fresh_negative
python3 - "$negative_root/release-manifest.json" top <<'PY'
import json, pathlib, sys
path = pathlib.Path(sys.argv[1]); value = json.loads(path.read_text())
value["unexpected"] = True
path.write_text(json.dumps(value) + "\n")
PY
expect_invalid_source unknown-top-field "$negative_root"

fresh_negative
python3 - "$negative_root/release-manifest.json" nested <<'PY'
import json, pathlib, sys
path = pathlib.Path(sys.argv[1]); value = json.loads(path.read_text())
value["identity"]["unknown_alias"] = "unexpected"
path.write_text(json.dumps(value) + "\n")
PY
expect_invalid_source unknown-identity-field "$negative_root"

fresh_negative
python3 - "$negative_root/release-manifest.json" file <<'PY'
import json, pathlib, sys
path = pathlib.Path(sys.argv[1]); value = json.loads(path.read_text())
value["files"][0]["unknown_path"] = value["files"][0]["path"]
path.write_text(json.dumps(value) + "\n")
PY
expect_invalid_source unknown-file-field "$negative_root"

python3 "$project_dir/scripts/test-installation.py"
python3 "$project_dir/scripts/test_release_manifest.py"
bash -n "$setup_script" "$release_root/scripts/run-server-wsl.sh" \
  "$release_root/scripts/start-server-wsl.sh" "$release_root/scripts/verify-server-wsl.sh" "$0"
if [[ "${XSZS_VERIFY_SYSTEMD:-0}" == "1" ]]; then
  install_root="$test_root/unit-root"
  mkdir "$install_root"
  printf '%s\n' deployment-smoke-password | XSZS_INSTALL_ROOT="$install_root" XSZS_INSTALL_TEST=1 "$setup_script" setup "$release_root/config/xszs.json.example" admin
  systemd-analyze --root="$install_root" --recursive-errors=no verify "$install_root/etc/systemd/system/xszs.service"
fi
printf 'release passed explicit identity/tamper verification, ordinary-binary HTTP smoke and setup/upgrade preservation tests\n'
