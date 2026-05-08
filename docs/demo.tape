# vhs script — produces docs/demo.gif from the tape CLI.
# The .tape extension collision is real: vhs (charmbracelet) uses .tape too.
#
# Re-render with: `vhs docs/demo.tape` from the repo root, with a copy of
# tests/fixtures/killer-scenario-a.tape at ./bug-447.tape.

Output docs/demo.gif

Set FontSize 16
Set Width 1000
Set Height 560
Set Theme "Dracula"
Set Padding 18
Set TypingSpeed 22ms
Set LoopOffset 0

# ── Scene 1 ── verify + tracklist ──────────────────────────────
Type "tape verify bug-447.tape"
Sleep 100ms
Enter
Sleep 700ms

Type "tape ls bug-447.tape"
Sleep 100ms
Enter
Sleep 2.6s

# clear (Hide/Show suppresses the typing-of-clear from the GIF)
Hide
Type "clear"
Enter
Show
Sleep 200ms

# ── Scene 2 ── filter to annotations (the smoking gun) ────────
Type "tape play bug-447.tape --kind annotation"
Sleep 100ms
Enter
Sleep 2.6s

Hide
Type "clear"
Enter
Show
Sleep 200ms

# ── Scene 3 ── machine-readable verify, colorized via jq ─────
Type "tape verify --json bug-447.tape | jq -C"
Sleep 100ms
Enter
Sleep 2.4s
