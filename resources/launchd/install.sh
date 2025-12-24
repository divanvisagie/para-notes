#!/bin/zsh
set -euo pipefail

label="com.divanv.para.serve"
script_dir="$(cd "$(dirname "$0")" && pwd)"
plist_src="${script_dir}/${label}.plist"
launch_agents_dir="${HOME}/Library/LaunchAgents"
plist_dest="${launch_agents_dir}/${label}.plist"

if [[ ! -f "${plist_src}" ]]; then
  echo "Missing plist: ${plist_src}" >&2
  exit 1
fi

mkdir -p "${launch_agents_dir}"
cp "${plist_src}" "${plist_dest}"

if launchctl list | rg -q "${label}"; then
  launchctl unload -w "${plist_dest}"
fi

launchctl load -w "${plist_dest}"
echo "Loaded ${label} from ${plist_dest}"
