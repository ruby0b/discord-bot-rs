use bot_core::audio::Playable;
use bot_core::hash_store;
use eyre::{Result, ensure};
use futures::{StreamExt, TryStreamExt, stream};

const GOOGLE_TTS_MAX_CHARS: usize = 100;

pub async fn get_tts(text: &str) -> Result<Playable> {
    hash_store::get_or_store(format!("tts/{text}").as_bytes(), "mp3", async {
        stream::iter(text.split_whitespace().fold(vec!["".to_string()], |mut chunks, w| {
            let last = chunks.last_mut().unwrap();
            if last.len() + w.len() + 1 > GOOGLE_TTS_MAX_CHARS {
                chunks.push(w.to_string());
            } else {
                last.push(' ');
                last.push_str(w);
            }
            chunks
        }))
        .then(async |x| get_tts_chunk(x.as_str()).await)
        .try_concat()
        .await
    })
    .await
    .map(Playable::file)
}

async fn get_tts_chunk(text: &str) -> Result<Vec<u8>> {
    ensure!(!text.is_empty(), "Empty TTS text");
    ensure!(text.len() <= GOOGLE_TTS_MAX_CHARS, "TTS text is too long");

    let language = "de";
    Ok(reqwest::Client::new()
        .get("https://translate.google.com/translate_tts")
        .query(&[
            ("ie", "UTF-8"),
            ("q", text),
            ("tl", language),
            ("total", "1"),
            ("idx", "0"),
            ("textlen", text.len().to_string().as_str()),
            ("tl", language),
            ("client", "tw-ob"),
        ])
        .send()
        .await?
        .bytes()
        .await?
        .to_vec())
}
