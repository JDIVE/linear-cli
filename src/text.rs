pub fn truncate(value: &str, max_len: Option<usize>) -> String {
    let Some(max_len) = max_len else {
        return value.to_string();
    };
    if max_len == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut count = 0;

    for ch in value.chars() {
        if count + 1 > max_len {
            break;
        }
        out.push(ch);
        count += 1;
    }

    if value.chars().count() > max_len && max_len > 3 {
        out.truncate(max_len.saturating_sub(3));
        out.push_str("...");
    }

    out
}
