#!/usr/bin/env bash
#
# Stop hook: a turn cannot end while `make check` fails.
#
# Exit 0 -> turn ends normally.
# Exit 2 -> stderr goes back to Claude as the reason to keep working.

set -uo pipefail

input=$(cat)

# A retry this hook already triggered; honouring it prevents an endless loop.
if [ "$(printf '%s' "$input" | jq -r '.stop_hook_active // false')" = "true" ]; then
	exit 0
fi

project_dir="${CLAUDE_PROJECT_DIR:-$(git rev-parse --show-toplevel 2>/dev/null)}"
[ -d "$project_dir" ] || exit 0
cd "$project_dir" || exit 0

# Only run when something the gate reads changed (uncommitted, or committed on
# this branch but not on main). Docs-only and question turns stay fast.
watched=(src data Cargo.toml Cargo.lock Makefile)
dirty=$(git status --porcelain -- "${watched[@]}" 2>/dev/null)
ahead=""
if git rev-parse --verify -q main >/dev/null; then
	ahead=$(git diff --name-only main...HEAD -- "${watched[@]}" 2>/dev/null)
fi
[ -z "$dirty$ahead" ] && exit 0

if output=$(make check 2>&1); then
	exit 0
fi

cat >&2 <<MSG
\`make check\` is failing, so this work is not done. Fix the errors below and
run \`make check\` again. Do not weaken a test or silence a lint to pass it.

$(printf '%s' "$output" | tail -n 80)
MSG

exit 2
