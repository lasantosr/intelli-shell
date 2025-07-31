# Prepare some variable suggestions
Hide
Sleep 0.1 Type@0 "kubectl {{resource-name}} {{namespace}}" Sleep 0.1 Ctrl+L
Sleep 1.5 Type@0 "my-pod-name" Sleep 0.1 Enter
Sleep 0.1 Type@0 "my-custom-namespace" Sleep 0.1 Enter 
Sleep 0.1 Ctrl+U Sleep 0.1
Show

# Type kube on the terminal
Sleep 0.5 Type "kube"

# Trigger intelli-shell search
Sleep 0.5 Ctrl+Space

# Type a hashtag to list them
Sleep 0.5 Type@0.5 " #" 

# Select the third suggestion, #get
Sleep 1 Down@0.1 2 Sleep 0.5 Enter

# Select the second command
Sleep 1 Down Sleep 0.5 Enter

# For the first variable, the cursor is on new but select the second suggestion (pod)
Sleep 1.5 Down Sleep 0.5 Enter

# For the second variable the cursor is on my-pod-name, but type other name
Sleep 1 Type "other-pod-name" Sleep 0.5 Enter 

# For the third variable, select the namespace suggested
Sleep 0.5 Enter 

# For the fourth variable, select the second suggestion (json)
Sleep 0.5 Down Sleep 0.5 Enter 

# Wait a bit before looping
Sleep 3
