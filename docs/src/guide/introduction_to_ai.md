# Introduction to AI

IntelliShell integrates with various AI providers to act as your command-line co-pilot, helping you generate, fix, and
even discover commands when you're stuck. This chapter provides a high-level overview of what these features can do and
how to get them up and running.

## Why Use AI?

AI integration is completely optional, but enabling it unlocks a new level of productivity. Instead of just recalling
commands you've already saved, you can create new ones on the fly from natural language descriptions. It's like having
an expert assistant who knows the syntax for thousands of tools, ready to help you at a moment's notice.

## Features Unlocked by AI

Enabling AI integration powers up several key workflows:

- **Generate Commands from Search**: Can't find the command you're looking for? In the search UI (<kbd>Ctrl</kbd>+<kbd>Space</kbd>),
  type a description of what you want to do (e.g., _"find all files larger than 10MB"_) and press <kbd>Ctrl</kbd>+<kbd>I</kbd>
  to let the AI write the command for you.

- **Fix Failing Commands**: When a command fails, recall it from your history and press <kbd>Ctrl</kbd>+<kbd>X</kbd>. The
  AI will analyze the command and the error message to suggest a working version or explain next steps.

- **Create New Bookmarks from a Prompt**: When bookmarking (<kbd>Ctrl</kbd>+<kbd>B</kbd>), you can provide a
  description instead of a command and press <kbd>Ctrl</kbd>+<kbd>I</kbd>. The AI will generate the command template
  for you, which you can then edit and save.

- **Generate Dynamic Completions**: When creating a new completion provide the root command and variable, optionally
  describing what you need (e.g., _"list all running docker containers"_) on the provider, then press <kbd>Ctrl</kbd>+<kbd>I</kbd>.
  The AI will generate the shell command to produce the suggestions.

- **Import from Anywhere**: The `import` command gains the ability to parse unstructured text. You can point it at
  a blog post, a cheat sheet, or even your own shell history, and it will extract and convert commands into reusable
  templates.

## How to Enable AI

AI features are opt-in and disabled by default. To enable them, you need to:

1. **Open Your Configuration File**

   Run the following command to open your `config.toml` file in your default editor:

   ```sh
   intelli-shell config
   ```

2. **Enable AI in the Configuration**

   Add or modify the `[ai]` section in the file to set `enabled` to `true`:

   ```toml
   [ai]
   enabled = true
   ```

   Save and close the file.

3. **Provide an API Key**

   By default, IntelliShell is configured to use Google Gemini, which has a generous free tier. You can obtain an API
   key from [Google AI Studio](https://aistudio.google.com/app/apikey) and set it as an environment variable:

   ```sh
   export GEMINI_API_KEY="your-api-key-here"
   ```

   You can add this line to your shell's profile file (e.g., `~/.bashrc`, `~/.zshrc`) to make it permanent.

> 💡 **Note**: IntelliShell supports a wide range of AI providers, including OpenAI, Anthropic, and local models via
> Ollama. For detailed instructions on how to configure different models and customize prompts, see the
> [**AI Integration**](../configuration/ai.md) chapter in the reference section.

---

With AI enabled, you now have even more ways to build your command library. Let's explore how to use them in the next
chapter: [**Populating Your Library**](./populating_your_library.md).
