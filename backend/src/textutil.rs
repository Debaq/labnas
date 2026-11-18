//! Utilidades de texto seguras con UTF-8 (`&s[..n]` hace panic si corta una tilde o un emoji)

/// Corta a `max` caracteres; si corta, agrega "..." (el total no supera `max`)
pub fn ellipsize(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let keep = max.saturating_sub(3);
    let mut out: String = s.chars().take(keep).collect();
    out.push_str("...");
    out
}

/// Corta a `max` caracteres, sin agregar nada
pub fn take_chars(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_parte_multibyte() {
        assert_eq!(ellipsize("hola", 10), "hola");
        assert_eq!(ellipsize("áéíóúáéíóú", 6), "áéí...");
        assert_eq!(ellipsize("🙂🙂🙂🙂", 4), "🙂🙂🙂🙂");
        assert_eq!(ellipsize("🙂🙂🙂🙂🙂", 4), "🙂...");
        assert_eq!(take_chars("ñandú", 3), "ñan");
    }
}
