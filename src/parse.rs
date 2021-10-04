pub fn discard_string<'a>(to_find: &str, s: &'a str) -> Result<&'a str, ()> {
    if s.starts_with(to_find) {
        Ok(&s[to_find.len()..])
    } else {
        Err(())
    }
}

pub fn discard_char(to_find: char, s: &str) -> Result<&str, ()> {
    if s.starts_with(to_find) {
        Ok(&s[1..])
    } else {
        Err(())
    }
}
