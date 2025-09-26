# --- Nushell Integration (using Reedline - Nu's Line Editor) ---
# https://www.nushell.sh/commands/docs/commandline.html
# https://www.nushell.sh/book/line_editor.html#keybindings

# Define key bindings, using defaults if environment variables are not set.
# NOTE: The format can be "modifier keycode" (e.g., "control char_b") or just a keycode for keys without modifiers.
#       See `keybindings list` for available modifiers and keycodes or `keybindings listen` to check.
let intelli_search_key = ($env.INTELLI_SEARCH_HOTKEY? | default "control space")
let intelli_bookmark_key = ($env.INTELLI_BOOKMARK_HOTKEY? | default "control char_b")
let intelli_variable_key = ($env.INTELLI_VARIABLE_HOTKEY? | default "control char_l")
let intelli_fix_key = ($env.INTELLI_FIX_HOTKEY? | default "control char_x")

# Helper function to execute intelli-shell and update the command line buffer.
def _intelli_exec [
  command: string         # The intelli-shell command to run (e.g., "search", "new")
  args: list<string>      # Arguments for the intelli-shell command
] {
  let temp_file = (mktemp -t "intelli-shell.XXXXXX")

  # Clear the buffer with ANSI escapes to be rendered immediately
  let buffer_len = (commandline | str length)
  if $buffer_len > 0 {
    print -n $"\e[($buffer_len)D\e[K"
    commandline edit --replace ""
  }

  # Run intelli-shell
  let exit_code = (try {
    intelli-shell --extra-line --skip-execution --file-output $temp_file $command ...$args
    0
  } catch {
    $env.LAST_EXIT_CODE
  })
  
  # If the output file is missing or empty, there's nothing to process (likely a crash)
  if not (($temp_file | path exists) and ($temp_file | path type) == file and (ls $temp_file | get size | first) > 0b) {
    # Panic report was likely printed, but nu will also display a new prompt automatically
    rm -f $temp_file
    return
  }

  # Read the file content and parse it
  let lines = (open $temp_file | lines)
  rm -f $temp_file
  let out_status = ($lines | get 0)
  let action = if ($lines | length) > 1 { ($lines | get 1) } else { "" }
  let command_out = if ($lines | length) > 2 { ($lines | skip 2 | str join "\n") } else { "" }
  
  # Nu always starts a new prompt, but it does so on the same line if a single line is printed to stderr
  if $out_status == "DIRTY" or $exit_code != 0 {
    # When dirty, we move the cursor one extra line down so that the prompt is not redisplayed over the stderr output
    print -n "\u{1b}[1B"
  } else if $out_status == "CLEAN" {
    # intelli-shell always leaves the cursor at the end of the prompt but nu doesn't properly redisplay it
    # Move the cursor to the beginning of the prompt so that it's properly rendered where it should be
    let prompt_output = (do $env.PROMPT_COMMAND)
    let num_lines = ($prompt_output | lines | length)
    let move_up_code = if $num_lines > 1 {
      let lines_to_move = $num_lines - 1
      $"\u{1b}[($lines_to_move)A"
    } else {
      ""
    }
    print -n $"($move_up_code)\r"
  }

  # Determine the content of the buffer
  if $action == "REPLACE" {
    commandline edit --replace $command_out
  } else if $action == "EXECUTE" {
    commandline edit --replace $command_out --accept
  }
}

# Wrapper function for the search keybinding
def --env "intelli-search" [] {
  _intelli_exec "search" ["-i", (commandline)]
}

# Wrapper function for the bookmark/save keybinding
def --env "intelli-save" [] {
  _intelli_exec "new" ["-i", (commandline)]
}

# Wrapper function for the variable replacement keybinding
def --env "intelli-variable" [] {
  _intelli_exec "replace" ["-i", (commandline)]
}

# Wrapper function for the command fixing keybinding
def --env "intelli-fix" [] {
  let hist = (history | last 5 | each { |it| $it.command } | str join "\n")
  _intelli_exec "fix" ["--history", $hist, (commandline)]
}

# Define the bindings and their corresponding commands in a list for dynamic generation
let keybinding_definitions = [
  { name: "intelli-search", binding: $intelli_search_key, command: "intelli-search" },
  { name: "intelli-bookmark", binding: $intelli_bookmark_key, command: "intelli-save" },
  { name: "intelli-variable", binding: $intelli_variable_key, command: "intelli-variable" },
  { name: "intelli-fix", binding: $intelli_fix_key, command: "intelli-fix" },
]

# Dynamically generate the final keybinding records
let intelli_keybindings = ($keybinding_definitions | each { |kb|
  let binding_parts = ($kb.binding | split row " " -n 2)
  let modifier = if ($binding_parts | length) == 2 {
    ($binding_parts | first)
  } else {
    "none"
  }
  let keycode = ($binding_parts | last)
  {
    name: $kb.name,
    modifier: $modifier,
    keycode: $keycode,
    mode: ["emacs", "vi_insert"]
    event: { 
      send: ExecuteHostCommand
      cmd: $kb.command
    }
  }
})

# Append the new keybindings to the config
$env.config.keybindings ++= $intelli_keybindings

# Bind ESC to kill the whole line if not skipped
if ($env.INTELLI_SKIP_ESC_BIND? | default "0") == "0" {
  $env.config.keybindings ++= [{
    name: "kill-line",
    modifier: "none",
    keycode: "esc",
    mode: ["emacs", "vi_insert"],
    event: [
      { edit: SelectAll }
      { edit: Delete }
    ]
  }]
}
