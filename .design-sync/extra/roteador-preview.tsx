// Wrapper de preview/design: um MemoryRouter vindo da MESMA instância de
// react-router-dom que o esbuild inlinou no _ds_bundle.js — assim NavLink e
// afins encontram o contexto do Router (uma segunda cópia importada no
// preview criaria um contexto diferente e o NavLink lançaria erro).
// Exportado via cfg.extraEntries; usado como cfg.provider global e disponível
// ao agente de design como DevCliPortal.RoteadorPreview.
import type { ReactNode } from 'react'
import { MemoryRouter } from 'react-router-dom'

export function RoteadorPreview({ children }: { children?: ReactNode }) {
  return <MemoryRouter>{children}</MemoryRouter>
}
