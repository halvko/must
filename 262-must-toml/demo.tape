# #246: `must run` loads the library under must.toml, so day1 can use util.
Set Shell bash
Set FontSize 20
Set Width 1000
Set Height 330
Set Padding 16
Set TypingSpeed 60ms
Hide
Type "PS1='$ '; clear" Enter
Sleep 500ms
Show
Type "ls; cat day1.must" Enter
Sleep 1.5s
Type "must run day1.must" Enter
Sleep 4s
