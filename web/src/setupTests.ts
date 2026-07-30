// Carregado antes de cada arquivo de teste (ver vite.config.ts).
// Adiciona matchers de DOM ao expect: toBeInTheDocument(), toHaveTextContent()...
// docs: https://github.com/testing-library/jest-dom
import '@testing-library/jest-dom/vitest'

// Polyfill localStorage para o ambiente de teste — jsdom deve prover, mas
// algumas versões do Node (22+) emitem warning e deixam undefined quando o
// flag --localstorage-file não é passado. Este polyfill garante que os
// hooks que leem/gravam localStorage não quebrem nos testes.
// docs: https://developer.mozilla.org/docs/Web/API/Window/localStorage
if (typeof globalThis.localStorage === 'undefined') {
  const storage: Record<string, string> = {}
  Object.defineProperty(globalThis, 'localStorage', {
    value: {
      getItem: (key: string) => storage[key] ?? null,
      setItem: (key: string, value: string) => { storage[key] = value },
      removeItem: (key: string) => { delete storage[key] },
      clear: () => { for (const k in storage) delete storage[k] },
      get length() { return Object.keys(storage).length },
      key: (i: number) => Object.keys(storage)[i] ?? null,
    } satisfies Storage,
    writable: true,
    configurable: true,
  })
}

// jsdom não implementa matchMedia. O polyfill abaixo fornece um mock básico
// que reporta "light mode" por padrão — cada teste pode substituir via
// `vi.stubGlobal` se precisar de modo escuro ou de escutar eventos.
// docs: https://developer.mozilla.org/docs/Web/API/Window/matchMedia
if (typeof window.matchMedia !== 'function') {
  window.matchMedia = (query: string) => ({
    // Sempre "light mode" por padrão — testes que precisam de outro valor
    // substituem via `vi.stubGlobal('matchMedia', ...)`.
    matches: false,
    media: query,
    onchange: null,
    addEventListener: () => {},
    removeEventListener: () => {},
    dispatchEvent: () => true,
    addListener: () => {},
    removeListener: () => {},
  })
}
