use anyhow::{Context, Result};
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use crate::comparator;
use crate::parser::TestCase;

/// Запускает все тесты на исполняемом файле и возвращает (пройдено, всего).
pub fn run_tests(exe_path: &Path, tests: &[TestCase]) -> Result<(usize, usize)> {
    let mut passed_count = 0;

    for test in tests {
        // Запускаем процесс с перехватом stdin, stdout и stderr
        let mut child = Command::new(exe_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context(format!("Не удалось запустить тест #{}", test.id))?;

        // Передаем входные данные в stdin
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(test.input.as_bytes())
                .context(format!("Ошибка записи данных в stdin для теста #{}", test.id))?;
        }

        // Ждем завершения и забираем вывод
        let output = child
            .wait_with_output()
            .context(format!("Ошибка чтения вывода для теста #{}", test.id))?;

        let actual_stdout = String::from_utf8_lossy(&output.stdout);
        let actual_stderr = String::from_utf8_lossy(&output.stderr);

        // Валидация результата
        if comparator::is_output_matching(&actual_stdout, &test.expected_output) {
            println!("Тест #{}: Пройден", test.id);
            passed_count += 1;
        } else {
            println!("Тест #{}: Провален", test.id);
            println!("   └─ Ожидалось:\n{}", indent_text(&test.expected_output));
            println!("   └─ Получено:\n{}", indent_text(&actual_stdout));
            if !actual_stderr.trim().is_empty() {
                println!(
                    "   └─ Предупреждения/Ошибки (stderr):\n{}",
                    indent_text(&actual_stderr)
                );
            }
        }
    }

    Ok((passed_count, tests.len()))
}

/// Вспомогательная функция для красивого форматирования многострочного вывода в консоли
fn indent_text(text: &str) -> String {
    text.lines()
        .map(|line| format!("      {}", line))
        .collect::<Vec<_>>()
        .join("\n")
}