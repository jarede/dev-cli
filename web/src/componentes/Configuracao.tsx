// Tela Configuração: espelha a config EFETIVA do dev-server (GET
// /api/config — o mesmo padrão de fetch sob demanda de `Historico.tsx`),
// não mais valores chumbados no React. Puramente somente-leitura: o
// servidor hoje só LÊ o TOML (não existe endpoint de escrita), então todo
// campo é `readOnly`/`disabled` — nenhum input aqui finge ser editável sem
// persistir de verdade.

import { useEffect, useState } from 'react'
import { buscarConfig } from '../api'
import type { ConfigEfetiva } from '../tipos'

export function Configuracao() {
  const [config, setConfig] = useState<ConfigEfetiva | null>(null)
  const [erro, setErro] = useState<string | null>(null)

  useEffect(() => {
    let ativo = true
    buscarConfig()
      .then((c) => {
        if (ativo) {
          setConfig(c)
          setErro(null)
        }
      })
      .catch((e: unknown) => {
        if (ativo) setErro(e instanceof Error ? e.message : String(e))
      })
    return () => {
      ativo = false
    }
  }, [])

  return (
    <main className="shell shell-narrow" data-screen-label="Configuração">
      <header className="tela-header" style={{ flexDirection: 'column', alignItems: 'flex-start' }}>
        <h1>Configuração</h1>
        <p className="subtitulo" style={{ marginTop: 6 }}>
          Espelha o <span className="caminho-mono">/etc/dev-cli/config.toml</span> — também ajustável por
          variáveis <span className="caminho-mono">DEV_CLI_*</span>. Somente leitura: o dev-server hoje só
          lê o TOML, não escreve de volta.
        </p>
      </header>

      {erro !== null && <div className="banner-api-fora">⚠ {erro}</div>}

      {config === null && erro === null && <p className="vazio">carregando…</p>}

      {config !== null && (
        <form className="config-form" onSubmit={(e) => e.preventDefault()}>
          <div className="field">
            <label>Origem da coleta</label>
            <div className="seg" role="radiogroup" aria-label="Origem da coleta (somente leitura)">
              {(['docker', 'ssh'] as const).map((op) => {
                // `ssh` vazio na config = docker local (mesma regra do
                // `dev-server`: ver `Executor::Local` vs `Executor::Ssh`).
                const origem = config.coleta.ssh === '' ? 'docker' : 'ssh'
                return (
                  <label key={op} className={`seg-opt ${origem === op ? 'selected' : ''}`}>
                    <input
                      className="sr-only"
                      type="radio"
                      name="origem"
                      value={op}
                      checked={origem === op}
                      readOnly
                      disabled
                    />
                    {op === 'docker' ? 'docker local' : 'SSH remoto'}
                  </label>
                )
              })}
            </div>
          </div>

          <div className="field">
            <label>Host SSH</label>
            <input
              className="input"
              type="text"
              value={config.coleta.ssh}
              placeholder="(docker local)"
              readOnly
              disabled={config.coleta.ssh === ''}
              aria-label="Host SSH"
            />
          </div>

          <div className="config-grid-2">
            <div className="field">
              <label>Intervalo de coleta (s)</label>
              <input className="input" type="number" value={config.coleta.intervalo_seg} readOnly />
            </div>
            <div className="field">
              <label>Janela de análise (min)</label>
              <input className="input" type="number" value={config.coleta.janela_min} readOnly />
            </div>
          </div>

          <div className="config-grid-2">
            <div className="field">
              <label>Retenção do banco (horas)</label>
              <input className="input" type="number" value={config.coleta.retencao_horas} readOnly />
            </div>
            <div className="field">
              <label>Bind da API</label>
              <input className="input" type="text" value={config.servidor.bind} readOnly />
            </div>
          </div>

          <div className="field">
            <label>Diretório do portal</label>
            <input
              className="input"
              type="text"
              value={config.servidor.portal_dir || '(nenhum — só API)'}
              readOnly
            />
          </div>

          <div className="config-acoes">
            <button
              type="submit"
              className="btn btn-primary"
              disabled
              title="Escrita ainda não implementada — servidor só lê o TOML"
            >
              Salvar alterações
            </button>
          </div>
        </form>
      )}
    </main>
  )
}
