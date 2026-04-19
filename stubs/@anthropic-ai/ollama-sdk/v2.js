/**
 * Ollama SDK Compatibility Layer v2
 * Provides an Anthropic-compatible interface for Ollama API
 */

export class AnthropicOllama {
  constructor(options = {}) {
    this.baseURL = options.baseURL || 'http://localhost:11434'
    this.apiKey = options.apiKey
    this.defaultHeaders = options.defaultHeaders || {}
    this.maxRetries = options.maxRetries || 3
    this.timeout = options.timeout || 600000
    this.logger = options.logger
  }

  async messages.create(params) {
    const { model, messages, max_tokens, temperature, stream, tools, system } = params

    console.error('[OLLAMA v2] Using baseURL:', this.baseURL)
    const ollamaModel = this._mapModel(model)
    console.error('[OLLAMA v2] Using model:', ollamaModel)

    const ollamaMessages = messages.map(msg => ({
      role: msg.role,
      content: msg.content
    }))

    if (system) {
      ollamaMessages.unshift({ role: 'system', content: system })
    }

    const requestBody = {
      model: ollamaModel,
      messages: ollamaMessages,
      stream: false,
      options: {
        temperature: temperature || 0.7,
        num_predict: max_tokens || 4096,
      }
    }

    try {
      const response = await fetch(`${this.baseURL}/api/chat`, {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          ...(this.apiKey ? { 'Authorization': `Bearer ${this.apiKey}` } : {}),
          ...this.defaultHeaders
        },
        body: JSON.stringify(requestBody)
      })

      if (!response.ok) {
        const error = await response.text()
        throw new Error(`Ollama API error: ${response.status} - ${error}`)
      }

      const data = await response.json()
      return this._transformResponse(data, tools)
    } catch (error) {
      this.logger?.error?.('Ollama request failed:', error.message)
      throw error
    }
  }

  _mapModel(model) {
    if (typeof process !== 'undefined' && process.env?.OLLAMA_MODEL) {
      return process.env.OLLAMA_MODEL
    }
    return 'llama3.2:1b'
  }

  _transformResponse(data, tools) {
    const message = data.message || {}
    const content = []

    if (message.content) {
      content.push({
        type: 'text',
        text: message.content
      })
    }

    return {
      id: `msg_${Date.now()}`,
      type: 'message',
      role: 'assistant',
      content,
      model: data.model || 'unknown',
      stop_reason: data.done ? 'end_turn' : null,
      usage: {
        input_tokens: data.prompt_eval_count || 0,
        output_tokens: data.eval_count || 0
      }
    }
  }
}

export default { AnthropicOllama }
