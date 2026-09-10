/// Сравнивает фактический вывод с ожидаемым, игнорируя незначительные различия
/// в пробельных символах в конце строк и финальные пустые переносы.
pub fn is_output_matching(actual: &str, expected: &str) -> bool {
    let mut actual_lines = actual.lines();
    let mut expected_lines = expected.lines();

    loop {
        match (actual_lines.next(), expected_lines.next()) {
            (Some(a), Some(e)) => {
                // Сравниваем строки после удаления завершающих пробельных символов
                if a.trim_end() != e.trim_end() {
                    return false;
                }
            }
            (None, None) => return true, // Успешное завершение
            (Some(a), None) => return a.trim().is_empty(), // Игнорируем финальные пустые строки в actual
            (None, Some(e)) => return e.trim().is_empty(), // Игнорируем финальные пустые строки в expected
        }
    }
}