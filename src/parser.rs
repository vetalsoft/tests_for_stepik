//! Парсер файла тестов для C-программ.
//!
//! Формат файла тестов:
//! ```text
//! Test #1 input
//! <входные данные, могут быть многострочными>
//! Test #1 output
//! <ожидаемый вывод, могут быть многострочными>
//! Test #2 input
//! ...
//! ```
//!
//! Перед первым тестом допускается произвольный «мусор» (комментарии, пустые строки) —
//! он будет проигнорирован. Номера тестов должны идти последовательно, начиная с 1.
//!
//! # Пример использования
//! ```no_run
//! use parser::parse_test_file;
//!
//! let tests = parse_test_file("tests.txt").expect("ошибка чтения файла");
//! for t in &tests {
//!     println!("Тест #{}: input={:?}, expected={:?}", t.id, t.input, t.expected_output);
//! }
//! `

use std::fs;
use std::ops::Not;
use std::path::Path;
use thiserror::Error;

/// Один тестовый случай: входные данные и ожидаемый вывод.
///
/// Формируется парсером после успешного разбора пары секций `input`
#[derive(Debug, Clone, PartialEq)]
pub struct TestCase {
    /// Порядковый номер теста (начинается с 1).
    pub id: usize,
    /// Входные данные, которые будут поданы в `stdin` программы.
    pub input: String,
    // Ожидаемый вывод в `stdout`, с которым сравнивается фактический результат.
    pub expected_output: String,
}

/// Ошибки, возникающие при парсинге файла тестов.
///
/// Все варианты содержат достаточно информации (номер строки, содержимое строки),
/// чтобы пользователь мог быстро найти и исправить проблему в файле.
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

/// Одна строка файла вместе с её физическим номером (начинается с 1).
///
/// Используется для формирования точных сообщений об ошибках:
/// парсер всегда знает, в какой строке файла он находится.
#[derive(Clone, Copy)]
struct Line<'a> {
    number: usize,
    text: &'a str,
}

/// «Курсор» парсера — срез ещё не обработанных строк.
///
/// Каждая функция парсинга принимает `Input` и возвращает новый `Input`,
/// сдвинутый на количество съеденных строк. Это обеспечивает неизменяемый
/// (immutable) стиль работы без индексов.
#[derive(Clone, Copy)]
struct Input<'a> {
    lines: &'a [Line<'a>],
    /// Номер строки, которая была бы следующей после конца файла.
    /// Используется в сообщениях об ошибках.
    eof_line: usize,
}

/// Результат успешного шага парсинга: извлечённое значение + оставшийся вход.
struct Parsed<'a, T> {
    value: T,
    remaining_input: Input<'a>,
}

/// Алиас типа для читаемости: результат парсинга — это либо успех с `Parsed<T>`,
/// либо ошибка `ParserError`.
type ParseResult<'a, T> = Result<Parsed<'a, T>, ParserError>;

/// Промежуточное состояние парсера между чтением `Test #N input` и `Test #N output`.
///
/// Хранит уже разобранный `test_id` и накопленный текст входных данных,
/// пока парсер ждёт заголовок `Test #N output`.
pub struct InputParsed {
    pub test_id: usize,
    pub input_text: String,
}

/// Читает файл тестов по пути и возвращает список разобранных тестовых случаев.
///
/// # Ошибки
/// - [`ParserError::Io`] — если файл не удалось прочитать.
/// - Другие варианты [`ParserError`] — если содержимое файла некорректно.
pub fn parse_test_file<P: AsRef<Path>>(path: P) -> Result<Vec<TestCase>, ParserError> {
    let content = fs::read_to_string(path).map_err(ParserError::Io)?;
    parse_test_content(&content)
}

/// Парсит строковое содержимое файла в список тестов.
///
/// Алгоритм:
/// 1. Разбивает содержимое на `Line` с номерами.
/// 2. Пропускает «мусор» до первого валидного заголовка `Test #N input`.
/// 3. Последовательно извлекает тесты через [`Input::parse_many`].
/// 4. Если тестов не найдено — возвращает [`ParserError::NoTestsFound`].
fn parse_test_content(content: &str) -> Result<Vec<TestCase>, ParserError> {
    // `lines_vec` — владелец данных, на которые ссылается `input`.
    let lines_vec: Vec<Line> = content
        .lines()
        .enumerate()
        .map(|(idx, text)| Line {
            number: idx + 1,
            text,
        })
        .collect();

    let input = Input {
        lines: &lines_vec,
        eof_line: lines_vec.len() + 1,
    };

    input
        .skip_before_test()?
        .remaining_input
        .parse_many(|i| i.parse_test_at_input())
        .and_then(|tests| {
            tests
                .is_empty()
                .not()
                .then_some(tests)
                .ok_or(ParserError::NoTestsFound)
        })
}

impl<'a> Input<'a> {
    /// Многократно применяет переданную функцию парсинга, пока вход не закончится.
    ///
    /// На первой ошибке прерывает выполнение и возвращает её.
    /// Используется для последовательного разбора всех тестов в файле.
    fn parse_many<F>(mut self, parse_fn: F) -> Result<Vec<TestCase>, ParserError>
    where
        F: Fn(Input<'a>) -> ParseResult<'a, TestCase>,
    {
        std::iter::from_fn(move || {
            if self.lines.is_empty() {
                None // Стоп
            } else {
                Some(parse_fn(self).map(|step| {
                    self = step.remaining_input;
                    step.value
                }))
            }
        })
        .collect::<Result<Vec<TestCase>, ParserError>>()
    }

    /// Разбирает один полный тест.
    fn parse_test_at_input(self) -> ParseResult<'a, TestCase> {
        self.parse_input_header()?
            .parse_input_body()?
            .parse_output_header()?
            .parse_output_body()
    }

    /// «Откусывает» первую строку из входа, возвращая её и остаток.
    ///
    /// Возвращает [`None`], если вход пуст.
    fn next_line(self) -> Option<Parsed<'a, Line<'a>>> {
        self.lines.split_first().map(|(line, rest)| Parsed {
            value: *line,
            remaining_input: Input {
                lines: rest,
                ..self
            },
        })
    }

    /// Пропускает строки до первого валидного заголовка `Test #N input`.
    ///
    /// Если перед первым тестом встретится заголовок `Test #N output`
    ///   — это ошибка формата: [`ParserError::InvalidFormat`].
    fn skip_before_test(mut self) -> ParseResult<'a, ()> {
        while let Some(success) = self.next_line() {
            if is_input_header(success.value.text) {
                return Ok(Parsed {
                    value: (),
                    remaining_input: self,
                });
            }
            if is_output_header(success.value.text) {
                return Err(ParserError::InvalidFormat {
                    line_number: success.value.number,
                    line: success.value.text.to_owned(),
                });
            }
            self = success.remaining_input;
        }

        Ok(Parsed {
            value: (),
            remaining_input: self,
        })
    }

    /// Ожидает и проверяет заголовок вида `Test #N input`.
    ///
    /// Возвращает номер теста `N` при успехе.
    /// Или ошибку: [`ParserError::InvalidFormat`]
    fn parse_input_header(self) -> ParseResult<'a, usize> {
        let success = self.next_line().ok_or_else(|| ParserError::InvalidFormat {
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
    /// Читает тело секции `Test #N input` до ближайшего заголовка `Test #N output`.
    ///
    /// Все строки между заголовками накапливаются в буфер (через [`append_to_buffer`]).
    /// Если встретится ещё один `Test #N input` без промежуточного `Test #N output`
    ///   — ошибка: [`ParserError::MissingOutput`].
    fn parse_input_body(self) -> ParseResult<'a, InputParsed> {
        let test_id = self.value;
        let mut input = self.remaining_input;
        let mut buffer = String::new();
        while let Some(success) = input.next_line() {
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
    /// Ожидает и проверяет заголовок `Test #N output`.
    ///
    /// Убеждается, что `N` совпадает с `test_id` из предыдущего `input`.
    /// Если `N` несовпало — ошибка: [`ParserError::TestIdMismatch`].
    fn parse_output_header(self) -> ParseResult<'a, InputParsed> {
        let state = self.value;

        let success = self
            .remaining_input
            .next_line()
            .ok_or(ParserError::MissingOutput {
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

    /// Читает тело секции `Test #N output` до следующего `Test #N input` (или до EOF).
    ///
    /// Завершает формирование [`TestCase`]. Если в теле попадётся заголовок `Test #N output`
    /// с тем же `test_id` — он игнорируется (защита от дубликатов).
    ///
    /// # TODO
    /// Определиться с финальной стратегией обработки дубликатов `test_id`.
    /// Сейчас они игнорируются, но может стоит возвращать ошибку парсинга 🤔.
    fn parse_output_body(self) -> ParseResult<'a, TestCase> {
        let state = self.value;
        let mut input = self.remaining_input;
        let mut buffer = String::new();

        while let Some(success) = input.next_line() {
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

/// Проверяет, является ли строка заголовком секции `Test #N input`.
///
/// Допускаются ведущие/замыкающие пробелы. Формат: `Test #<число> input`.
fn is_input_header(text: &str) -> bool {
    let text = text.trim();
    text.starts_with("Test #") && text.ends_with(" input")
}

/// Проверяет, является ли строка заголовком секции `Test #N output`.
///
/// Допускаются ведущие/замыкающие пробелы. Формат: `Test #<число> output`.
fn is_output_header(text: &str) -> bool {
    let text = text.trim();
    text.starts_with("Test #") && text.ends_with(" output")
}

/// Извлекает числовой ID теста из строки-маркера.
///
/// `suffix` — ожидаемое окончание строки (`" input"` или `" output"`).
/// Возвращает [`ParserError::InvalidFormat`], если ID не является числом.
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

/// Добавляет строку в буфер многострочного текста.
///
/// Между строками вставляет `\n`. Первая строка добавляется без префикса.
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
        let content = r#"Какой-то мусор или комментарий
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
