// Direct test of Ollama SDK
import { AnthropicOllama } from './stubs/@anthropic-ai/ollama-sdk/index.js';

const client = new AnthropicOllama({
  baseURL: 'http://localhost:11434'
});

console.log('Testing Ollama SDK...');
const start = Date.now();

try {
  const response = await client.messages.create({
    model: 'claude-sonnet-4-5',
    messages: [{ role: 'user', content: 'Hi' }],
    max_tokens: 30
  });
  console.log('Response time:', Date.now() - start, 'ms');
  console.log('Response:', response.content[0].text);
} catch (e) {
  console.error('Error:', e.message);
}
