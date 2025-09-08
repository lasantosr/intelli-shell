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
Sleep 1 Ctrl+Space Sleep 1

# Just move the selected row while waiting to allow for the user to review
Down@0.5 2 Sleep 1 Down 1 Sleep 0.5 Up 3 Sleep 0.5

# Start typing to showcase search
Type "git" Sleep 2 Backspace 3 Sleep 1 Type "dock" Sleep 1 Backspace 4 Sleep 1

# Start typing the final query
Type "kube" Sleep 0.5 Type " logs" Sleep 2

# Select the first command
Enter

# For the first variable the cursor is already on "my-namespace", just select it
Sleep 1 Enter

# Move cursor while waiting for completions
Sleep 1.5 Down

# Move again to highlight an on-the-fly suggestion
Sleep 1 Down Sleep 2

# Filter results
Type "this" Sleep 2 

# Select "i-want-this-service"
Down@0.5 2 Enter 

# For the third variable, select a new value
Sleep 1 Type "5m" Sleep 1 Enter 

# Execute the command
Sleep 1 Enter

# Wait a bit before looping
Sleep 5
