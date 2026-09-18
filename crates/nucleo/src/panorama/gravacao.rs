// Gravação atômica do snapshot e retenção dos arquivos antigos.
//
// Por que atômico? O consumidor lê o MESMO diretório enquanto o binário
// escreve. Escrever direto no arquivo final abre uma janela em que ele lê
// JSON truncado. O padrão é: escrever num `.tmp`, `sync` do arquivo e
// `rename` para o nome final — o rename é atômico dentro do mesmo filesystem.
//
// A idade de um snapshot sai do NOME do arquivo (precisão de segundo),
// jamais do mtime: mtime muda com cópia e backup.

use std::fs;
// Extensão só de unix: o `cfg` evita que o crate deixe de compilar no
// Windows, para onde o release também publica binário.
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use chrono::{NaiveDate, NaiveDateTime};

/// Nome do arquivo a partir de um `coletado_em` ISO ("2026-09-18T19:22:37"):
/// "panorama_snapshot_20260918_192237.json" (prefixo fixo + `yyyymmdd_hhmmss`).
/// Precisão de segundo: cada coleta gera um arquivo próprio, sem sobrescrever
/// a anterior. Ordem lexicográfica = ordem cronológica, então "o mais recente"
/// é o último da lista ordenada. Devolve `None` quando o texto não é um ISO
/// válido — em vez de gerar um arquivo com carimbo mentiroso.
pub fn nome_arquivo(coletado_em: &str) -> Option<String> {
    // `parse_from_str` valida de verdade (mês 13 ou hora 25 dão `Err`); o
    // `format` seguinte só reordena os campos já validados para o padrão
    // compacto do nome.
    // docs: https://docs.rs/chrono/latest/chrono/naive/struct.NaiveDateTime.html#method.parse_from_str
    let instante = NaiveDateTime::parse_from_str(coletado_em, "%Y-%m-%dT%H:%M:%S").ok()?;
    Some(
        instante
            .format("panorama_snapshot_%Y%m%d_%H%M%S.json")
            .to_string(),
    )
}

/// Grava o conteúdo num arquivo temporário, sincroniza e renomeia para o
/// nome final (passo atômico). Cria o diretório se não existir e garante que
/// o arquivo fique legível pelo grupo (o consumidor roda como outro usuário)
/// mas NÃO gravável por outros usuários.
pub fn gravar_atomico(
    diretorio: &Path,
    nome_final: &str,
    conteudo: &str,
) -> Result<PathBuf, Box<dyn std::error::Error>> {
    fs::create_dir_all(diretorio)?;

    let caminho_final = diretorio.join(nome_final);
    // O temporário mora no MESMO diretório: só assim o rename final é
    // atômico (a atomicidade exige o mesmo filesystem; gravar em /tmp e
    // renomear para outro diretório não é atômico).
    let caminho_tmp = diretorio.join(format!("{nome_final}.tmp"));

    let resultado = (|| -> Result<(), Box<dyn std::error::Error>> {
        fs::write(&caminho_tmp, conteudo)?;
        // `sync_all`: garante que os bytes chegaram ao disco ANTES do rename
        // — sem isso, um crash entre o nome e o conteúdo poderia deixar um
        // arquivo final com dados pela metade.
        // docs: https://doc.rust-lang.org/std/fs/struct.File.html#method.sync_all
        let arquivo = fs::File::open(&caminho_tmp)?;
        arquivo.sync_all()?;

        // 0o644: dono escreve/lê, grupo lê, outros só leem — legível pelo
        // consumidor (outro usuário do grupo) e não gravável por terceiros.
        //
        // Só em unix: modo POSIX não existe no Windows. O coletor roda em
        // Linux (systemd, socket do docker), mas o crate precisa COMPILAR
        // no Windows — `nucleo` entra no binário que o release publica lá.
        // Sem permissão explícita, o arquivo herda a ACL do diretório, que
        // é o comportamento razoável na plataforma onde isto não roda.
        #[cfg(unix)]
        {
            let mut permissao = fs::metadata(&caminho_tmp)?.permissions();
            permissao.set_mode(0o644);
            fs::set_permissions(&caminho_tmp, permissao)?;
        }

        fs::rename(&caminho_tmp, &caminho_final)?;
        Ok(())
    })();

    // Nada de `.tmp` sobrando: qualquer falha no caminho acima remove o
    // temporário antes de devolver o erro.
    if resultado.is_err() {
        let _ = fs::remove_file(&caminho_tmp);
    }
    resultado?;
    Ok(caminho_final)
}

/// Lê o `NaiveDateTime` embutido no nome "panorama_snapshot_20260807_140000.json".
/// Nomes fora do padrão devolvem `None` — e por isso são IGNORADOS pela
/// retenção, nunca apagados.
fn data_do_nome(nome: &str) -> Option<NaiveDateTime> {
    let sem_extensao = nome.strip_suffix(".json")?;
    // Formato atual: prefixo fixo + carimbo compacto `yyyymmdd_hhmmss`.
    if let Some(carimbo) = sem_extensao.strip_prefix("panorama_snapshot_") {
        return NaiveDateTime::parse_from_str(carimbo, "%Y%m%d_%H%M%S").ok();
    }
    // Legado ("2026-08-07T14", granularidade de hora): ainda entendido para a
    // retenção continuar limpando arquivos gravados antes da renomeação — sem
    // isso eles virariam órfãos no diretório para sempre.
    let (dia, hora) = sem_extensao.split_once('T')?;
    let data = NaiveDate::parse_from_str(dia, "%Y-%m-%d").ok()?;
    let hora: u32 = hora.parse().ok()?;
    data.and_hms_opt(hora, 0, 0)
}

/// Remove do diretório os snapshots mais velhos que `retencao_dias` dias.
///
/// A idade é calculada do NOME (não do mtime) comparando com o instante de
/// referência `agora`. Devolve a lista de arquivos removidos (para quem
/// quiser avisar). Leitura do diretório que falha é erro; remoção que falha
/// é IGNORADA — faxina não derruba uma coleta já gravada.
pub fn aplicar_retencao(
    diretorio: &Path,
    retencao_dias: u64,
    agora: &str,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    let agora_dt = NaiveDateTime::parse_from_str(agora, "%Y-%m-%dT%H:%M:%S")
        .map_err(|erro| format!("agora fora do formato ISO esperado: {erro}"))?;
    let limite = chrono::Duration::days(retencao_dias as i64);

    let mut removidos = Vec::new();
    for entrada in fs::read_dir(diretorio)? {
        let entrada = match entrada {
            Ok(e) => e,
            // Alguém criou/apagou arquivo no meio da faxina: segue em frente.
            Err(_) => continue,
        };
        let caminho = entrada.path();
        if !caminho.is_file() {
            continue;
        }
        let nome = entrada.file_name().to_string_lossy().into_owned();
        // Fora do padrão ("README", "notas.txt"): ignorado, NÃO apagado.
        let Some(data) = data_do_nome(&nome) else {
            continue;
        };
        let idade = agora_dt.signed_duration_since(data);
        if idade > limite {
            match fs::remove_file(&caminho) {
                Ok(()) => removidos.push(nome),
                // Falha ao remover é AVISO, nunca erro fatal.
                Err(_) => continue,
            }
        }
    }
    Ok(removidos)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nome_arquivo_deriva_do_coletado_em() {
        assert_eq!(
            nome_arquivo("2026-09-18T19:22:37").as_deref(),
            Some("panorama_snapshot_20260918_192237.json")
        );
        // Precisão de segundo: coletas distintas geram arquivos distintos.
        assert_eq!(
            nome_arquivo("2026-09-18T19:22:38").as_deref(),
            Some("panorama_snapshot_20260918_192238.json")
        );
        // Texto fora do ISO (ou curto demais): sem nome, em vez de um
        // arquivo com carimbo mentiroso.
        assert_eq!(nome_arquivo("2026-08-07"), None);
        assert_eq!(nome_arquivo("gibberish"), None);
    }

    #[test]
    fn grava_atomico_cria_o_arquivo_sem_tmp_residual() {
        let diretorio = tempfile::tempdir().expect("tempdir do teste");

        let caminho = gravar_atomico(
            diretorio.path(),
            "panorama_snapshot_20260807_140000.json",
            "{\"versao\":1}",
        )
        .expect("gravação atômica");

        // Conteúdo completo e nenhum `.tmp` sobrou no diretório.
        let texto = std::fs::read_to_string(&caminho).unwrap();
        assert_eq!(texto, "{\"versao\":1}");
        let entradas: Vec<_> = std::fs::read_dir(diretorio.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(entradas.iter().all(|nome| !nome.ends_with(".tmp")));
    }

    #[test]
    fn gravar_atomico_cria_o_diretorio_se_ausente() {
        let raiz = tempfile::tempdir().unwrap();
        let subdir = raiz.path().join("um").join("dois");
        let caminho = gravar_atomico(
            &subdir,
            "panorama_snapshot_20260807_140000.json",
            "{\"a\":1}",
        )
        .unwrap();
        assert!(caminho.is_file());
    }

    #[test]
    fn retencao_remove_velhos_preserva_novos_e_ignora_fora_do_padrao() {
        let diretorio = tempfile::tempdir().unwrap();
        let agora = "2026-08-07T12:00:00";

        // Um snapshot de ontem (deve sumir) e um de hoje (deve ficar).
        fs::write(
            diretorio
                .path()
                .join("panorama_snapshot_20260806_100000.json"),
            "{}",
        )
        .unwrap();
        fs::write(
            diretorio
                .path()
                .join("panorama_snapshot_20260807_100000.json"),
            "{}",
        )
        .unwrap();
        // Fora do padrão: ignorado, NUNCA apagado.
        fs::write(diretorio.path().join("notas.txt"), "não sou snapshot").unwrap();
        fs::write(diretorio.path().join("2026-08-07.json"), "sem carimbo").unwrap();

        let removidos = aplicar_retencao(diretorio.path(), 1, agora).unwrap();
        assert_eq!(
            removidos,
            vec!["panorama_snapshot_20260806_100000.json".to_string()]
        );

        let restantes: Vec<_> = std::fs::read_dir(diretorio.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(restantes.contains(&"panorama_snapshot_20260807_100000.json".to_string()));
        assert!(restantes.contains(&"notas.txt".to_string()));
        assert!(restantes.contains(&"2026-08-07.json".to_string()));
    }

    #[test]
    fn retencao_entende_nome_legado() {
        // Arquivos gravados antes da renomeação ("2026-08-06T10.json")
        // continuam sendo limpos pela retenção em vez de virarem órfãos.
        let diretorio = tempfile::tempdir().unwrap();
        fs::write(diretorio.path().join("2026-08-06T10.json"), "{}").unwrap();
        let removidos = aplicar_retencao(diretorio.path(), 1, "2026-08-07T12:00:00").unwrap();
        assert_eq!(removidos, vec!["2026-08-06T10.json".to_string()]);
    }

    #[test]
    fn retencao_zero_remove_tudo_do_padrao() {
        let diretorio = tempfile::tempdir().unwrap();
        fs::write(
            diretorio
                .path()
                .join("panorama_snapshot_20260807_040000.json"),
            "{}",
        )
        .unwrap();
        let removidos = aplicar_retencao(diretorio.path(), 0, "2026-08-07T05:00:00").unwrap();
        assert_eq!(
            removidos,
            vec!["panorama_snapshot_20260807_040000.json".to_string()]
        );
    }
}
