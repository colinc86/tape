#!/bin/sh
# PostToolUse hook: when SPEC.md is edited, emit a reminder to
# re-run the spec-impl-auditor agent before claiming the change is done.
# Uses the additionalContext mechanism so the reminder lands in the
# assistant's transcript rather than as user-facing chatter.

input=$(cat)
path=$(printf '%s' "$input" | jq -r '.tool_input.file_path // empty' 2>/dev/null)

case "$path" in
  */SPEC.md|SPEC.md)
    cat <<'JSON'
{"hookSpecificOutput":{"hookEventName":"PostToolUse","additionalContext":"SPEC.md was edited. Run the spec-impl-auditor agent before considering this change complete — drift between SPEC.md and src/format/ is the most expensive bug class in this project."}}
JSON
    ;;
esac

exit 0
