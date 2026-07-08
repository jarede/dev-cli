// CASCA DE IO: subcomando `logs containers`.
// Lê logs de containers locais via `container logs`, com suporte a
// `--follow` (modo tempo real com painel redesenhavel).

use std::collections::BTreeMap;
// `BufRead` traz o método `.lines()` para leitores bufferizados (usado no
// modo `-f`, lendo o stdout do processo filho linha a linha).
// docs: https://doc.rust-lang.org/std/io/trait.BufRead.html#method.lines
use std::io::BufRead;
// `Write` traz o método `.flush()`, usado para forçar a stdout bufferizada a
// aparecer imediatamente no terminal (ver comentários mais abaixo).
// docs: https://doc.rust-lang.org/std/io/trait.Write.html#method.flush
use std::io::Write;

use clap::Args;

use nucleo::core::contar_niveis_container;
use crate::logs::render::renderizar_container;

/// Estatísticas de logs de todos os containers detectados via `container list`.
#[derive(Args, Debug)]
#[command(help_template = crate::help::ARGUMENTOS, next_help_heading = crate::help::OPCOES)]
pub struct ContainersArgs {
    /// Container específico; se omitido, varre todos os detectados por `container list`.
    // `Option<String>`: o clap deixa esse argumento posicional opcional porque
    // o tipo é `Option`; sem valor na linha de comando, vira `None`.
    // docs: https://doc.rust-lang.org/std/option/enum.Option.html
    #[arg(help_heading = crate::help::ARGUMENTOS_HEADING)]
    container: Option<String>,
    /// Acompanha os logs em tempo real (como `tail -f`), redesenhando o painel a cada linha nova.
    // `bool` com `#[arg(short, long)]`: o clap trata automaticamente como uma
    // flag (`-f`/`--follow`) que não recebe valor — presente = `true`.
    // docs: https://docs.rs/clap/latest/clap/_derive/index.html
    #[arg(short = 'f', long, help_heading = crate::help::OPCOES)]
    follow: bool,
}

impl ContainersArgs {
    pub fn execute(&self) -> Result<String, Box<dyn std::error::Error>> {
        // `if let ... else`: um único container pedido vira um Vec de 1;
        // sem container, perguntamos ao binário `container` quem está rodando.
        let nomes = if let Some(container) = &self.container {
            vec![container.clone()]
        } else {
            listar_containers()?
        };

        if self.follow {
            // Modo ao vivo: fica bloqueado imprimindo atualizações direto no
            // terminal. Só retorna quando todos os containers pararem de logar
            // (ou nunca, se o usuário encerrar com Ctrl+C antes disso).
            seguir_containers(&nomes)?;
            return Ok(String::new());
        }

        let mut saida = String::new();
        for nome in nomes {
            let conteudo = obter_logs(&nome)?;
            // NÚCLEO PURO: mesmo princípio do `contar` acima, mas adaptado ao
            // formato colorido (códigos ANSI) que `container logs` emite.
            let niveis = contar_niveis_container(&conteudo);
            saida.push_str(&renderizar_container(&nome, None, &niveis));
        }

        Ok(saida.trim_end().to_string())
    }
}

/// CASCA DE IO: modo `-f`. Abre um processo `container logs -f <nome>` por
/// container; cada um é lido numa thread própria (senão o `for` bloqueante de
/// uma leitura travaria a leitura das outras). As threads só enviam linhas por
/// um canal `mpsc`; quem acumula contagens e redesenha o painel é a thread
/// principal, evitando qualquer mutex compartilhado entre elas.
fn seguir_containers(nomes: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    // `mpsc` = multi-producer, single-consumer: uma thread por container
    // (produtoras) alimentam um único receptor aqui na thread principal.
    // docs: https://doc.rust-lang.org/std/sync/mpsc/fn.channel.html
    let (tx, rx) = std::sync::mpsc::channel::<(String, String)>();

    for nome in nomes {
        let mut child = std::process::Command::new("container")
            .args(["logs", "-f", nome])
            .stdout(std::process::Stdio::piped())
            .spawn()
            .map_err(|erro| format!("falha ao executar 'container logs -f {nome}': {erro}"))?;

        // `take()` retira o stdout de dentro do `Child` (deixando `None` no
        // lugar) para podermos movê-lo para a thread de leitura junto com o
        // resto do `child` (que precisamos manter vivo até o processo acabar).
        // docs: https://doc.rust-lang.org/std/option/enum.Option.html#method.take
        // docs: https://doc.rust-lang.org/std/option/enum.Option.html#method.ok_or
        // docs: https://doc.rust-lang.org/std/process/struct.Child.html#structfield.stdout
        let stdout = child
            .stdout
            .take()
            .ok_or("não foi possível capturar a saída do container")?;

        let tx = tx.clone();
        let nome_da_thread = nome.clone();
        std::thread::spawn(move || {
            let leitor = std::io::BufReader::new(stdout);
            // `lines()` bloqueia entre chamadas até a próxima linha chegar —
            // é isso que dá o efeito de "tempo real"; `map_while(Result::ok)`
            // ignora erros de leitura e para no primeiro (ex.: pipe fechado).
            // docs: https://doc.rust-lang.org/std/io/trait.BufRead.html#method.lines
            // docs: https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.map_while
            for linha in leitor.lines().map_while(Result::ok) {
                // Erro de `send` só ocorre se a thread principal já saiu;
                // ignoramos porque não há mais ninguém para avisar.
                // docs: https://doc.rust-lang.org/std/sync/mpsc/struct.Sender.html#method.send
                let _ = tx.send((nome_da_thread.clone(), linha));
            }
            // Espera o processo `container logs -f` encerrar de fato (evita
            // deixá-lo como zumbi quando o container para).
            // docs: https://doc.rust-lang.org/std/process/struct.Child.html#method.wait
            let _ = child.wait();
        });
    }
    // Descarta nosso clone original do transmissor: o canal só fecha (e o
    // `for` abaixo termina) quando TODAS as threads soltarem o `tx` delas.
    // docs: https://doc.rust-lang.org/std/mem/fn.drop.html
    drop(tx);

    // Contagem acumulada por container, viva só na thread principal (não
    // precisa de `Mutex`: nenhuma outra thread toca nela, só recebem por canal).
    // `collect()` monta o `BTreeMap` a partir de um iterador de tuplas
    // `(chave, valor)` — cada container começa com um mapa de níveis vazio.
    // docs: https://doc.rust-lang.org/std/iter/trait.Iterator.html#method.collect
    let mut totais: BTreeMap<String, BTreeMap<String, usize>> = nomes
        .iter()
        .map(|nome| (nome.clone(), BTreeMap::new()))
        .collect();

    // `Receiver` (o `rx`) implementa `Iterator`: o `for` bloqueia esperando a
    // próxima mensagem e só termina quando todos os `tx` (um por thread) forem
    // descartados, ou seja, quando todas as threads produtoras acabarem.
    // docs: https://doc.rust-lang.org/std/sync/mpsc/struct.Receiver.html
    for (nome, linha) in rx {
        let niveis_da_linha = contar_niveis_container(&linha);
        // `entry(nome).or_default()`: garante um `BTreeMap<String, usize>`
        // vazio para containers que ainda não apareceram no canal, antes de
        // somar os níveis desta linha nele.
        // docs: https://doc.rust-lang.org/std/collections/struct.BTreeMap.html#method.entry
        // docs: https://doc.rust-lang.org/std/collections/btree_map/enum.Entry.html#method.or_default
        let acumulado = totais.entry(nome).or_default();
        for (nivel, quantidade) in niveis_da_linha {
            // Mesmo idiom de `entry(...).or_insert(0) +=` usado em `contar` e
            // `contar_niveis_container`: soma `quantidade` ao total daquele
            // nível, partindo de `0` se for a primeira ocorrência.
            // docs: https://doc.rust-lang.org/std/collections/struct.BTreeMap.html#method.entry
            // docs: https://doc.rust-lang.org/std/collections/btree_map/enum.Entry.html#method.or_insert
            *acumulado.entry(nivel).or_insert(0) += quantidade;
        }

        // Redesenha o painel inteiro a cada linha nova: limpa a tela e move o
        // cursor para o topo (códigos ANSI), como um dashboard ao vivo.
        // docs: https://doc.rust-lang.org/std/macro.print.html
        print!("\x1b[2J\x1b[H");
        for nome in nomes {
            if let Some(niveis) = totais.get(nome) {
                print!("{}", renderizar_container(nome, None, niveis));
            }
        }
        // `print!` só escreve no buffer da stdout; sem `flush` o painel não
        // apareceria até o buffer encher.
        // docs: https://doc.rust-lang.org/std/io/trait.Write.html#tymethod.flush
        std::io::stdout().flush()?;
    }

    Ok(())
}

/// CASCA DE IO: pergunta ao binário `container` quais containers existem.
/// `-q` faz o comando devolver só os IDs/nomes, um por linha.
fn listar_containers() -> Result<Vec<String>, Box<dyn std::error::Error>> {
    // `Command::new(...).args([...]).output()` executa o processo, espera ele
    // terminar e captura stdout/stderr/status de uma vez (diferente de
    // `spawn()`, que devolve o `Child` já em execução para controlarmos nós
    // mesmos — usado em `seguir_containers` para o modo `-f`).
    let saida = std::process::Command::new("container")
        .args(["list", "-q"])
        .output()
        .map_err(|erro| format!("falha ao executar 'container list': {erro}"))?;

    if !saida.status.success() {
        return Err(format!(
            "'container list' terminou com erro: {}",
            String::from_utf8_lossy(&saida.stderr)
        )
        .into());
    }

    // `stdout`/`stderr` vêm como `Vec<u8>` (bytes crus); `from_utf8_lossy`
    // converte para `str` trocando bytes inválidos por `�` em vez de falhar.
    Ok(String::from_utf8_lossy(&saida.stdout)
        .lines()
        // `str::trim`/`str::to_string` usados como valor (sem `|x| x.trim()`):
        // toda função de um argumento pode virar closure implícita em `map`.
        .map(str::trim)
        .filter(|linha| !linha.is_empty())
        .map(str::to_string)
        .collect())
}

/// CASCA DE IO: busca o log completo de um container via `container logs`.
fn obter_logs(nome: &str) -> Result<String, Box<dyn std::error::Error>> {
    let saida = std::process::Command::new("container")
        .args(["logs", nome])
        .output()
        .map_err(|erro| format!("falha ao executar 'container logs {nome}': {erro}"))?;

    if !saida.status.success() {
        return Err(format!(
            "'container logs {nome}' terminou com erro: {}",
            String::from_utf8_lossy(&saida.stderr)
        )
        .into());
    }

    Ok(String::from_utf8_lossy(&saida.stdout).to_string())
}
