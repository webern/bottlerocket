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

sudo cp -a "${TOUGH_SRC}" "${TOUGH_DST}"
sudo rm -rf "${TOUGH_DST}/.git"
sudo rm -rf "${TOUGH_DST}/.idea"
sudo chown -R "$(whoami)" "${TOUGH_DST}"
ls -al "${IGNORED}"