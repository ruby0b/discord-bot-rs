use imara_diff::{BasicLineDiffPrinter, Diff, InternedInput, UnifiedDiffConfig};
use once_cell::sync::Lazy;
use poise::serenity_prelude::CreateAttachment;

pub fn diff(before: &str, after: &str) -> String {
    let input = InternedInput::new(before, after);
    let mut diff = Diff::compute(imara_diff::Algorithm::Histogram, &input);
    diff.postprocess_lines(&input);
    diff.unified_diff(&BasicLineDiffPrinter(&input.interner), UnifiedDiffConfig::default(), &input)
        .to_string()
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

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn basic_diff() {
        let before = r#"fn foo() -> Bar {
    let mut foo = 2;
    foo *= 50;
    println!("hello world")
}
"#;

        let after = r#"// lorem ipsum
fn foo() -> Bar {
    let mut foo = 2;
    foo *= 50;
    println!("hello world");
    println!("{foo}");
}
// foo
"#;
        assert_eq!(
            diff(before, after),
            r#"@@ -1,5 +1,8 @@
+// lorem ipsum
 fn foo() -> Bar {
     let mut foo = 2;
     foo *= 50;
-    println!("hello world")
+    println!("hello world");
+    println!("{foo}");
 }
+// foo
"#
        )
    }
}
