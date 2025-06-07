pub enum Chunk {
    Text(String),
    Variable(String),
}

pub fn template_to_chunks(template: &str) -> Vec<Chunk> {
    let mut chunks = Vec::<Chunk>::with_capacity(1);

    let mut last_i = 0;
    let bytes = template.as_bytes();
    let mut iter = bytes.iter().copied().enumerate();

    while let Some((i, c)) = iter.next() {
        if c != b'{' {
            continue;
        }

        let text_chunk = unsafe { get_unchecked(bytes, last_i, i) };
        chunks.push(Chunk::Text(text_chunk.to_string()));

        for (sub_i, c) in iter.by_ref() {
            if c != b'}' {
                continue;
            }

            let var = unsafe { get_unchecked(bytes, i + 1, sub_i) };
            chunks.push(Chunk::Variable(var.to_string()));

            last_i = sub_i + 1;
            break;
        }
    }

    if last_i < template.len() {
        let text_chunk = unsafe { get_unchecked(bytes, last_i, template.len()) };
        chunks.push(Chunk::Text(text_chunk.to_string()));
    }

    chunks
}

unsafe fn get_unchecked(bytes: &[u8], lower: usize, upper: usize) -> &str {
    unsafe { std::str::from_utf8_unchecked(bytes.get_unchecked(lower..upper)) }
}
