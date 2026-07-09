import { afterEach, describe, expect, it, vi } from 'vitest'
import { buscarAlertas, buscarContainers, buscarLinhas } from './api'

function respostaJson(corpo: unknown): Response {
  return new Response(JSON.stringify(corpo), {
    status: 200,
    headers: { 'Content-Type': 'application/json' },
  })
}

describe('cliente da API', () => {
  afterEach(() => vi.unstubAllGlobals())

  it('buscarContainers chama /api/containers e devolve o JSON', async () => {
    const fetchFalso = vi.fn().mockResolvedValue(respostaJson([{ nome: 'app' }]))
    vi.stubGlobal('fetch', fetchFalso)

    const lista = await buscarContainers()

    expect(fetchFalso).toHaveBeenCalledWith('/api/containers')
    expect(lista[0].nome).toBe('app')
  })

  it('buscarContainers propaga janela_min na query', async () => {
    const fetchFalso = vi.fn().mockResolvedValue(respostaJson([]))
    vi.stubGlobal('fetch', fetchFalso)

    await buscarContainers(60)

    expect(fetchFalso).toHaveBeenCalledWith('/api/containers?janela_min=60')
  })

  it('erro HTTP vira exceção com o status', async () => {
    vi.stubGlobal('fetch', vi.fn().mockResolvedValue(new Response('x', { status: 500 })))

    await expect(buscarContainers()).rejects.toThrow('500')
  })

  it('buscarLinhas monta a URL com nome escapado, nível e limite', async () => {
    const fetchFalso = vi.fn().mockResolvedValue(respostaJson([]))
    vi.stubGlobal('fetch', fetchFalso)

    await buscarLinhas('qa/prezzo', 'ERROR', 50)

    expect(fetchFalso).toHaveBeenCalledWith(
      '/api/containers/qa%2Fprezzo/linhas?limite=50&nivel=ERROR',
    )
  })

  it('buscarAlertas usa o limite default 100', async () => {
    const fetchFalso = vi.fn().mockResolvedValue(respostaJson([]))
    vi.stubGlobal('fetch', fetchFalso)

    await buscarAlertas()

    expect(fetchFalso).toHaveBeenCalledWith('/api/alertas?limite=100')
  })
})
