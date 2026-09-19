use imara_diff::{BasicLineDiffPrinter, Diff, InternedInput, UnifiedDiffConfig};

pub fn diff(before: &str, after: &str) -> String {
    let input = InternedInput::new(before, after);
    let mut diff = Diff::compute(imara_diff::Algorithm::Histogram, &input);
    diff.postprocess_lines(&input);
    diff.unified_diff(&BasicLineDiffPrinter(&input.interner), UnifiedDiffConfig::default(), &input).to_string()
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
