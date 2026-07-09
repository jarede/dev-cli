import { describe, expect, it } from 'vitest'
import { render, screen } from '@testing-library/react'
import App from './App'

describe('App', () => {
  it('renderiza o título do portal', () => {
    render(<App />)
    expect(screen.getByText('dev-cli · portal')).toBeInTheDocument()
  })
})
