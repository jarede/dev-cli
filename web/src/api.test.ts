import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  buscarAlertas,
  buscarCambio,
  buscarConfig,
  buscarContainers,
  buscarCustosIa,
  buscarErros,
  buscarHistorico,
  buscarLinhas,
} from './api'

/// Uma Response de sucesso com corpo JSON (o objeto Response do fetch
/// existe no jsdom/node moderno — não precisa de mock de biblioteca).
function respostaJson(corpo: unknown): Response {
  return new Response(JSON.stringify(corpo), {
    status: 200,
    headers: { 'Content-Type': 'application/json' },
  })
}

describe('cliente da API', () => {
  // `vi.stubGlobal` troca o fetch global só dentro do teste;
  // `unstubAllGlobals` desfaz para não vazar entre testes.
  // docs: https://vitest.dev/api/vi.html#vi-stubglobal
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

  it('buscarErros monta a URL com desde_id e limite', async () => {
    const fetchFalso = vi.fn().mockResolvedValue(respostaJson([]))
    vi.stubGlobal('fetch', fetchFalso)

    await buscarErros(42)

    expect(fetchFalso).toHaveBeenCalledWith('/api/erros?desde_id=42&limite=100')
  })

  it('buscarErros propaga limite customizado', async () => {
    const fetchFalso = vi.fn().mockResolvedValue(respostaJson([]))
    vi.stubGlobal('fetch', fetchFalso)

    await buscarErros(0, 50)

    expect(fetchFalso).toHaveBeenCalledWith('/api/erros?desde_id=0&limite=50')
  })

  it('buscarHistorico usa horas=24 como default', async () => {
    const fetchFalso = vi.fn().mockResolvedValue(respostaJson([]))
    vi.stubGlobal('fetch', fetchFalso)

    await buscarHistorico()

    expect(fetchFalso).toHaveBeenCalledWith('/api/containers/historico?horas=24')
  })

  it('buscarHistorico propaga horas customizado', async () => {
    const fetchFalso = vi.fn().mockResolvedValue(respostaJson([]))
    vi.stubGlobal('fetch', fetchFalso)

    await buscarHistorico(6)

    expect(fetchFalso).toHaveBeenCalledWith('/api/containers/historico?horas=6')
  })

  it('buscarCustosIa sem mes chama o endpoint sem query', async () => {
    const fetchFalso = vi.fn().mockResolvedValue(respostaJson({}))
    vi.stubGlobal('fetch', fetchFalso)

    await buscarCustosIa()

    expect(fetchFalso).toHaveBeenCalledWith('/api/ia/custos')
  })

  it('buscarCustosIa propaga mes escapando caracteres', async () => {
    const fetchFalso = vi.fn().mockResolvedValue(respostaJson({}))
    vi.stubGlobal('fetch', fetchFalso)

    await buscarCustosIa('2026-07')

    expect(fetchFalso).toHaveBeenCalledWith('/api/ia/custos?mes=2026-07')
  })

  it('buscarCambio chama o endpoint fixo', async () => {
    const fetchFalso = vi.fn().mockResolvedValue(respostaJson({ usd_brl: 5.42 }))
    vi.stubGlobal('fetch', fetchFalso)

    const cambio = await buscarCambio()
    expect(fetchFalso).toHaveBeenCalledWith('/api/ia/cambio')
    expect(cambio.usd_brl).toBe(5.42)
  })

  it('buscarConfig chama /api/config e devolve o JSON', async () => {
    const configFalsa = {
      coleta: {
        intervalo_seg: 60,
        janela_min: 15,
        retencao_horas: 24,
        tail_inicial: 1000,
        db: '',
        ssh: '',
      },
      limiares: { p95_lento_seg: 1.0, taxa_erro_pct: 5.0 },
      servidor: { bind: '127.0.0.1:8787', portal_dir: '' },
    }
    const fetchFalso = vi.fn().mockResolvedValue(respostaJson(configFalsa))
    vi.stubGlobal('fetch', fetchFalso)

    const config = await buscarConfig()

    expect(fetchFalso).toHaveBeenCalledWith('/api/config')
    expect(config.servidor.bind).toBe('127.0.0.1:8787')
  })
})
