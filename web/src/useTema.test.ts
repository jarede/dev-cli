// Testes do hook useTema: persistência, fallback ao sistema (mock de
// matchMedia) e alternância. Usa `vi.mocked` para tipar os mocks, e
// `vi.useFakeTimers` não é necessário — o hook só escuta eventos.
//
// docs: https://vitest.dev/api/vi.html#vi-mock
// docs: https://vitest.dev/api/vi.html#vi-stubglobal

import { act, renderHook } from '@testing-library/react'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { useTema } from './useTema'

/// Monta um mock do `matchMedia` que reporta `matches` fixo e devolve
/// uma `MediaQueryList` com `addEventListener`/`removeEventListener`
/// rastreáveis (para o hook poder escutar o evento `change`).
function criarMockMatchMedia(prefereEscuro: boolean) {
  const ouvintes = new Set<EventListener>()
  return vi.fn().mockImplementation((query: string) => ({
    matches: query === '(prefers-color-scheme: dark)' ? prefereEscuro : false,
    media: query,
    addEventListener: (_tipo: string, handler: EventListener) => {
      ouvintes.add(handler)
    },
    removeEventListener: (_tipo: string, handler: EventListener) => {
      ouvintes.delete(handler)
    },
    // Expõe os ouvintes para o teste simular a mudança de preferência.
    _ouvintes: ouvintes,
  }))
}

beforeEach(() => {
  // Garante que cada teste começa sem localStorage residual e sem data-theme.
  localStorage.clear()
  delete document.documentElement.dataset.theme
})

afterEach(() => {
  vi.restoreAllMocks()
})

describe('useTema', () => {
  it('inicializa do dataset.theme quando escuro', () => {
    // O atributo carimbado no <html> é 'dark'/'light' (vocabulário do CSS),
    // não 'escuro'/'claro' (vocabulário interno do hook) — ver achado 1.
    document.documentElement.dataset.theme = 'dark'
    vi.stubGlobal('matchMedia', criarMockMatchMedia(false))

    const { result } = renderHook(() => useTema())
    expect(result.current.tema).toBe('escuro')
  })

  it('inicializa do dataset.theme quando claro', () => {
    document.documentElement.dataset.theme = 'light'
    vi.stubGlobal('matchMedia', criarMockMatchMedia(true))

    const { result } = renderHook(() => useTema())
    expect(result.current.tema).toBe('claro')
  })

  it('cai para preferência do sistema quando dataset não está definido', () => {
    // dataset já foi deletado no beforeEach, então está undefined.
    vi.stubGlobal('matchMedia', criarMockMatchMedia(true))

    const { result } = renderHook(() => useTema())
    expect(result.current.tema).toBe('escuro')
  })

  it('alternar() troca claro ↔ escuro e persiste no localStorage', () => {
    document.documentElement.dataset.theme = 'light'
    vi.stubGlobal('matchMedia', criarMockMatchMedia(false))

    const { result } = renderHook(() => useTema())
    expect(result.current.tema).toBe('claro')

    act(() => result.current.alternar())
    expect(result.current.tema).toBe('escuro')
    // localStorage guarda o vocabulário pt-br ('escuro'/'claro'); o
    // dataset do <html> guarda o vocabulário do CSS ('dark'/'light').
    expect(localStorage.getItem('dev-cli-tema')).toBe('escuro')
    expect(document.documentElement.dataset.theme).toBe('dark')

    act(() => result.current.alternar())
    expect(result.current.tema).toBe('claro')
    expect(localStorage.getItem('dev-cli-tema')).toBe('claro')
    expect(document.documentElement.dataset.theme).toBe('light')
  })

  it('após escolha salva, não segue mais mudanças do sistema', () => {
    document.documentElement.dataset.theme = 'light'
    const mm = criarMockMatchMedia(false)
    vi.stubGlobal('matchMedia', mm)

    const { result } = renderHook(() => useTema())
    expect(result.current.tema).toBe('claro')

    // Alterna manualmente → salva escolha e (via deps [temEscolha] do
    // efeito) desliga o listener do `change` do matchMedia.
    act(() => result.current.alternar())
    expect(result.current.tema).toBe('escuro')

    // Dispara o evento `change` do sistema (escuro → claro) mesmo assim —
    // é o mesmo listener que o hook registrou, exposto via `_ouvintes`.
    // Antes da correção do achado 2, o efeito só checava o localStorage na
    // montagem (deps `[]`), então o listener continuava vivo e este evento
    // sobrescrevia a escolha manual do usuário.
    const mq = mm('(prefers-color-scheme: dark)')
    const changeEvent = new Event('change')
    Object.defineProperty(changeEvent, 'matches', { value: false })
    act(() => {
      mq._ouvintes.forEach((h: EventListener) => h(changeEvent))
    })

    // O tema permanece a escolha manual, e o localStorage continua com ela.
    expect(result.current.tema).toBe('escuro')
    expect(localStorage.getItem('dev-cli-tema')).toBe('escuro')
    expect(document.documentElement.dataset.theme).toBe('dark')
  })

  it('sem escolha salva, segue preferência do sistema via change event mas não persiste', () => {
    document.documentElement.dataset.theme = 'light'
    const mm = criarMockMatchMedia(false)
    vi.stubGlobal('matchMedia', mm)

    const { result } = renderHook(() => useTema())
    expect(result.current.tema).toBe('claro')

    // Simula o sistema mudando para escuro.
    const mq = mm('(prefers-color-scheme: dark)')
    const changeEvent = new Event('change')
    Object.defineProperty(changeEvent, 'matches', { value: true })
    act(() => {
      mq._ouvintes.forEach((h: EventListener) => h(changeEvent))
    })

    expect(result.current.tema).toBe('escuro')
    // O <html> é carimbado com o valor que o CSS espera (`dark`/`light`,
    // não `escuro`/`claro`) — é exatamente o achado 1 do review: o CSS só
    // reconhece `:root[data-theme="dark"]`.
    expect(document.documentElement.dataset.theme).toBe('dark')
    // "Seguir o sistema" NUNCA grava no localStorage (achado 3) — só
    // `alternar()` persiste uma escolha explícita do usuário.
    expect(localStorage.getItem('dev-cli-tema')).toBeNull()
  })

  it('carimba o <html> com o vocabulário que o CSS espera (dark/light)', () => {
    document.documentElement.dataset.theme = 'light'
    vi.stubGlobal('matchMedia', criarMockMatchMedia(false))

    const { result } = renderHook(() => useTema())
    expect(document.documentElement.dataset.theme).toBe('light')

    act(() => result.current.alternar())
    expect(result.current.tema).toBe('escuro')
    // O CSS (index.css) só define `:root[data-theme="dark"]` — se o hook
    // carimbasse 'escuro' aqui (como na implementação original), o seletor
    // nunca casaria e o modo escuro nunca ativaria visualmente.
    expect(document.documentElement.dataset.theme).toBe('dark')
  })

  it('localStorage falha silenciosamente', () => {
    document.documentElement.dataset.theme = 'light'
    vi.stubGlobal('matchMedia', criarMockMatchMedia(false))
    vi.stubGlobal('localStorage', {
      getItem: () => null,
      setItem: () => { throw new Error('quota exceeded') },
      clear: () => {},
      removeItem: () => {},
      length: 0,
      key: () => null,
    })

    const { result } = renderHook(() => useTema())
    expect(result.current.tema).toBe('claro')

    act(() => result.current.alternar())
    expect(result.current.tema).toBe('escuro')
  })
})
