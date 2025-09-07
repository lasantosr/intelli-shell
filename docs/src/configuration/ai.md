<!-- markdownlint-disable MD036 5th level titles are too small on mdbook -->

# AI Integration

IntelliShell integrates with various AI providers to provide powerful features like generating commands from natural language,
automatically fixing errors in failed commands, and importing new commands from text sources. This chapter guides you
through configuring these features.

## Enabling AI Features

AI integration is disabled by default. To turn on all AI-powered functionality, you must first enable it:

```toml
{{#include ../../../default_config.toml:225:227}}
```

> 📝 **Note**: By default, IntelliShell is configured to use **Google Gemini**, given that it has a generous free tier.
> For the AI features to work out-of-the-box, you must set the `GEMINI_API_KEY` environment variable with a valid key
> from [Google AI Studio](https://aistudio.google.com/app/apikey).
>
> Alternatively, you can configure a different provider (like OpenAI or a local Ollama model) by following the
> examples in the sections below.

## Configuration Overview

The `[ai]` section in your configuration is organized into three main parts, which work together to connect IntelliShell
to your preferred AI provider:

1. **Task Assignment:** This is where you assign a specific AI model to a task , such as suggesting commands or fixing errors.
2. **Model Catalog:** This is a library of all the AI models you want to make available. Each model is given a unique
   alias (e.g., "gemini", "gpt4-fast") that you can then use in the Task Assignment section.
3. **Custom Prompts:** For advanced control, this section allows you to customize the instructions that IntelliShell
   sends to the AI provider for each task.

Let's look at each part in detail.

### 1. Task Assignment

In the `[ai.models]` section, you tell IntelliShell which model from your catalog to use for each specific AI-powered task.

```toml
{{#include ../../../default_config.toml:230:241}}
```

### 2. Model Catalog

The `[ai.catalog]` section is where you define the connection details for every model alias you wish to use. This allows
you to configure multiple models from different providers and easily switch between them in the Task Assignment section.

```toml
{{#include ../../../default_config.toml:252:258}}
```

#### Supported Providers

IntelliShell has native support for several major providers. It will automatically look for API keys in the standard
environment variables associated with each one:

| Provider Name | Default API Key Environment Variable                |
| ------------- | --------------------------------------------------- |
| `openai`      | `OPENAI_API_KEY`                                    |
| `gemini`      | `GEMINI_API_KEY`                                    |
| `anthropic`   | `ANTHROPIC_API_KEY`                                 |
| `ollama`      | `OLLAMA_API_KEY` (not required for local instances) |

#### Configuration Examples

Here are some examples of how to configure different models in your catalog.
Each model you define must be under `ai.catalog.<your-alias-name>`.

> ⚠️ **IMPORTANT**
>
> When you add your first model to `[ai.catalog]`, it replaces the _entire_ default catalog. Therefore, you must ensure
> the model aliases you create match the ones assigned in the `[ai.models]` section above.

- **OpenAI**

  To use a model from [OpenAI](https://platform.openai.com/), like GPT-4o:

  ```toml
  [ai.catalog.gpt-4o]
  provider = "openai"
  model = "gpt-4o"
  ```

- **Anthropic Claude**

  To use a model from [Anthropic](https://console.anthropic.com/), like Claude 4.0 Sonnet:

  ```toml
  [ai.catalog.claude-sonnet]
  provider = "anthropic"
  model = "claude-sonnet-4-0"
  ```

- **Local Models with Ollama**

  You can run models locally using [Ollama](https://ollama.com/). This is a great option for privacy and offline use.

  ```toml
  [ai.catalog.local-llama]
  provider = "ollama"
  model = "llama3"
  # If Ollama is running on a different host, specify the URL:
  # url = "http://192.168.1.100:11434"
  ```

- **Using OpenAI-Compatible Endpoints**

  Many other AI providers (like Groq, xAI, DeepSeek, etc.) offer APIs that are compatible with OpenAI's API structure.
  You can connect to these providers by setting the `provider` to `openai`.
  
  - **Groq**

    [Groq](https://console.groq.com) is known for its high-speed inference. To use a Llama 3 model via Groq:

    ```toml
    [ai.catalog.groq-llama]
    provider = "openai"
    model = "llama-3.1-8b-instant"
    url = "https://api.groq.com/openai/v1"
    api_key_env = "GROQ_API_KEY"
    ```
  
  - **xAI**

    To connect to a model from [xAI](https://console.x.ai):

    ```toml
    [ai.model.grok]
    provider = "openai"
    model = "grok-4"
    url = "https://api.x.ai/v1"
    api_key_env = "XAI_API_KEY"
    ```
  
  - **DeepSeek**

    To use a model from [DeepSeek](https://platform.deepseek.com):

    ```toml
    [ai.model.deepseek]
    provider = "openai"
    model = "deepseek-chat"
    url = "https://api.deepseek.com"
    api_key_env = "DEEPSEEK_API_KEY"
    ```

  - **Azure OpenAI Service**

    To connect to a model you've deployed on [Azure](https://azure.microsoft.com/es-es/products/ai-services/openai-service):

    ```toml
    [ai.catalog.azure-gpt4]
    provider = "openai"
    model = "my-gpt4-deployment"
    url = "https://your-resource-name.openai.azure.com/"
    api_key_env = "AZURE_OPENAI_KEY"
    ```

### 3. Customizing Prompts

For advanced users, IntelliShell allows you to completely customize the system prompts sent to the AI for the `suggest`,
`fix`, `import` and `completion` tasks. This gives you fine-grained control over the AI's behavior, tone, and output format.

The prompts are defined in the `[ai.prompts]` section.

#### Dynamic Placeholders

When crafting your prompts, you can use special placeholders that IntelliShell will replace with real-time contextual
information before sending the request to the AI:

- **`##OS_SHELL_INFO##`**: Replaced with details about the current operating system and shell
- **`##WORKING_DIR##`**: Replaced with the current working directory path and a tree-like view of its contents
- **`##SHELL_HISTORY##`**: Replaced with the last few commands from the shell's history, this is only available for the
  `fix` prompt

You can view the well-tuned default prompts in the [`default_config.toml`](https://github.com/lasantosr/intelli-shell/blob/main/default_config.toml)
file to use as a starting point for your own customizations.
