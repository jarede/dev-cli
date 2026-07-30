// Hook que gerencia o tema (claro/escuro) do portal.
// Inicializa sempre do `data-theme` já carimbado no <html> pelo script
// inline anti-flash (index.html), então nunca há discrepância entre o
// que o React vê e o CSS aplica.
//
// O atributo carimbado no <html> usa os valores `dark`/`light` — é o que
// o CSS espera (`:root[data-theme="dark"]` em index.css). O TIPO interno
// do hook continua em pt-br (`'claro' | 'escuro'`, mesmo vocabulário do
// resto do domínio); `paraAtributo`/`doAtributo` fazem a ponte entre os
// dois vocabulários num único lugar, então nunca há um `data-theme="escuro"`
// escrito por engano (foi exatamente esse bug que a spec original tinha:
// o hook carimbava 'claro'/'escuro' e o CSS só reconhecia 'dark'/'light',
// então o seletor nunca casava e o modo escuro não ativava).
//
// O `alternar()` troca o tema, persiste no localStorage e re-carimba o
// <html>. Enquanto não houver escolha salva, escuta o evento `change` do
// `matchMedia` para acompanhar o sistema; após a primeira alternância
// manual, para de escutar — e, diferente da escolha manual, seguir o
// sistema NUNCA grava no localStorage (ver `carimbarTema` vs
// `persistirTema` abaixo), senão uma mudança de tema do SO viraria
// silenciosamente uma "escolha salva" permanente.
//
// docs: https://developer.mozilla.org/docs/Web/API/Window/matchMedia
// docs: https://developer.mozilla.org/docs/Web/API/MediaQueryList/change_event

import { useCallback, useEffect, useState } from 'react'

type Tema = 'claro' | 'escuro'

/// Valores aceitos pelo atributo `data-theme` do <html> — é o vocabulário
/// que o CSS (index.css) e o script anti-flash (index.html) usam.
type AtributoTema = 'dark' | 'light'

const CHAVE_LOCALSTORAGE = 'dev-cli-tema'

function paraAtributo(tema: Tema): AtributoTema {
  return tema === 'escuro' ? 'dark' : 'light'
}

function doAtributo(atributo: string | undefined): Tema | null {
  if (atributo === 'dark') return 'escuro'
  if (atributo === 'light') return 'claro'
  return null
}

/// Lê o tema do <html> (fonte única, sincronizado com o anti-flash).
/// Se o dataset não foi carimbado (ex.: testes sem anti-flash), cai para
/// a preferência do sistema.
function temaDoHtml(): Tema {
  const tema = doAtributo(document.documentElement.dataset.theme)
  if (tema !== null) return tema
  // dataset não foi carimbado (ex.: testes sem anti-flash); cai para o sistema.
  return window.matchMedia('(prefers-color-scheme: dark)').matches ? 'escuro' : 'claro'
}

/// Carimba o <html> (CSS reage via `:root[data-theme]`) SEM persistir.
/// Usado tanto por `alternar()` (que persiste em seguida) quanto pelo
/// listener do sistema (que nunca deve persistir — ver achado 3 do
/// review: "seguir o sistema" não é uma escolha do usuário).
function carimbarTema(tema: Tema) {
  document.documentElement.dataset.theme = paraAtributo(tema)
}

/// Grava a escolha do usuário no localStorage. Só quem representa uma
/// escolha EXPLÍCITA (o clique em `alternar()`) deve chamar isto.
function persistirTema(tema: Tema) {
  try {
    localStorage.setItem(CHAVE_LOCALSTORAGE, tema)
  } catch {
    // localStorage pode falhar (quota excedida, privacidade restrita);
    // ignorar — o tema ainda funciona na sessão atual.
  }
}

function existeEscolhaSalva(): boolean {
  return localStorage.getItem(CHAVE_LOCALSTORAGE) !== null
}

export interface UseTemaRetorno {
  tema: Tema
  alternar: () => void
}

export function useTema(): UseTemaRetorno {
  const [tema, setTema] = useState<Tema>(temaDoHtml)
  // Reflete se já existe uma escolha manual salva. Entra nas deps do efeito
  // abaixo: assim que `alternar()` seta isto para `true`, o efeito roda de
  // novo e desliga o listener do sistema — sem isso, o `useEffect` com deps
  // `[]` só checava o localStorage na montagem, e um `change` do SO depois
  // da escolha manual ainda sobrescrevia o tema escolhido pelo usuário.
  const [temEscolha, setTemEscolha] = useState<boolean>(existeEscolhaSalva)

  const alternar = useCallback(() => {
    setTema((atual) => {
      const proximo: Tema = atual === 'claro' ? 'escuro' : 'claro'
      carimbarTema(proximo)
      persistirTema(proximo)
      return proximo
    })
    setTemEscolha(true)
  }, [])

  useEffect(() => {
    // Já existe escolha manual salva: não escuta o sistema.
    if (temEscolha) return

    const mq = window.matchMedia('(prefers-color-scheme: dark)')
    const handler = (e: MediaQueryListEvent) => {
      const novo: Tema = e.matches ? 'escuro' : 'claro'
      // Só carimba — NÃO persiste. "Seguir o sistema" não é uma escolha
      // do usuário, então não deve virar uma entrada permanente no
      // localStorage (isso é o que `alternar()` faz).
      carimbarTema(novo)
      setTema(novo)
    }
    mq.addEventListener('change', handler)
    return () => mq.removeEventListener('change', handler)
  }, [temEscolha])

  return { tema, alternar }
}
