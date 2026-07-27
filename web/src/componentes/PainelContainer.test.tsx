import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { PainelContainer } from './PainelContainer'

// `vi.mock` substitui o módulo ../api inteiro: o componente importa
// buscarLinhas e recebe esta versão falsa — teste sem rede.
// docs: https://vitest.dev/api/vi.html#vi-mock
vi.mock('../api', () => ({
  buscarLinhas: vi.fn(),
}))
import { buscarLinhas } from '../api'
const buscarLinhasFalso = vi.mocked(buscarLinhas)

describe('PainelContainer', () => {
  // `cleanup()` desmonta o que ficou renderizado do teste anterior antes do
  // próximo — sem isso, dois <PainelContainer> concorrentes na jsdom fariam
  // `getByText`/`getByRole` encontrar mais de um elemento e falhar.
  afterEach(() => {
    cleanup()
    buscarLinhasFalso.mockReset()
  })
  // Resposta default (lista vazia): só os testes que precisam de dados
  // específicos sobrescrevem com o próprio mockResolvedValue.
  beforeEach(() => {
    buscarLinhasFalso.mockResolvedValue([])
  })

  it('busca e mostra as linhas do container ao montar', async () => {
    buscarLinhasFalso.mockResolvedValue([
      { nivel: 'ERROR', linha: 'deu ruim na fatura', collected_at: 1 },
    ])
    render(<PainelContainer nome="qa-prezzo-1" aoFechar={() => {}} />)

    // findBy*: espera o resultado assíncrono do useEffect aparecer no DOM.
    // docs: https://testing-library.com/docs/dom-testing-library/api-async/
    expect(await screen.findByText(/deu ruim na fatura/)).toBeInTheDocument()
    expect(buscarLinhasFalso).toHaveBeenCalledWith('qa-prezzo-1', undefined)
  })

  it('nivelInicial filtra a busca na montagem', async () => {
    buscarLinhasFalso.mockResolvedValue([])
    render(<PainelContainer nome="app" nivelInicial="ERROR" aoFechar={() => {}} />)

    await screen.findByText(/nenhuma linha/)
    expect(buscarLinhasFalso).toHaveBeenCalledWith('app', 'ERROR')
    expect(screen.getByRole('combobox')).toHaveValue('ERROR')
  })

  it('nivelInicial com abreviação de supervisord (CRIT) aparece selecionada', async () => {
    // Regressão: CRIT/ERRO/FATAL são níveis reais (formato supervisord, ver
    // NIVEIS_DOCKER no nucleo) que chegam via clique no feed de erros. Antes
    // do fix, o <select> não tinha essas opções e mostrava "todos os
    // níveis" mesmo com a busca já filtrada — este teste trava isso.
    buscarLinhasFalso.mockResolvedValue([])
    render(<PainelContainer nome="app" nivelInicial="CRIT" aoFechar={() => {}} />)

    await screen.findByText(/nenhuma linha/)
    expect(buscarLinhasFalso).toHaveBeenCalledWith('app', 'CRIT')
    expect(screen.getByRole('combobox')).toHaveValue('CRIT')
  })

  it('trocar o nível refaz a busca com o filtro', async () => {
    buscarLinhasFalso.mockResolvedValue([])
    render(<PainelContainer nome="app" aoFechar={() => {}} />)
    await screen.findByText(/nenhuma linha/)

    fireEvent.change(screen.getByRole('combobox'), { target: { value: 'ERROR' } })

    expect(await screen.findByText(/nenhuma linha/)).toBeInTheDocument()
    expect(buscarLinhasFalso).toHaveBeenLastCalledWith('app', 'ERROR')
  })

  it('botão fechar chama aoFechar', async () => {
    buscarLinhasFalso.mockResolvedValue([])
    const aoFechar = vi.fn()
    render(<PainelContainer nome="app" aoFechar={aoFechar} />)
    await screen.findByText(/nenhuma linha/)

    fireEvent.click(screen.getByText('fechar'))
    expect(aoFechar).toHaveBeenCalled()
  })

  it('tecla Escape chama aoFechar', async () => {
    buscarLinhasFalso.mockResolvedValue([])
    const aoFechar = vi.fn()
    render(<PainelContainer nome="app" aoFechar={aoFechar} />)
    await screen.findByText(/nenhuma linha/)

    fireEvent.keyDown(window, { key: 'Escape' })
    expect(aoFechar).toHaveBeenCalled()
  })

  it('clique no backdrop chama aoFechar', async () => {
    buscarLinhasFalso.mockResolvedValue([])
    const aoFechar = vi.fn()
    const { container } = render(<PainelContainer nome="app" aoFechar={aoFechar} />)
    await screen.findByText(/nenhuma linha/)

    const backdrop = container.querySelector('.painel-backdrop')
    expect(backdrop).toBeInTheDocument()
    fireEvent.click(backdrop!)
    expect(aoFechar).toHaveBeenCalled()
  })

  it('clique dentro da gaveta não chama aoFechar', async () => {
    buscarLinhasFalso.mockResolvedValue([])
    const aoFechar = vi.fn()
    render(<PainelContainer nome="app" aoFechar={aoFechar} />)
    await screen.findByText(/nenhuma linha/)

    // Clicar no <select> (dentro da gaveta) não deve propagar pro backdrop.
    fireEvent.click(screen.getByRole('combobox'))
    expect(aoFechar).not.toHaveBeenCalled()
  })

  it('erro da API aparece no painel', async () => {
    // `mockImplementation` em vez de `mockRejectedValue`: evita a promise
    // rejeitada "pendurada" no mock antes do componente montar e anexar o
    // `.catch` — o que geraria um unhandled rejection no ambiente de teste.
    buscarLinhasFalso.mockImplementation(async () => {
      throw new Error('API respondeu 500')
    })
    render(<PainelContainer nome="app" aoFechar={() => {}} />)

    expect(await screen.findByText(/API respondeu 500/)).toBeInTheDocument()
  })
})
