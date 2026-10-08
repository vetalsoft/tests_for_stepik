use thiserror::Error;
use std::fs;
use std::path::Path;

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
    #[error("Несоответствие номера теста: ожидался #{expected}, найден #{found_id} в строке {line_number}")]
    TestIdMismatch { expected: usize, found_id: usize, line_number: usize },
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

struct ParseSuccess<'a, T> {
    value: T,
    remaining_input: Input<'a>,
}

type ParseResult<'a, T> = Result<ParseSuccess<'a, T>, ParserError>;

/// Читает и парсит файл с тестами по указанному пути.
pub fn parse_test_file<P: AsRef<Path>>(path: P) -> Result<Vec<TestCase>, ParserError> {
    let content = fs::read_to_string(path).map_err(ParserError::Io)?;
    parse_test_content(&content)
}

/// Парсит строковое содержимое, возвращая вектор тестовых случаев.
fn parse_test_content(content: &str) -> Result<Vec<TestCase>, ParserError> {
    let lines_vec: Vec<Line> = content.lines().enumerate()
        .map(|(idx, text)| Line { number: idx + 1, text})
        .collect();

    let mut input = Input {
        lines: &lines_vec,
        eof_line: lines_vec.len() + 1,
    };

    let mut tests: Vec<TestCase> = Vec::new();

    let input_skip = skip_before_test(input)?;
    input = input_skip.remaining_input;

    while !input.lines.is_empty() {
        if input.lines.is_empty() { break; }

        let test_step = parse_test_at_input(input)?;
        tests.push(test_step.value);
        input = test_step.remaining_input;
    }

    if tests.is_empty() {
        return Err(ParserError::NoTestsFound);
    }

    Ok(tests)
}

fn skip_before_test(mut input: Input<'_>) -> ParseResult<'_, ()> {
    while let Some(success) = next_line(input) {
        if is_input_header(success.value.text) {
            return Ok(ParseSuccess { value: (), remaining_input: input});
        }
        if is_output_header(success.value.text) {
            return Err(ParserError::InvalidFormat {
                line_number: success.value.number,
                line: success.value.text.to_owned(),
            });
        }
        input = success.remaining_input;
    }

    Ok(ParseSuccess {
        value: (),
        remaining_input: input,
    })
}

fn parse_test_at_input(input: Input<'_>) -> ParseResult<'_, TestCase> {
    let step1 = parse_input_header(input)?;
    let step2 = parse_input_body(step1)?;
    let step3 = parse_output_header(step2)?;
    let step4 = parse_output_body(step3)?;

    Ok(step4)
}

fn next_line(input: Input<'_>) -> Option<ParseSuccess<'_, Line<'_>>> {
    input.lines.split_first()
        .map(|(line, rest)| ParseSuccess {
            value: *line,
            remaining_input: Input { lines: rest, ..input },
        })
}

fn parse_input_header(input: Input<'_>) -> ParseResult<'_, usize> {
    let success = next_line(input).ok_or_else(|| ParserError::InvalidFormat {
        line_number: input.eof_line,
        line: "<EOF>".to_owned(),
    })?;

    if !is_input_header(success.value.text) {
        return Err(ParserError::InvalidFormat {
            line_number: success.value.number,
            line: success.value.text.to_owned() });
    }
    let id = extract_test_id(success.value.text, " input", success.value.number)?;

    Ok(ParseSuccess { value: id, remaining_input: success.remaining_input })
}

fn parse_input_body(prev: ParseSuccess<'_, usize>) -> ParseResult<'_, (usize, String)> {
    let test_id = prev.value;
    let mut input = prev.remaining_input;
    let mut buffer = String::new();
    while let Some(success) = next_line(input) {
        if is_output_header(success.value.text) {
            return Ok(ParseSuccess {
                value: (test_id, buffer),
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

fn parse_output_header(prev: ParseSuccess<'_, (usize, String)>) -> ParseResult<'_, (usize, String)> {
    let (expected_id, input_text) = prev.value;
    let input = prev.remaining_input;

    let success = next_line(input).ok_or(ParserError::MissingOutput {
        test_id: expected_id,
        line_number: input.eof_line,
    })?;

    if !is_output_header(success.value.text) {
        return Err(ParserError::InvalidFormat {
            line_number: success.value.number,
            line: success.value.text.to_owned(),
        });
    }
    let found_id = extract_test_id(success.value.text, " output", success.value.number)?;
    if found_id != expected_id {
        return Err(ParserError::TestIdMismatch {
            expected: expected_id,
            found_id,
            line_number: success.value.number,
        });
    }

    Ok(ParseSuccess { value: (expected_id, input_text), remaining_input: success.remaining_input })
}

fn parse_output_body(prev: ParseSuccess<'_, (usize, String)>) -> ParseResult<'_, TestCase> {
    let (id, input_text) = prev.value;
    let mut input = prev.remaining_input;
    let mut buffer = String::new();

    while let Some(success) = next_line(input) {
        if is_input_header(success.value.text) {
            let test_case = TestCase { id, input: input_text, expected_output: buffer };
            return Ok(ParseSuccess { value: test_case, remaining_input: input });
        }
        if is_output_header(success.value.text) {
            let found_id = extract_test_id(success.value.text, " output", success.value.number)?;
            if found_id != id {
                return Err(ParserError::TestIdMismatch { expected: id, found_id, line_number: success.value.number });
            }
            input = success.remaining_input;
            continue;
        }
        append_to_buffer(&mut buffer, success.value.text);
        input = success.remaining_input;
    }

    let test_case = TestCase { id, input: input_text, expected_output: buffer };
    Ok(ParseSuccess { value: test_case, remaining_input: input })
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
    id_str.parse::<usize>().map_err(|_| ParserError::InvalidFormat {
        line_number,
        line: marker.to_owned(),
    })
}

fn append_to_buffer(buffer: &mut String, line: &str) {
    if !buffer.is_empty() { buffer.push('\n'); }
    buffer.push_str(line);
}