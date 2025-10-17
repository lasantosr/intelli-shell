Output docs/src/images/fast.gif

# Setup
Set Framerate 24
Set Shell "bash"
Set FontSize 18
Set Width 1000
Set Height 420
Set Padding 30

# Source shared environment
Source vhs/env.tape

Env INTELLI_CONFIG "./non-existent-config.toml"

# Prepare some stored variable suggestions
Hide
Type@0 "kubectl {{namespace}} {{service}} {{10m}}" Sleep 0.1 Ctrl+L Sleep 0.5
Type@0 "my-namespace" Sleep 0.1 Enter Sleep 0.1
Type@0 "most-selected-service" Sleep 0.1 Enter Sleep 0.1
Type@0 "10m" Sleep 0.1 Enter Sleep 0.5
Ctrl+U Sleep 0.1
Type@0 "kubectl {{service}}" Sleep 0.1 Ctrl+L Sleep 0.5
Enter Sleep 0.5
Ctrl+U Sleep 0.1
Type@0 "kubectl {{service}}" Sleep 0.1 Ctrl+L Sleep 0.5
Type@0 "other-previously-selected-service" Sleep 0.1 Enter Sleep 0.5
Ctrl+U Sleep 0.1
Show

# Trigger intelli-shell search
Sleep 0.5 Ctrl+Space Sleep 0.5

# Just move the selected row while waiting to allow for the user to review
Down@0.1 2 Sleep 0.2 Down 1 Sleep 0.2 Up@0.1 3 Sleep 0.2

# Start typing to showcase search
Type "git" Sleep 0.5 Backspace 3 Sleep 0.1 Type "dock" Sleep 0.5 Backspace 4 Sleep 0.2

# Start typing the final query
Type "kube" Sleep 0.3 Type " logs" Sleep 0.5

# Select the first command
Enter

# For the first variable the cursor is already on "my-namespace", just select it
Sleep 0.5 Enter

# Filter results
Sleep 1 Type "this" Sleep 1.5

# Select "i-want-this-service"
Down@0.2 2 Enter 

# For the third variable, select a new value
Sleep 0.5 Type "5m" Sleep 0.2 Enter 

# Execute the command
Sleep 0.5 Enter

# Wait a bit before looping
Sleep 3
