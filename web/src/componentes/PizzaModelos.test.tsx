import { describe, expect, it } from 'vitest'
import { render, screen } from '@testing-library/react'
import { PizzaModelos, fatiasPizza } from './PizzaModelos'
import type { ModeloCusto } from '../tipos'

const modelo = (nome: string, tokens: number, cache = 0, semCache = 0): ModeloCusto =>
  ({ modelo: nome, provedor: 'x', sessoes: 1, tokens, custo_usd: 0, tokens_cache: cache, tokens_sem_cache: semCache })

describe('fatiasPizza', () => {
  it('ordena por tokens desc e calcula percentuais', () => {
    const fatias = fatiasPizza([modelo('haiku', 250), modelo('sonnet', 750)])
    expect(fatias.map((f) => f.modelo)).toEqual(['sonnet', 'haiku'])
    expect(fatias.map((f) => f.pct)).toEqual([75, 25])
  })
  it('modelo com 0 tokens não gera fatia', () => {
    expect(fatiasPizza([modelo('a', 100), modelo('b', 0)])).toHaveLength(1)
  })
  it('sem tokens, sem fatias', () => {
    expect(fatiasPizza([])).toEqual([])
  })
  it('sonnet/opus/haiku têm cores fixas do DS', () => {
    const fatias = fatiasPizza([modelo('claude-sonnet-5', 1), modelo('claude-opus-5', 1)])
    expect(fatias.find((f) => f.modelo.includes('sonnet'))?.cor).toBe('var(--color-accent)')
    expect(fatias.find((f) => f.modelo.includes('opus'))?.cor).toBe('var(--sev-vermelho)')
  })
})

describe('PizzaModelos', () => {
  it('renderiza fatias com legenda', () => {
    render(<PizzaModelos modelos={[modelo('claude-sonnet-5', 1000), modelo('claude-haiku-4', 500)]} />)
    expect(screen.getByText('claude-sonnet-5')).toBeInTheDocument()
    expect(screen.getByText('claude-haiku-4')).toBeInTheDocument()
    // 1000/1500 = 66,7%, 500/1500 = 33,3%
    expect(screen.getByText('66,7%')).toBeInTheDocument()
    expect(screen.getByText('33,3%')).toBeInTheDocument()
  })
  it('retorna null sem modelos', () => {
    const { container } = render(<PizzaModelos modelos={[]} />)
    expect(container.innerHTML).toBe('')
  })
})