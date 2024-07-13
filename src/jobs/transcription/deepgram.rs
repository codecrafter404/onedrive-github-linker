use std::path::PathBuf;

use color_eyre::eyre::OptionExt;
use deepgram::transcription::prerecorded::{
    audio_source::AudioSource,
    options::{self, OptionsBuilder},
};
use graph_rs_sdk::{GraphClient, ODataQuery};
use serde::Deserialize;

use crate::utils::config::{Config, TranscriptionConfig};

use super::link::Link;

pub(crate) async fn transcribe_link(
    link: &Link,
    conf: &Config,
    deepgram: deepgram::Deepgram,
    graph: &GraphClient,
) -> color_eyre::Result<()> {
    let transcription_config = conf.transcription.clone().unwrap();
    let source = get_source(link, conf, graph).await?;
    let options = OptionsBuilder::new()
        .model(options::Model::Nova2Meeting)
        .diarize(true)
        .detect_language(true)
        .summarize("v2")
        .topics(true)
        .smart_format(true)
        .punctuate(true)
        .paragraphs(true);

    //TODO: finish & check if the crate has been updated yet

    unimplemented!();
}
async fn get_source(
    link: &Link,
    config: &Config,
    graph: &GraphClient,
) -> color_eyre::Result<AudioSource> {
    let res = match &link.link_target {
        crate::jobs::transcription::link::LinkType::FileSytemLink(rel_path) => {
            let path = config
                .git_directory
                .join(rel_path.strip_prefix("/").unwrap_or(&rel_path));
            let file = tokio::fs::File::open(path).await?;
            AudioSource::from_buffer(file)
        }
        crate::jobs::transcription::link::LinkType::WebLink(link) => {
            AudioSource::from_url(link.clone())
        }
        crate::jobs::transcription::link::LinkType::OneNoteLink(link) => {
            AudioSource::from_url(get_onenote_download_link(link.clone(), graph).await?)
        }
    };
    Ok(res)
}
#[derive(Debug, Deserialize)]
struct GraphResponse {
    #[serde(rename = "@microsoft.graph.downloadUrl")]
    download_url: String,
}
async fn get_onenote_download_link(
    path: PathBuf,
    graph: &GraphClient,
) -> color_eyre::Result<reqwest::Url> {
    let path = path.strip_prefix("/").unwrap_or(&path);
    let file = graph
        .me()
        .drive()
        .item_by_path(format!(
            ":/{}:",
            path.to_str()
                .ok_or_eyre(format!("Expected path {:?} to be parsable", path))?
        ))
        .get_items()
        .select(&["@microsoft.graph.downloadUrl"])
        .send()
        .await?
        .json::<GraphResponse>()
        .await?;
    Ok(file.download_url.parse()?)
}