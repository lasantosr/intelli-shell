Set Shell "bash"
Set FontSize 18
Set Width 1200
Set Height 400
Set Padding 30

Require "batcat"
Require "./target/debug/intelli-shell"

Set TypingSpeed 0

Hide
Type 'export INTELLI_SKIP_WORKSPACE=1' Sleep 0.1 Enter
Type 'export INTELLI_STORAGE="$HOME/.local/share/intelli-shell/demo.db3"' Sleep 0.1 Enter
Type 'alias intelli-shell="./target/debug/intelli-shell"' Sleep 0.1 Enter
Type 'eval "$(intelli-shell init bash)"' Sleep 0.1 Enter
Type "rm $INTELLI_STORAGE" Sleep 0.1 Enter
Type "intelli-shell import vhs/data/demo.commands" Sleep 0.1 Enter
Type "kubectl() { batcat --color=always --style=plain --paging=never vhs/data/kube.log; }" Sleep 0.1 Enter
Type "clear" Sleep 0.1 Enter
Sleep 0.5
Show

Set TypingSpeed 0.2
