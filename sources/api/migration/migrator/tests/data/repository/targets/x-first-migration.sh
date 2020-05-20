#!/bin/bash
set -eo pipefail
migration_name="x-first-migration"
datastore_parent_dir="$(dirname "${3}")"
outfile="${datastore_parent_dir}/result.txt"
echo "${migration_name}: writing a message to '${outfile}'"
echo "${migration_name}:" "${@}" >> "${outfile}"
