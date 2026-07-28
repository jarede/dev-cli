// Entry point: BrowserRouter envolve o App para o react-router-dom
// gerenciar as 4 rotas do portal (Visão geral / Histórico / IA / Config).
// Sem BrowserRouter aqui, o use <NavLink> e <Routes> em App.tsx não
// teriam acesso ao contexto do router.
import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { BrowserRouter } from 'react-router-dom'
import './index.css'
import App from './App.tsx'

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <BrowserRouter>
      <App />
    </BrowserRouter>
  </StrictMode>,
)
