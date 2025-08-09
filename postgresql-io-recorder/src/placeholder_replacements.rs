fn find_first_placeholder(query: &str) -> Option<String> {
    let (_before, after) = query.split_once("$")?;
    // find first non-alphabetic character after "$"
    let placeholder = after.chars().take_while(|c| c.is_ascii_alphanumeric()).collect::<String>();
    Some(placeholder)
}

#[test]
fn test_find_placeholder() {
    let query = "SELECT $id";
    let first_placeholder = find_first_placeholder(query).unwrap();
    assert_eq!(first_placeholder, "id");
    let query = "SELECT $id, ...";
    let first_placeholder = find_first_placeholder(query).unwrap();
    assert_eq!(first_placeholder, "id");
    let query = "SELECT $id;";
    let first_placeholder = find_first_placeholder(query).unwrap();
    assert_eq!(first_placeholder, "id");
}


#[derive(Debug, Clone, PartialEq)]
pub struct PartiallyReplacedQuery {
    pub left_part_with_replacement: String,
    pub placeholder: String,
    pub right_part: String,
}
pub fn find_and_replace_first_placeholder(query: &str, idx: i32) -> Option<PartiallyReplacedQuery> {
    let placeholder = find_first_placeholder(query)?;
    let placeholder_replacement = format!("${idx}");
    let replaced = query.replacen(&format!("${placeholder}"), &placeholder_replacement, 1);
    let byte_position = replaced.find(&placeholder_replacement).unwrap() + placeholder_replacement.len();
    let (left_part, right_part) = replaced.split_at(byte_position);
    Some(PartiallyReplacedQuery {
        left_part_with_replacement: left_part.to_string(),
        placeholder,
        right_part: right_part.to_string(),
    })
}


#[derive(Debug, Clone, PartialEq)]
pub struct ReplacedQuery {
    pub replaced_query: String,
    pub placeholders: Vec<String>,
}

pub fn replace_query(mut non_replaced_query: String) -> ReplacedQuery {
    let mut placeholders = vec![];
    let mut idx = 1;
    let mut replaced_query_str = "".to_string();
    while let Some(replaced_query) = find_and_replace_first_placeholder(&non_replaced_query, idx) {
        idx += 1;
        replaced_query_str.push_str(&replaced_query.left_part_with_replacement);
        placeholders.push(replaced_query.placeholder);
        non_replaced_query = replaced_query.right_part;
    }
    ReplacedQuery {
        replaced_query: replaced_query_str,
        placeholders,
    }
}

#[test]
fn test_replace_query() {
    let query = "SELECT $id , $id2, $id3;";
    let res = replace_query(query.to_string());
    assert_eq!(res, ReplacedQuery {
        replaced_query: "SELECT $1 , $2, $3".to_string(),
        placeholders: vec!["id".to_string(), "id2".to_string(), "id3".to_string()],
    });
}
#[test]
fn test_find_and_replace_first_placeholder() {
    let query = "SELECT $id";
    let first_placeholder = find_and_replace_first_placeholder(query, 1).unwrap();
    assert_eq!(first_placeholder, PartiallyReplacedQuery {
        left_part_with_replacement: "SELECT $1".to_string(),
        placeholder: "id".to_string(),
        right_part: "".to_string(),
    });

    let query = "SELECT $id ";
    let first_placeholder = find_and_replace_first_placeholder(query, 2).unwrap();
    assert_eq!(first_placeholder, PartiallyReplacedQuery {
        left_part_with_replacement: "SELECT $2".to_string(),
        placeholder: "id".to_string(),
        right_part: " ".to_string(),
    });

    let query = "SELECT $id , $id2, $id3;";
    let replaced_query = find_and_replace_first_placeholder(query, 1).unwrap();
    assert_eq!(replaced_query, PartiallyReplacedQuery {
        left_part_with_replacement: "SELECT $1".to_string(),
        placeholder: "id".to_string(),
        right_part: " , $id2, $id3;".to_string(),
    });
}