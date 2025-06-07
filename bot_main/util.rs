use once_cell::sync::Lazy;
use poise::serenity_prelude::CreateAttachment;

pub fn diff(before: &str, after: &str) -> String {
    let input = imara_diff::intern::InternedInput::new(before, after);
    imara_diff::diff(
        imara_diff::Algorithm::Histogram,
        &input,
        imara_diff::UnifiedDiffBuilder::new(&input),
    )
}

pub fn code_block_or_file(
    description: impl Into<String>,
    code: impl Into<Vec<u8>>,
    filestem: &str,
    extension: &str,
) -> (String, Vec<CreateAttachment>) {
    let description = description.into();
    let code = code.into();

    // Character limit is 2000 (bytes? glyphs?) minus the backticks and extension, we'll play it safe.
    // Triple backticks would end the code block early, so we can't allow them in the code.
    static RE: Lazy<regex::bytes::Regex> = Lazy::new(|| regex::bytes::Regex::new(r"```").unwrap());
    if code.len() + description.len() > 1980 || RE.is_match(&code) {
        let attachment = CreateAttachment::bytes(code.to_vec(), format!("{filestem}.{extension}"));
        (description, vec![attachment])
    } else {
        let code = String::from_utf8_lossy(&code);
        (format!("{description}\n```{extension}\n{code}\n```"), vec![])
    }
}
