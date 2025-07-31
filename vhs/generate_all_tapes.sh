#!/bin/bash

# A script to generate all vhs gifs from tapes that also merges all theme gifs into a single demo one
set -euo pipefail

# --- Argument Parsing ---
MERGE_ONLY=false
if [[ "${1-}" = "--merge-only" ]]; then
  MERGE_ONLY=true
  echo "🏃 Running in --merge-only mode. Skipping generation."
fi

# Validate required tools
if ! command -v ffmpeg &> /dev/null || ! command -v ffprobe &> /dev/null || ! command -v script &>/dev/null; then
  echo "Error: This script requires 'ffmpeg', 'ffprobe' and 'script'" >&2
  exit 1
fi

# Get the directory where the script is located
ORIGINAL_DIR=$(pwd)
SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
PROJECT_ROOT=$(dirname "$SCRIPT_DIR")

# --- Cleanup & Trap Logic ---
cleanup() {
  # Always try to restore cursor, ignore errors
  tput cnorm &>/dev/null || true

  # Return to the original directory
  if [[ "$(pwd)" != "$ORIGINAL_DIR" ]]; then
    cd "$ORIGINAL_DIR"
  fi
}
# Trap EXIT, which is triggered on any exit (normal, error, or signal like INT/TERM)
trap cleanup EXIT

# --- Helper Function to run a command and show only the last N lines of its output ---
# Usage: run_and_show_last_n <lines> <command> [args...]
run_and_show_last_n() {
  local lines="$1"
  shift
  local cmd=("$@")
  local last_n_lines=()
  local exit_code_file
  local exit_code

  # Safely quote the command and its arguments
  local quoted_cmd=$(printf '%q ' "${cmd[@]}")

  # Append a command to echo the exit code to file descriptor 3
  local full_script_cmd="$quoted_cmd; echo \$? >&3"

  # Create a temporary file to store the command's exit code
  exit_code_file=$(mktemp) || return 1

  # Set up a trap to ensure the temporary file is removed when the function exits
  trap 'rm -f "$exit_code_file"' RETURN

  # Hide cursor and save its position
  tput civis
  tput sc

  # The `while` loop reads the command's output line-by-line.
  while IFS= read -r line; do
    # Add the new line to our array
    last_n_lines+=("$line")
    # If the array is too long, remove the oldest line
    if ((${#last_n_lines[@]} > lines)); then
      last_n_lines=("${last_n_lines[@]:1}")
    fi
    # Restore cursor position, clear from cursor to end of screen, and print lines
    tput rc
    tput ed
    printf "        %s\n" "${last_n_lines[@]}"
  done < <((script -q /dev/null -c "$full_script_cmd" 2>&1) 3>"$exit_code_file")

  # Read the captured exit code from the temporary file
  exit_code=$(<"$exit_code_file")

  # Check the exit code
  if [[ $exit_code -eq 0 ]]; then
    # On success, clear the output area for a clean finish
    tput rc
    tput ed
  else
    # On failure, the output remains visible
    echo
  fi

  # Always restore the cursor's visibility
  tput cnorm

  # Return the original command's exit code
  return $exit_code
}

# --- Change to Project Root ---
# All subsequent commands will run from the script's parent directory
if [[ "$(pwd)" != "$PROJECT_ROOT" ]]; then
  echo "📂 Running from project root: ${PROJECT_ROOT}"
  cd "${PROJECT_ROOT}"
fi

if [ "$MERGE_ONLY" = false ]; then
  ## --- Clean up existing images ---
  echo "🗑️  Removing existing GIF files ..."
  rm -f "docs/src/images/"*.gif

  ## --- Generate all gifs from tapes ---
  echo "📼 Recording VHS tapes ..."
  find "vhs/tapes" -name "*.tape" -print0 | sort -z | while IFS= read -r -d '' tape_file; do
    echo "   ⚙️  Processing ${tape_file##*/}"
    run_and_show_last_n 10 vhs "$tape_file" < /dev/null
  done
fi

## --- Merge Theme GIF files ---
OUTPUT_FILE="docs/src/images/demo.gif"
INPUT_FILES=(docs/src/images/theme_*.gif)

# Calculations
NUM_FILES=${#INPUT_FILES[@]}
TOTAL_FRAMES=$(ffprobe -v error -select_streams v:0 -count_frames -show_entries stream=nb_read_frames -of default=noprint_wrappers=1:nokey=1 "${INPUT_FILES[0]}")
FPS_FRAC=$(ffprobe -v error -select_streams v:0 -show_entries stream=r_frame_rate -of default=noprint_wrappers=1:nokey=1 "${INPUT_FILES[0]}")
FPS=$(echo "$FPS_FRAC" | LC_NUMERIC=C bc -l)

# Safety check to prevent division by zero
if (( $(echo "$FPS == 0" | LC_NUMERIC=C bc -l) )); then
  echo "Error: Could not determine a valid frame rate from the input file, exiting" >&2
  exit 1
fi

CHUNK_SIZE=$((TOTAL_FRAMES / NUM_FILES))

echo "🔎 Found ${NUM_FILES} theme GIFs to process"
echo "📊 Detected ${TOTAL_FRAMES} frames at $(printf "%.2f" "$FPS") FPS, using chunks of ~${CHUNK_SIZE} frames"

# Build FFmpeg Command
FFMPEG_INPUTS=""
FILTER_COMPLEX=""
CONCAT_STREAMS=""

for i in "${!INPUT_FILES[@]}"; do
  CURRENT_FILE="${INPUT_FILES[$i]}"
  FFMPEG_INPUTS+=" -i ${CURRENT_FILE}"

  START_FRAME=$((i * CHUNK_SIZE))
  CHUNK_DURATION=$(echo "$CHUNK_SIZE / $FPS" | LC_NUMERIC=C bc -l)
  START_TIME=$(echo "$START_FRAME / $FPS" | LC_NUMERIC=C bc -l)

  echo "   ✂️  Chunk ${i}: Taking ~$(printf "%.2f" "$CHUNK_DURATION")s from ${CURRENT_FILE} (starting at $(printf "%.2f" "$START_TIME")s)"

  FILTER_COMPLEX+="[${i}:v]trim=start=${START_TIME}:duration=${CHUNK_DURATION},setpts=PTS-STARTPTS[v${i}];"
  CONCAT_STREAMS+="[v${i}]"
done

FILTER_COMPLEX+="${CONCAT_STREAMS}concat=n=${NUM_FILES}:v=1:a=0,split[s0][s1];[s0]palettegen[p];[s1][p]paletteuse"

# Execute Command
echo "✨ Generating ${OUTPUT_FILE} with FFmpeg ..."
run_and_show_last_n 10 ffmpeg -y ${FFMPEG_INPUTS} -filter_complex "${FILTER_COMPLEX}" -loop 0 "$OUTPUT_FILE"

echo "✅ Done! All GIFs were generated"
