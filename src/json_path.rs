use serde_json::Value;

pub fn get_path<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for part in path {
        current = current.get(*part)?;
    }
    Some(current)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_get_path_single_level() {
        let value = json!({"name": "Alice"});
        let result = get_path(&value, &["name"]);
        assert_eq!(result, Some(&json!("Alice")));
    }

    #[test]
    fn test_get_path_nested() {
        let value = json!({"user": {"name": "Bob", "age": 30}});
        let result = get_path(&value, &["user", "name"]);
        assert_eq!(result, Some(&json!("Bob")));
    }

    #[test]
    fn test_get_path_deeply_nested() {
        let value = json!({"a": {"b": {"c": {"d": "deep"}}}});
        let result = get_path(&value, &["a", "b", "c", "d"]);
        assert_eq!(result, Some(&json!("deep")));
    }

    #[test]
    fn test_get_path_missing_key() {
        let value = json!({"name": "Alice"});
        let result = get_path(&value, &["email"]);
        assert_eq!(result, None);
    }

    #[test]
    fn test_get_path_missing_nested_key() {
        let value = json!({"user": {"name": "Bob"}});
        let result = get_path(&value, &["user", "email"]);
        assert_eq!(result, None);
    }

    #[test]
    fn test_get_path_empty_path() {
        let value = json!({"name": "Alice"});
        let result = get_path(&value, &[]);
        assert_eq!(result, Some(&value));
    }

    #[test]
    fn test_get_path_array_element() {
        let value = json!({"items": [1, 2, 3]});
        let result = get_path(&value, &["items"]);
        assert_eq!(result, Some(&json!([1, 2, 3])));
    }
}
