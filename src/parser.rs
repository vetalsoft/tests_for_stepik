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

/// Исчерпывающий перечень ошибок, которые могут возникнуть при парсинге.
#[derive(Error, Debug)]
pub enum ParserError {
    #[error("Ошибка ввода-вывода при чтении файла: {0}")]
    Io(#[from] std::io::Error),

    #[error("Неверный формат заголовка теста в строке {line_number}: '{line}'")]
    InvalidFormat { line_number: usize, line: String },

    #[error("Отсутствует секция 'output' для теста #{test_id} (файл завершился или начался новый тест на строке {line_number})")]
    MissingOutput { test_id: usize, line_number: usize },

    #[error("Несоответствие номера теста: ожидался #{expected}, найден #{found} в строке {line_number}")]
    TestIdMismatch { expected: usize, found: usize, line_number: usize },

    #[error("Файл не содержит ни одного валидного теста")]
    NoTestsFound,
}

/// Состояния конечного автомата парсера.
enum ParserState {
    WaitingForTest,
    ReadingInput,
    ReadingOutput,
}

/// Читает и парсит файл с тестами по указанному пути.
pub fn parse_test_file<P: AsRef<Path>>(path: P) -> Result<Vec<TestCase>, ParserError> {
    let content = fs::read_to_string(path)?;
    parse_test_content(&content)
}

/// Парсит строковое содержимое, возвращая вектор тестовых случаев.
pub fn parse_test_content(content: &str) -> Result<Vec<TestCase>, ParserError> {
    let mut tests = Vec::new();
    let mut current_test: Option<TestCase> = None;
    let mut state = ParserState::WaitingForTest;

    for (line_idx, line) in content.lines().enumerate() {
        let line_number = line_idx + 1;
        let trimmed = line.trim();

        // Игнорируем пустые строки, если мы еще не начали читать первый тест
        if matches!(state, ParserState::WaitingForTest) && trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with("Test #") && trimmed.ends_with(" input") {
            finalize_current_test(&mut tests, &mut current_test, &state, line_number)?;
            
            let id = extract_test_id(trimmed, " input", line_number)?;
            current_test = Some(TestCase {
                id,
                input: String::new(),
                expected_output: String::new(),
            });
            state = ParserState::ReadingInput;
            
        } else if trimmed.starts_with("Test #") && trimmed.ends_with(" output") {
            let id = extract_test_id(trimmed, " output", line_number)?;
            
            let test = current_test.as_mut().ok_or_else(|| ParserError::InvalidFormat {
                line_number,
                line: line.to_string(),
            })?;

            if test.id != id {
                return Err(ParserError::TestIdMismatch {
                    expected: test.id,
                    found: id,
                    line_number,
                });
            }

            state = ParserState::ReadingOutput;
            
        } else {
            // Это строка с данными (ввод или вывод)
            if let Some(test) = current_test.as_mut() {
                match state {
                    ParserState::ReadingInput => append_to_buffer(&mut test.input, line),
                    ParserState::ReadingOutput => append_to_buffer(&mut test.expected_output, line),
                    ParserState::WaitingForTest => {
                        // Игнорируем мусорные строки до первого теста для устойчивости
                    }
                }
            }
        }
    }

    // Финализация последнего теста в файле
    finalize_current_test(&mut tests, &mut current_test, &state, content.lines().count() + 1)?;

    if tests.is_empty() {
        return Err(ParserError::NoTestsFound);
    }

    Ok(tests)
}

// --- Вспомогательные функции для чистоты основного цикла ---

/// Извлекает и валидирует ID теста из строки-маркера.
fn extract_test_id(marker: &str, suffix: &str, line_number: usize) -> Result<usize, ParserError> {
    let id_str = &marker["Test #".len() .. marker.len() - suffix.len()];
    id_str.parse::<usize>().map_err(|_| ParserError::InvalidFormat {
        line_number,
        line: marker.to_string(),
    })
}

/// Безопасно добавляет строку в буфер, сохраняя оригинальные переносы.
fn append_to_buffer(buffer: &mut String, line: &str) {
    if !buffer.is_empty() {
        buffer.push('\n');
    }
    buffer.push_str(line);
}

/// Проверяет корректность завершения предыдущего теста и сохраняет его.
fn finalize_current_test(
    tests: &mut Vec<TestCase>,
    current_test: &mut Option<TestCase>,
    state: &ParserState,
    line_number: usize,
) -> Result<(), ParserError> {
    if let Some(test) = current_test.take() {
        if matches!(state, ParserState::ReadingInput) {
            return Err(ParserError::MissingOutput {
                test_id: test.id,
                line_number,
            });
        }
        tests.push(test);
    }
    Ok(())
}