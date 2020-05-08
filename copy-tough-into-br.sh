#!/bin/bash
set -e
TOUGH_SRC="${REPOS}/tough"
BR="${REPOS}/br"
IGNORED="${BR}/sources/ignored"
TOUGH_DST="${IGNORED}/tough"

echo "TOUGH_SRC=${TOUGH_SRC}"
echo "TOUGH_DST=${TOUGH_DST}"

# delete tough's build artifacts before copying
sudo rm -rf "${TOUGH_SRC}/target"

# ensure the directory is there to receive the tough sourcecode
sudo rm -rf "${TOUGH_DST}"
sudo mkdir -p "${TOUGH_DST}"

sudo cp -a "${TOUGH_SRC}" "${IGNORED}"
sudo rm -rf "${TOUGH_DST}/.git"
sudo rm -rf "${TOUGH_DST}/.idea"
rm "${TOUGH_DST}/Cargo.toml"
rm "${TOUGH_DST}/Cargo.lock"
rm -rf "${TOUGH_DST}/tuftool"
sudo chown -R "$(whoami)" "${TOUGH_DST}"
ls -al "${IGNORED}"

# annoying
sed 's/mockito = "0.25"/#mockito = "0.25"/g' \
    "${TOUGH_DST}/tough/Cargo.toml" > \
    "${TOUGH_DST}/tough/Cargo.toml.new"

mv "${TOUGH_DST}/tough/Cargo.toml" "${TOUGH_DST}/tough/Cargo.toml.orig"
mv "${TOUGH_DST}/tough/Cargo.toml.new" "${TOUGH_DST}/tough/Cargo.toml"
