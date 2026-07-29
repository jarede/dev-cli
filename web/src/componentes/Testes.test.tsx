// Testes do componente `Testes` (tela "Testes · e2e" do portal).
// Cobre: lista vazia, lista com suítes, expansão de uma suíte, criação
// de uma suíte via formulário (com validações), e o rodar de uma suíte
// com polling que termina em sucesso.
//
// O padrão é o mesmo dos outros componentes que batem em /api/*:
// `vi.mock('../api', ...)` no topo e `vi.mocked(buscarX)` para configurar
// as respostas — assim o teste não abre rede.

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react'
import { Testes } from './Testes'
import type { Suite } from '../tipos'

vi.mock('../api', () => ({
  buscarSuites: vi.fn(),
  criarSuite: vi.fn(),
  rodarSuite: vi.fn(),
  buscarExecucao: vi.fn(),
  cancelarExecucao: vi.fn(),
  buscarHistoricoExecucoes: vi.fn(),
}))
import {
  buscarExecucao,
  buscarHistoricoExecucoes,
  buscarSuites,
  cancelarExecucao,
  criarSuite,
  rodarSuite,
} from '../api'

const suitesFalso = vi.mocked(buscarSuites)
const criarFalso = vi.mocked(criarSuite)
const rodarFalso = vi.mocked(rodarSuite)
const execFalso = vi.mocked(buscarExecucao)
const cancelarFalso = vi.mocked(cancelarExecucao)
const historicoFalso = vi.mocked(buscarHistoricoExecucoes)

function suite(parcial: Partial<Suite> & { id: string }): Suite {
  return {
    id: parcial.id,
    nome: parcial.nome ?? `suíte ${parcial.id}`,
    timeout_etapa_seg: parcial.timeout_etapa_seg ?? 30,
    passos: parcial.passos ?? [
      { tipo: 'exec', cmd: 'echo oi' },
      { tipo: 'valida', cmd: 'echo oi', esperado: 'oi' },
    ],
  }
}

describe('Testes', () => {
  afterEach(() => {
    cleanup()
    suitesFalso.mockReset()
    criarFalso.mockReset()
    rodarFalso.mockReset()
    execFalso.mockReset()
    cancelarFalso.mockReset()
    historicoFalso.mockReset()
  })

  beforeEach(() => {
    // Defaults: lista vazia + nenhuma chamada a rodar.
    suitesFalso.mockResolvedValue([])
    criarFalso.mockResolvedValue(suite({ id: 'criada' }))
    rodarFalso.mockResolvedValue({ id_execucao: 'exec-1' })
    execFalso.mockResolvedValue({
      id_execucao: 'exec-1',
      suite_id: 's1',
      iniciada_em_unix: 1,
      estados: [],
      conclusao: 'rodando',
    })
    cancelarFalso.mockResolvedValue(undefined)
    historicoFalso.mockResolvedValue([])
  })

  it('carrega as suítes ao montar', async () => {
    suitesFalso.mockResolvedValue([
      suite({ id: 'sanidade', nome: 'sanidade · arquivo' }),
      suite({ id: 'prezzo', nome: 'prezzo · recalcular' }),
    ])
    render(<Testes />)

    expect(await screen.findByText('sanidade · arquivo')).toBeInTheDocument()
    expect(screen.getByText('prezzo · recalcular')).toBeInTheDocument()
  })

  it('lista vazia mostra o estado vazio com dica de onde ficam as suítes', async () => {
    render(<Testes />)
    expect(await screen.findByText(/nenhuma suíte encontrada/)).toBeInTheDocument()
    // Dica menciona o path do TOML (ajuda o operador a entender o que tá vazio).
    expect(screen.getByText(/\/etc\/dev-cli\/testes\//)).toBeInTheDocument()
  })

  it('falha da API mostra o banner de erro', async () => {
    suitesFalso.mockImplementation(async () => {
      throw new Error('API respondeu 500')
    })
    render(<Testes />)
    expect(await screen.findByText(/API respondeu 500/)).toBeInTheDocument()
  })

  it('clicar no cabeçalho expande os detalhes da suíte', async () => {
    suitesFalso.mockResolvedValue([
      suite({
        id: 's1',
        nome: 'suíte x',
        passos: [
          { tipo: 'exec', cmd: 'echo a' },
          { tipo: 'valida', cmd: 'echo a', esperado: 'a' },
        ],
      }),
    ])
    render(<Testes />)
    await screen.findByText('suíte x')

    // Antes do clique: nenhum stepper visível.
    expect(screen.queryByText('cli 1')).not.toBeInTheDocument()

    fireEvent.click(screen.getByText('suíte x'))
    expect(screen.getByText('cli 1')).toBeInTheDocument()
    expect(screen.getByText('cli 2')).toBeInTheDocument()
  })

  it('botão Rodar dispara o endpoint e mostra a UI de "rodando"', async () => {
    suitesFalso.mockResolvedValue([suite({ id: 's1', nome: 'suíte x' })])
    render(<Testes />)
    await screen.findByText('suíte x')

    // Antes: "Rodar". Clica: vira "Cancelar" + o stepper aparece.
    const botaoRodar = screen.getByText('Rodar')
    fireEvent.click(botaoRodar)

    await waitFor(() => {
      expect(rodarFalso).toHaveBeenCalledWith('s1')
    })
    // Após clicar, a suíte fica expandida automaticamente.
    expect(screen.getByText('cli 1')).toBeInTheDocument()
  })

  it('polling: o estado da execução em andamento atualiza a UI', async () => {
    // Suite + execucao que passa a "sucesso" depois de uma pollada.
    suitesFalso.mockResolvedValue([suite({ id: 's1', nome: 'suíte x' })])
    // 1ª pollada: ainda rodando, etapa 0 ok, etapa 1 rodando.
    // 2ª pollada: concluída com sucesso.
    execFalso
      .mockResolvedValueOnce({
        id_execucao: 'exec-1',
        suite_id: 's1',
        iniciada_em_unix: 1,
        estados: [
          { status: 'ok', saida: 'a' },
          { status: 'rodando' },
        ],
        conclusao: 'rodando',
      })
      .mockResolvedValueOnce({
        id_execucao: 'exec-1',
        suite_id: 's1',
        iniciada_em_unix: 1,
        estados: [
          { status: 'ok', saida: 'a' },
          { status: 'ok', saida: 'a' },
        ],
        conclusao: 'sucesso',
      })
    render(<Testes />)
    await screen.findByText('suíte x')

    fireEvent.click(screen.getByText('Rodar'))

    // 1ª pollada: etapa 1 está "rodando…". O polling roda a 1s, então
    // damos 2s de margem.
    await waitFor(
      () => {
        expect(execFalso).toHaveBeenCalled()
      },
      { timeout: 2000 },
    )
    // 2ª pollada: tudo "passou". Como o polling real é 1s, esperamos
    // mais um tick + a transição do estado rodando -> sucesso.
    await waitFor(
      () => {
        expect(screen.getAllByText('passou').length).toBe(2)
      },
      { timeout: 3000 },
    )
  })

  it('botão "Nova suíte" abre o formulário de cadastro', async () => {
    render(<Testes />)
    await screen.findByText(/nenhuma suíte encontrada/)

    // O texto "Nova suíte" aparece DUAS vezes (no botão e no estado vazio);
    // `getByRole` pega o <button> sem ambiguidade.
    fireEvent.click(screen.getByRole('button', { name: 'Nova suíte' }))
    expect(screen.getByText(/grava em/)).toBeInTheDocument()
    expect(screen.getByLabelText('Nome da suíte')).toBeInTheDocument()
    expect(screen.getByLabelText('Timeout por etapa (s)')).toBeInTheDocument()
    // 1 etapa inicial.
    expect(screen.getByText('1.')).toBeInTheDocument()
    expect(screen.getAllByDisplayValue('execução').length).toBeGreaterThan(0)
  })

  it('validação do form bloqueia salvar sem nome', async () => {
    render(<Testes />)
    await screen.findByText(/nenhuma suíte encontrada/)
    fireEvent.click(screen.getByRole('button', { name: 'Nova suíte' }))

    // Não preenche nome e clica Salvar.
    fireEvent.click(screen.getByRole('button', { name: 'Salvar suíte' }))
    expect(await screen.findByText('Dê um nome à suíte.')).toBeInTheDocument()
    expect(criarFalso).not.toHaveBeenCalled()
  })

  it('validação do form bloqueia etapa sem comando', async () => {
    render(<Testes />)
    await screen.findByText(/nenhuma suíte encontrada/)
    fireEvent.click(screen.getByRole('button', { name: 'Nova suíte' }))

    // Preenche o nome mas deixa a etapa com `cmd` vazio.
    fireEvent.change(screen.getByLabelText('Nome da suíte'), {
      target: { value: 'minha suíte' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Salvar suíte' }))
    expect(await screen.findByText(/Toda etapa precisa de um comando CLI\./)).toBeInTheDocument()
    expect(criarFalso).not.toHaveBeenCalled()
  })

  it('validação do form exige saída esperada em etapa de validação', async () => {
    render(<Testes />)
    await screen.findByText(/nenhuma suíte encontrada/)
    fireEvent.click(screen.getByRole('button', { name: 'Nova suíte' }))

    fireEvent.change(screen.getByLabelText('Nome da suíte'), {
      target: { value: 'val' },
    })
    // 1ª etapa é exec — muda o tipo para valida.
    const selects = screen.getAllByRole('combobox')
    // O primeiro combobox é o "Tipo" da etapa 1.
    fireEvent.change(selects[0], { target: { value: 'valida' } })
    // Preenche o comando da 1ª etapa (mas não o esperado) — usa
    // `getAllByLabelText` porque pode haver mais de um campo com
    // esse rótulo (um por etapa).
    const comandos = screen.getAllByLabelText('Comando CLI')
    fireEvent.change(comandos[0], { target: { value: 'echo oi' } })
    fireEvent.click(screen.getByRole('button', { name: 'Salvar suíte' }))
    expect(await screen.findByText(/Etapas de validação precisam da saída esperada\./)).toBeInTheDocument()
  })

  it('form válido chama criarSuite e fecha o form', async () => {
    render(<Testes />)
    await screen.findByText(/nenhuma suíte encontrada/)
    fireEvent.click(screen.getByRole('button', { name: 'Nova suíte' }))

    fireEvent.change(screen.getByLabelText('Nome da suíte'), {
      target: { value: 'minha suíte' },
    })
    const comandos = screen.getAllByLabelText('Comando CLI')
    fireEvent.change(comandos[0], { target: { value: 'echo oi' } })
    fireEvent.click(screen.getByRole('button', { name: 'Salvar suíte' }))

    await waitFor(() => {
      expect(criarFalso).toHaveBeenCalled()
    })
    // O form sumiu: o texto "grava em" não está mais no DOM.
    await waitFor(() => {
      expect(screen.queryByText(/grava em/)).not.toBeInTheDocument()
    })
  })

  it('"Adicionar etapa" insere uma nova linha', async () => {
    render(<Testes />)
    await screen.findByText(/nenhuma suíte encontrada/)
    fireEvent.click(screen.getByRole('button', { name: 'Nova suíte' }))

    // 1 etapa inicialmente.
    expect(screen.getAllByText(/^\d+\.$/).length).toBe(1)
    fireEvent.click(screen.getByRole('button', { name: '+ Adicionar etapa' }))
    expect(screen.getAllByText(/^\d+\.$/).length).toBe(2)
  })

  it('"Remover" desabilitado quando só tem uma etapa', async () => {
    render(<Testes />)
    await screen.findByText(/nenhuma suíte encontrada/)
    fireEvent.click(screen.getByRole('button', { name: 'Nova suíte' }))

    const botaoRemover = screen.getByRole('button', { name: 'Remover' })
    expect(botaoRemover).toBeDisabled()
  })

  it('suíte com falha de validação mostra a caixa de "esperado × obtido"', async () => {
    const s = suite({
      id: 'prezzo',
      nome: 'prezzo · recalcular',
      passos: [
        { tipo: 'exec', cmd: 'curl -sf ...' },
        {
          tipo: 'valida',
          cmd: 'curl ... | jq -r .status',
          esperado: 'concluido',
        },
      ],
    })
    suitesFalso.mockResolvedValue([s])
    // Execução já terminada: a validação falhou. O teste clica "Rodar"
    // para disparar o polling, que devolve esse estado e mostra a UI.
    execFalso.mockResolvedValue({
      id_execucao: 'exec-1',
      suite_id: 'prezzo',
      iniciada_em_unix: 1,
      estados: [
        { status: 'ok', saida: 'HTTP 202' },
        { status: 'falha_valida', esperado: 'concluido', obtido: 'pendente' },
      ],
      conclusao: 'falha',
    })

    render(<Testes />)
    await screen.findByText('prezzo · recalcular')
    // Rodar dispara o polling; o estado acima aparece quando o poll
    // acontece (1s depois na vida real — damos 2s de margem).
    fireEvent.click(screen.getByText('Rodar'))

    // A caixa de falha de validação aparece com os dois valores.
    expect(
      await screen.findByText(/Comando executou sem erro/, undefined, { timeout: 3000 }),
    ).toBeInTheDocument()
    expect(screen.getByText('esperado')).toBeInTheDocument()
    expect(screen.getByText('obtido')).toBeInTheDocument()
  })

  it('expandir a suíte busca e mostra o histórico vazio', async () => {
    suitesFalso.mockResolvedValue([suite({ id: 's1', nome: 'suíte x' })])
    historicoFalso.mockResolvedValue([])
    render(<Testes />)
    await screen.findByText('suíte x')

    fireEvent.click(screen.getByText('suíte x'))

    await waitFor(() => {
      expect(historicoFalso).toHaveBeenCalledWith('s1')
    })
    expect(
      await screen.findByText('nenhuma execução anterior registrada.'),
    ).toBeInTheDocument()
  })

  it('histórico com execuções anteriores lista mais recente primeiro e expande os passos ao clicar', async () => {
    suitesFalso.mockResolvedValue([suite({ id: 's1', nome: 'suíte x' })])
    historicoFalso.mockResolvedValue([
      {
        id_execucao: 'exec-2',
        suite_id: 's1',
        iniciada_em_unix: Math.floor(Date.now() / 1000) - 30,
        estados: [{ status: 'ok', saida: 'oi' }, { status: 'ok', saida: 'oi' }],
        conclusao: 'sucesso',
      },
      {
        id_execucao: 'exec-1',
        suite_id: 's1',
        iniciada_em_unix: Math.floor(Date.now() / 1000) - 300,
        estados: [
          { status: 'falha_exec', exit_code: 1, stderr: 'deu ruim' },
          { status: 'pulado' },
        ],
        conclusao: 'falha',
      },
    ])
    render(<Testes />)
    await screen.findByText('suíte x')
    fireEvent.click(screen.getByText('suíte x'))

    await screen.findByText('passou')
    expect(screen.getByText('falhou')).toBeInTheDocument()

    // Clicar na linha de uma execução antiga expande as etapas DAQUELA
    // execução (aqui, a falha de execução com o stderr).
    fireEvent.click(screen.getByText('falhou'))
    expect(await screen.findByText(/Falha de execução — exit 1/)).toBeInTheDocument()
  })

  it('após uma execução terminar, o histórico é recarregado', async () => {
    suitesFalso.mockResolvedValue([suite({ id: 's1', nome: 'suíte x' })])
    execFalso.mockResolvedValue({
      id_execucao: 'exec-1',
      suite_id: 's1',
      iniciada_em_unix: 1,
      estados: [{ status: 'ok' }, { status: 'ok' }],
      conclusao: 'sucesso',
    })
    render(<Testes />)
    await screen.findByText('suíte x')

    fireEvent.click(screen.getByText('Rodar'))

    // O polling roda a 1s (mesmo intervalo real do componente) — damos
    // 3s de margem, igual aos outros testes que dependem do polling.
    await waitFor(
      () => {
        expect(historicoFalso).toHaveBeenCalledWith('s1')
      },
      { timeout: 3000 },
    )
  })

  it('botão "Editar" abre o form pré-preenchido, com id fixo', async () => {
    suitesFalso.mockResolvedValue([
      suite({
        id: 'prezzo',
        nome: 'prezzo · recalcular',
        timeout_etapa_seg: 45,
        passos: [
          { tipo: 'exec', cmd: 'curl -sf ...' },
          { tipo: 'valida', cmd: 'curl ... | jq -r .status', esperado: 'concluido' },
        ],
      }),
    ])
    render(<Testes />)
    await screen.findByText('prezzo · recalcular')

    fireEvent.click(screen.getByRole('button', { name: 'Editar' }))

    // Título muda para "Editar suíte" e o path mostra o id ATUAL da
    // suíte (não "<id>", como aparece na criação de uma nova).
    expect(screen.getByText('Editar suíte')).toBeInTheDocument()
    expect(screen.getByText(/\/etc\/dev-cli\/testes\/prezzo\.toml/)).toBeInTheDocument()
    // Campos vieram preenchidos com os dados da suíte existente.
    expect(screen.getByLabelText('Nome da suíte')).toHaveValue('prezzo · recalcular')
    expect(screen.getByLabelText('Timeout por etapa (s)')).toHaveValue(45)
    expect(screen.getAllByLabelText('Comando CLI')[0]).toHaveValue('curl -sf ...')

    // Renomear a suíte e salvar: o id enviado ao servidor continua
    // "prezzo" (o mesmo arquivo é sobrescrito), não um slug do nome novo.
    fireEvent.change(screen.getByLabelText('Nome da suíte'), {
      target: { value: 'prezzo · recalcular preços v2' },
    })
    fireEvent.click(screen.getByRole('button', { name: 'Salvar alterações' }))

    await waitFor(() => {
      expect(criarFalso).toHaveBeenCalledWith(
        expect.objectContaining({ id: 'prezzo', nome: 'prezzo · recalcular preços v2' }),
      )
    })
  })

  it('botão "Cancelar" chama cancelarExecucao', async () => {
    suitesFalso.mockResolvedValue([suite({ id: 's1', nome: 'suíte x' })])
    // exec roda indefinidamente: o polling vai ler o mesmo estado "rodando"
    // várias vezes (suficiente para vermos o botão "Cancelar").
    execFalso.mockResolvedValue({
      id_execucao: 'exec-1',
      suite_id: 's1',
      iniciada_em_unix: 1,
      estados: [
        { status: 'rodando' },
        { status: 'pendente' },
      ],
      conclusao: 'rodando',
    })
    render(<Testes />)
    await screen.findByText('suíte x')

    fireEvent.click(screen.getByText('Rodar'))
    // Espera o polling pegar e renderizar o botão "Cancelar".
    const botaoCancelar = await screen.findByText('Cancelar')
    fireEvent.click(botaoCancelar)

    await waitFor(() => {
      expect(cancelarFalso).toHaveBeenCalledWith('exec-1')
    })
  })
})
