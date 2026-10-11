use std::fs;
use std::path::Path;
use thiserror::Error;

/// Представление одного тестового случая.
#[derive(Debug, Clone, PartialEq)]
pub struct TestCase {
    pub id: usize,
    pub input: String,
    pub expected_output: String,
}

#[derive(Error, Debug)]
pub enum ParserError {
    #[error("Ошибка ввода-вывода при чтении файла: {0}")]
    Io(#[from] std::io::Error),
    #[error("Неверный формат заголовка теста в строке {line_number}: '{line}'")]
    InvalidFormat { line_number: usize, line: String },
    #[error("Отсутствует секция 'output' для теста #{test_id} (строка {line_number})")]
    MissingOutput { test_id: usize, line_number: usize },
    #[error(
        "Несоответствие номера теста: ожидался #{expected}, найден #{found_id} в строке {line_number}"
    )]
    TestIdMismatch {
        expected: usize,
        found_id: usize,
        line_number: usize,
    },
    #[error("Файл не содержит ни одного валидного теста")]
    NoTestsFound,
}

#[derive(Clone, Copy)]
struct Line<'a> {
    number: usize,
    text: &'a str,
}

#[derive(Clone, Copy)]
struct Input<'a> {
    lines: &'a [Line<'a>],
    eof_line: usize,
}

struct Parsed<'a, T> {
    value: T,
    remaining_input: Input<'a>,
}

type ParseResult<'a, T> = Result<Parsed<'a, T>, ParserError>;

pub struct InputParsed {
    pub test_id: usize,
    pub input_text: String,
}

/// Читает и парсит файл с тестами по указанному пути.
pub fn parse_test_file<P: AsRef<Path>>(path: P) -> Result<Vec<TestCase>, ParserError> {
    let content = fs::read_to_string(path).map_err(ParserError::Io)?;
    parse_test_content(&content)
}

/// Парсит строковое содержимое, возвращая вектор тестовых случаев.
fn parse_test_content(content: &str) -> Result<Vec<TestCase>, ParserError> {
    let lines_vec: Vec<Line> = content
        .lines()
        .enumerate()
        .map(|(idx, text)| Line {
            number: idx + 1,
            text,
        })
        .collect();

    let mut input = skip_before_test(Input {
            lines: &lines_vec,
            eof_line: lines_vec.len() + 1,
        })?
        .remaining_input;

    let res_tests: Result<Vec<TestCase>, ParserError> = std::iter::from_fn(move || {
        if input.lines.is_empty() {
            None // Стоп итерации
        } else {
            Some(parse_test_at_input(input).map(|step| {
                input = step.remaining_input;
                step.value
            }))
        }
    })
    .collect();

    let tests = res_tests?;
    if tests.is_empty() {
        return Err(ParserError::NoTestsFound);
    }

    Ok(tests)
}

fn parse_test_at_input(input: Input<'_>) -> ParseResult<'_, TestCase> {
    input
        .parse_input_header()?
        .parse_input_body()?
        .parse_output_header()?
        .parse_output_body()
}

fn next_line(input: Input<'_>) -> Option<Parsed<'_, Line<'_>>> {
    input.lines.split_first().map(|(line, rest)| Parsed {
        value: *line,
        remaining_input: Input {
            lines: rest,
            ..input
        },
    })
}

fn skip_before_test(mut input: Input<'_>) -> ParseResult<'_, ()> {
    while let Some(success) = next_line(input) {
        if is_input_header(success.value.text) {
            return Ok(Parsed {
                value: (),
                remaining_input: input,
            });
        }
        if is_output_header(success.value.text) {
            return Err(ParserError::InvalidFormat {
                line_number: success.value.number,
                line: success.value.text.to_owned(),
            });
        }
        input = success.remaining_input;
    }

    Ok(Parsed {
        value: (),
        remaining_input: input,
    })
}

impl<'a> Input<'a> {
    /// Парсим заголовок input, получаем ID
    fn parse_input_header(self) -> ParseResult<'a, usize> {
        let success = next_line(self).ok_or_else(|| ParserError::InvalidFormat {
            line_number: self.eof_line,
            line: "<EOF>".to_owned(),
        })?;

        if !is_input_header(success.value.text) {
            return Err(ParserError::InvalidFormat {
                line_number: success.value.number,
                line: success.value.text.to_owned(),
            });
        }
        let id = extract_test_id(success.value.text, " input", success.value.number)?;

        Ok(Parsed {
            value: id,
            remaining_input: success.remaining_input,
        })
    }
}

impl<'a> Parsed<'a, usize> {
    /// Парсим тело input, переходим к состоянию InputParsed
    fn parse_input_body(self) -> ParseResult<'a, InputParsed> {
        let test_id = self.value;
        let mut input = self.remaining_input;
        let mut buffer = String::new();
        while let Some(success) = next_line(input) {
            if is_output_header(success.value.text) {
                return Ok(Parsed {
                    value: InputParsed {
                        test_id,
                        input_text: buffer,
                    },
                    remaining_input: input,
                });
            }
            if is_input_header(success.value.text) {
                return Err(ParserError::MissingOutput {
                    test_id,
                    line_number: success.value.number,
                });
            }
            append_to_buffer(&mut buffer, success.value.text);
            input = success.remaining_input;
        }

        Err(ParserError::MissingOutput {
            test_id,
            line_number: input.eof_line,
        })
    }
}

impl<'a> Parsed<'a, InputParsed> {
    /// Парсим заголовок, состояние не меняется
    fn parse_output_header(self) -> ParseResult<'a, InputParsed> {
        let state = self.value;

        let success = next_line(self.remaining_input).ok_or(ParserError::MissingOutput {
            test_id: state.test_id,
            line_number: self.remaining_input.eof_line,
        })?;

        if !is_output_header(success.value.text) {
            return Err(ParserError::InvalidFormat {
                line_number: success.value.number,
                line: success.value.text.to_owned(),
            });
        }
        let found_id = extract_test_id(success.value.text, " output", success.value.number)?;
        if found_id != state.test_id {
            return Err(ParserError::TestIdMismatch {
                expected: state.test_id,
                found_id,
                line_number: success.value.number,
            });
        }

        Ok(Parsed {
            value: state,
            remaining_input: success.remaining_input,
        })
    }

    /// Парсим тело output, завершаем TestCase
    fn parse_output_body(self) -> ParseResult<'a, TestCase> {
        let state = self.value;
        let mut input = self.remaining_input;
        let mut buffer = String::new();

        while let Some(success) = next_line(input) {
            if is_input_header(success.value.text) {
                let test_case = TestCase {
                    id: state.test_id,
                    input: state.input_text,
                    expected_output: buffer,
                };
                return Ok(Parsed {
                    value: test_case,
                    remaining_input: input,
                });
            }
            if is_output_header(success.value.text) {
                let found_id =
                    extract_test_id(success.value.text, " output", success.value.number)?;
                if found_id != state.test_id {
                    return Err(ParserError::TestIdMismatch {
                        expected: state.test_id,
                        found_id,
                        line_number: success.value.number,
                    });
                }
                input = success.remaining_input;
                continue;
            }
            append_to_buffer(&mut buffer, success.value.text);
            input = success.remaining_input;
        }

        Ok(Parsed {
            value: TestCase {
                id: state.test_id,
                input: state.input_text,
                expected_output: buffer,
            },
            remaining_input: input,
        })
    }
}

fn is_input_header(text: &str) -> bool {
    let text = text.trim();
    text.starts_with("Test #") && text.ends_with(" input")
}

fn is_output_header(text: &str) -> bool {
    let text = text.trim();
    text.starts_with("Test #") && text.ends_with(" output")
}

fn extract_test_id(marker: &str, suffix: &str, line_number: usize) -> Result<usize, ParserError> {
    let marker = marker.trim();
    let id_str = &marker["Test #".len()..marker.len() - suffix.len()];
    id_str
        .parse::<usize>()
        .map_err(|_| ParserError::InvalidFormat {
            line_number,
            line: marker.to_owned(),
        })
}

fn append_to_buffer(buffer: &mut String, line: &str) {
    if !buffer.is_empty() {
        buffer.push('\n');
    }
    buffer.push_str(line);
}

#[cfg(test)]
mod tests {
    use super::*;

//------------------------
// УСПЕШНЫЕ ВАРИАНТЫ
//------------------------

    #[test]
    fn test_single_test_case() {
        let content = r#"Test #1 input
1 2 3
Test #1 output
6
"#;
        let result = parse_test_content(content).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, 1);
        assert_eq!(result[0].input, "1 2 3");
        assert_eq!(result[0].expected_output, "6");
    }

    #[test]
    fn test_multiple_test_cases() {
        let content = r#"Test #1 input
1 2 3
Test #1 output
6
Test #2 input
4 5
Test #2 output
9
Test #3 input
10
Test #3 output
10
"#;
        let result = parse_test_content(content).unwrap();
        assert_eq!(result.len(), 3);
        
        assert_eq!(result[0].id, 1);
        assert_eq!(result[0].input, "1 2 3");
        assert_eq!(result[0].expected_output, "6");
        
        assert_eq!(result[1].id, 2);
        assert_eq!(result[1].input, "4 5");
        assert_eq!(result[1].expected_output, "9");
        
        assert_eq!(result[2].id, 3);
        assert_eq!(result[2].input, "10");
        assert_eq!(result[2].expected_output, "10");
    }

    #[test]
    fn test_multiline_input_and_output() {
        let content = r#"Test #1 input
line1
line2
line3
Test #1 output
result1
result2
"#;
        let result = parse_test_content(content).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].input, "line1\nline2\nline3");
        assert_eq!(result[0].expected_output, "result1\nresult2");
    }

    #[test]
    fn test_garbage_before_first_test() {
        let content = r#"Какой-то мусор или коментарий
Ещё строка мусора

Test #1 input
data
Test #1 output
result
"#;
        let result = parse_test_content(content).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, 1);
        assert_eq!(result[0].input, "data");
        assert_eq!(result[0].expected_output, "result");
    }

    #[test]
    fn test_empty_input_and_output() {
        let content = r#"Валидный тест без входных и выходных данных
Test #1 input
Test #1 output
"#;
        let result = parse_test_content(content).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].input, "");
        assert_eq!(result[0].expected_output, "");
    }

//------------------------
// ВАРИАНТЫ С ОШИБКАМИ
//------------------------

    #[test]
    fn test_empty_file() {
        let content = "";
        let result = parse_test_content(content);
        assert!(matches!(result, Err(ParserError::NoTestsFound)));
    }

    #[test]
    fn test_only_garbage() {
        let content = "мусор\nещё мусор\n";
        let result = parse_test_content(content);
        assert!(matches!(result, Err(ParserError::NoTestsFound)));
    }

    #[test]
    fn test_missing_output() {
        let content = r#"
Нет выходных данных для Тест1
Test #1 input
data
Test #2 input
more data
Test #2 output
result
"#;
        let result = parse_test_content(content);
        assert!(matches!(
            result,
            Err(ParserError::MissingOutput { test_id: 1, .. })
        ));
    }

    #[test]
    fn test_id_mismatch() {
        let content = r#"Test #1 input
data
Test #2 output
result
"#;
        let result = parse_test_content(content);
        assert!(matches!(
            result,
            Err(ParserError::TestIdMismatch {
                expected: 1,
                found_id: 2,
                ..
            })
        ));
    }

    #[test]
    fn test_invalid_header_format() {
        let content = r#"Test #abc input
data
Test #abc output
result
"#;
        let result = parse_test_content(content);
        assert!(matches!(result, Err(ParserError::InvalidFormat { .. })));
    }

    #[test]
    fn test_output_before_input() {
        let content = r#"Test #1 output
result
"#;
        let result = parse_test_content(content);
        assert!(matches!(result, Err(ParserError::InvalidFormat { .. })));
    }
}