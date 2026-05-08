# vhs script — produces docs/demo.gif from the tape CLI.
# Yes, the file extension collision is intentional comedy.
# Re-render with: `vhs docs/demo.tape` from the repo root.

Output docs/demo.gif

Set FontSize 16
Set Width 920
Set Height 460
Set Theme "Dracula"
Set Padding 18
Set TypingSpeed 22ms

# --- Verify
Type "tape verify bug-447.tape"
Sleep 150ms
Enter
Sleep 800ms

# --- Tracklist
Type "tape ls bug-447.tape"
Sleep 150ms
Enter
Sleep 2.4s

# --- Pull the smoking gun
Type "tape play bug-447.tape --step 4"
Sleep 150ms
Enter
Sleep 2.8s
