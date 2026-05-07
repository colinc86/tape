#!/bin/sh
# PostToolUse hook: auto-format Rust files after Edit/Write/MultiEdit.
# Reads the hook event JSON on stdin, extracts tool_input.file_path,
# and runs rustfmt only if the path ends in .rs.
#
# Silent on success; rustfmt errors are swallowed so a momentarily
# unparseable edit (e.g. mid-refactor) doesn't block the tool result.

input=$(cat)
path=$(printf '%s' "$input" | jq -r '.tool_input.file_path // empty' 2>/dev/null)

case "$path" in
  *.rs)
    if command -v rustfmt >/dev/null 2>&1; then
      rustfmt --edition 2021 "$path" >/dev/null 2>&1 || true
    fi
    ;;
esac

exit 0
