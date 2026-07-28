import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { FeedErros, type ErroLogComDestaque } from './FeedErros'

const base: ErroLogComDestaque = {
  id: 1,
  container: 'qa-prezzo-1',
  nivel: 'ERROR',
  linha: 'deu ruim na fatura',
  collected_at: Math.floor(Date.now() / 1000) - 5,
  novo: false,
}

describe('FeedErros', () => {
  afterEach(() => cleanup())

  it('renderiza nível com a cor de erro (vermelho)', () => {
    render(<FeedErros erros={[base]} aoClicar={() => {}} aoVivo={true} />)

    const nivel = screen.getByText('ERROR')
    expect(nivel).toHaveStyle({ color: 'var(--sev-vermelho)' })
    expect(screen.getByText('qa-prezzo-1')).toBeInTheDocument()
    expect(screen.getByText(/deu ruim na fatura/)).toBeInTheDocument()
  })

  it('clique dispara aoClicar com container e nível', () => {
    const aoClicar = vi.fn()
    render(<FeedErros erros={[base]} aoClicar={aoClicar} aoVivo={true} />)

    fireEvent.click(screen.getByText(/deu ruim na fatura/))

    expect(aoClicar).toHaveBeenCalledWith('qa-prezzo-1', 'ERROR')
  })

  it('item marcado novo tem a classe de destaque', () => {
    render(<FeedErros erros={[{ ...base, novo: true }]} aoClicar={() => {}} aoVivo={true} />)

    // A classe de destaque agora vive no `feed-item`, não num `<li>`.
    // Pegamos o container que tem o texto da linha e subimos um nível.
    const item = screen.getByText(/deu ruim na fatura/).closest('.feed-item')
    expect(item).toHaveClass('novo')
  })

  it('bolinha de "ao vivo" reflete a prop', () => {
    const { container: c1 } = render(<FeedErros erros={[]} aoClicar={() => {}} aoVivo={true} />)
    expect(c1.querySelector('.bolinha-aovivo')).not.toHaveClass('parado')

    const { container: c2 } = render(<FeedErros erros={[]} aoClicar={() => {}} aoVivo={false} />)
    expect(c2.querySelector('.bolinha-aovivo')).toHaveClass('parado')
  })

  it('lista vazia mostra o placeholder em vez de sumir', () => {
    // Mudou: o feed agora sempre renderiza a seção (com o título "Erros
    // ao vivo"); só some se não houver nada para mostrar depois — manter
    // a estrutura ajuda a UI a ter um layout estável.
    const { container } = render(<FeedErros erros={[]} aoClicar={() => {}} aoVivo={false} />)
    expect(screen.getByText(/Erros ao vivo/)).toBeInTheDocument()
    expect(container).not.toBeEmptyDOMElement()
  })
})
