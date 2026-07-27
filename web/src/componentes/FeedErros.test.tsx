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
    render(<FeedErros erros={[base]} aoClicar={() => {}} />)

    const nivel = screen.getByText('ERROR')
    expect(nivel).toHaveStyle({ color: 'var(--vermelho)' })
    expect(screen.getByText('qa-prezzo-1')).toBeInTheDocument()
    expect(screen.getByText(/deu ruim na fatura/)).toBeInTheDocument()
  })

  it('clique dispara aoClicar com container e nível', () => {
    const aoClicar = vi.fn()
    render(<FeedErros erros={[base]} aoClicar={aoClicar} />)

    fireEvent.click(screen.getByText(/deu ruim na fatura/))

    expect(aoClicar).toHaveBeenCalledWith('qa-prezzo-1', 'ERROR')
  })

  it('item marcado novo tem a classe de destaque', () => {
    render(<FeedErros erros={[{ ...base, novo: true }]} aoClicar={() => {}} />)

    const item = screen.getByText(/deu ruim na fatura/).closest('li')
    expect(item).toHaveClass('novo')
  })

  it('lista vazia não renderiza nada', () => {
    const { container } = render(<FeedErros erros={[]} aoClicar={() => {}} />)
    expect(container).toBeEmptyDOMElement()
  })
})
