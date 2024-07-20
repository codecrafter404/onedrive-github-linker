use std::path::{Path, PathBuf};

use crate::utils::git;
use chrono::{DateTime, TimeZone, Utc};
use color_eyre::eyre::{eyre, Context, OptionExt};
use itertools::Itertools;
use link::Link;
use markdown::CorrelatingFile;

use crate::utils::config::Config;

mod deepgram;
mod file_discovery;
mod file_meta;
mod link;
pub mod markdown;
mod template;

pub async fn transcribe_audio(conf: &Config) -> color_eyre::Result<()> {
    let transcription_conf = conf
        .transcription
        .clone()
        .ok_or_eyre("Expected transcription conf to be initialized")?;

    git::check_out_create_branch(&transcription_conf.git_source_branch, &conf)?;

    let files_to_transcribe = file_discovery::discover_files(conf)?;

    let deepgram = ::deepgram::Deepgram::new(transcription_conf.deepgram_key);
    let credentials =
        crate::utils::credentials::get_onedrive_credentials(&conf.credentials).await?;

    let graph = graph_rs_sdk::GraphClient::new(credentials.token);

    git::check_out_create_branch(&transcription_conf.git_source_branch, &conf)?;

    let mut processed = vec![];
    let mut links = Vec::new();
    for file in files_to_transcribe {
        match Link::from_path(&file, &conf) {
            Ok(x) => links.push((file, x)),
            Err(why) => {
                log::error!("Failed to parse link for file {:?}: {:?}", file, why);
            }
        }
    }

    if links.is_empty() {
        log::info!("didn't get any links");
        return Ok(());
    }

    git::check_out_create_branch(&transcription_conf.git_target_branch, &conf)?;

    let (blamed_files, _) =
        git::blame::BlamedFile::blame_all(&conf).wrap_err("Failed to blame directory tree")?;

    for (file, link) in links {
        match process_file(conf, file.clone(), link.clone(), &deepgram, &graph).await {
            Ok(_) => {
                processed.push(file);
            }
            Err(why) => {
                log::error!("Failed to proccess link: {:?}", why);
            }
        }
    }
    // commit changes
    if processed.len() > 0 {
        git::wrap_git_command_error(&git::git_command_wrapper(
            &["add", "*"],
            &conf.git_directory,
            conf,
        )?)?;
        git::wrap_git_command_error(&git::git_command_wrapper(
            &[
                "commit",
                "-m",
                &format!(
                    "transcribed: {}",
                    processed
                        .iter()
                        .map(|x| x
                            .file_name()
                            .unwrap_or_default()
                            .to_str()
                            .unwrap_or_default())
                        .join(",")
                ),
                "--author",
                git::GIT_AUTHOR,
            ],
            &conf.git_directory,
            conf,
        )?)?;
    }
    Ok(())
}
/// expects to be in the right git branch
async fn process_file(
    conf: &Config,
    file_to_transcribe: PathBuf,
    link: Link,
    deepgram: &::deepgram::Deepgram,
    graph: &graph_rs_sdk::GraphClient,
) -> color_eyre::Result<()> {
    let transcription_config = conf
        .transcription
        .clone()
        .ok_or_eyre("Expected transcription config to be set")?;
    log::info!("Proccessing link '{:?}'", file_to_transcribe);
    let transcription_result = deepgram::transcribe_link(&link, conf, deepgram, graph)
        .await
        .wrap_err(eyre!("Failed to transcribe file"))?;
    let file_content = template::get_transcription_file(&transcription_result, &link)?;
    let file_without_link_extension = file_to_transcribe
        .file_name()
        .ok_or_eyre("expected to get filename")?
        .to_str()
        .ok_or_eyre("Expected to parse filename")?
        .to_string()
        .strip_suffix(".link")
        .ok_or_eyre("Infallible")?
        .to_string();
    let target_file_name = format!("{}.transcript.md", file_without_link_extension);
    let dir = conf.git_directory.join(
        transcription_config
            .transcription_target_path
            .strip_prefix("/")
            .unwrap_or(&transcription_config.transcription_target_path),
    );

    std::fs::create_dir_all(dir.clone())?;
    let path = dir.join(target_file_name.clone());
    std::fs::write(path.clone(), file_content)?;

    Ok(())
}
fn handle_correlating_file(
    file: CorrelatingFile,
    link: &Link,
    transcript: PathBuf,
) -> color_eyre::Result<()> {
    log::info!("Got correlating file: {:?}", file);
    let correlating_file_path = file.path.clone();
    let content = std::fs::read_to_string(correlating_file_path.clone())?;
    let new_file = file.link_to_transcript(transcript, &content, &link.last_modified)?;
    std::fs::write(correlating_file_path, new_file)?;
    Ok(())
}
