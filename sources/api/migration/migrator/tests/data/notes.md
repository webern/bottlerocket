

## Creating the Root.json

```bash
export MIG="${REPOS}/br/sources/api/migration/migrator"
export DAT="${MIG}/tests/data"
export TUFTOOL="${REPOS}/tough/tuftool"
export ROOT="${DAT}/root.json"
export TUF_IN="${HOME}/tmp/mig/in"
export TUF_OUT="${DAT}/repository"
cd "${TUFTOOL}"

# Setup root.json (only needs to be done once)

cargo run -- root init "${ROOT}"
cargo run -- root expire "${ROOT}" "2020-05-17T00:00:00Z"
cargo run -- root set-threshold "${ROOT}" root 1
cargo run -- root set-threshold "${ROOT}" snapshot 1
cargo run -- root set-threshold "${ROOT}" targets 1
cargo run -- root set-threshold "${ROOT}" timestamp 1
cargo run -- root gen-rsa-key "${ROOT}" "${DAT}/root.pem" --role root
cargo run -- root gen-rsa-key "${ROOT}" "${DAT}/snapshot.pem" --role snapshot
cargo run -- root gen-rsa-key "${ROOT}" "${DAT}/targets.pem" --role targets
cargo run -- root gen-rsa-key "${ROOT}" "${DAT}/timestamp.pem" --role timestamp
cargo run -- sign "${ROOT}" --root "${ROOT}" --key "${DAT}/root.pem"

# Create the targets

rm -rf "${TUF_IN}"
mkdir -p "${TUF_IN}"
lz4 -z "${DAT}/x-first-migration.sh" "${TUF_IN}/x-first-migration.lz4"
lz4 -z "${DAT}/a-second-migration.sh" "${TUF_IN}/a-second-migration.lz4"
cp "${DAT}/manifest.json" "${TUF_IN}/manifest.json" 

rm -rf "${TUF_OUT}"
cargo run -- create \
  "${TUF_IN}/" "${TUF_OUT}/" \
  -k "file://${DAT}/root.pem" \
  -k "file://${DAT}/snapshot.pem" \
  -k "file://${DAT}/targets.pem" \
  -k "file://${DAT}/timestamp.pem" \
  --root "${ROOT}" \
  --targets-expires 'in 3 days' --targets-version $(date +%s) \
  --snapshot-expires 'in 4 days' --snapshot-version $(date +%s) \
  --timestamp-expires 'in 5 days' --timestamp-version $(date +%s)
ls -al "${TUF_OUT}"
ls -al "${TUF_OUT}/metadata"
ls -al "${TUF_OUT}/targets"
```