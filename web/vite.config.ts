// Configuração do Vite + Vitest num arquivo só.
// `defineConfig` de 'vitest/config' aceita o bloco `test` além das opções
// normais do Vite — evita um vitest.config.ts separado.
// docs: https://vitejs.dev/config/
// docs: https://vitest.dev/config/
import { defineConfig } from 'vitest/config'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
  server: {
    // Proxy de dev: o navegador chama /api na MESMA origem do portal e o
    // Vite repassa para o dev-server — zero CORS em desenvolvimento.
    // docs: https://vitejs.dev/config/server-options#server-proxy
    proxy: {
      '/api': 'http://127.0.0.1:8787',
    },
  },
  test: {
    // jsdom simula o DOM do navegador para a Testing Library.
    // docs: https://vitest.dev/config/#environment
    environment: 'jsdom',
    setupFiles: './src/setupTests.ts',
  },
})
