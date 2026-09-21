use anyhow::Result;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;
use thiserror::Error;
use wait_timeout::ChildExt;

use crate::comparator;
use crate::parser::TestCase;

/// Ошибки, которые могут возникнуть при запуске тестов
#[derive(Error, Debug)]
pub enum RunTestsError {
    /// Тест превысил лимит времени выполнения
    #[error("Тест #{0}: Превышен лимит времени выполнения")]
    Timeout(usize),

    /// Системные ошибки (запуск процесса, ввод-вывод и т.д.)
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

/// Запускает все тесты на исполняемом файле с таймаутом `watchdog_secs` секунд
///
/// Возвращает:
/// - `Ok((passed, total))` — все тесты завершены (успешно или провалены)
/// - `Err(RunTestsError::Timeout(test_id))` — тест превысил лимит времени, остальные тесты не выполняются
/// - `Err(RunTestsError::Other(e))` — системная ошибка
pub fn run_tests(
    exe_path: &Path,
    tests: &[TestCase],
    watchdog_secs: u64,
) -> Result<(usize, usize), RunTestsError> {
    let timeout = Duration::from_secs(watchdog_secs);
    let mut passed_count = 0;

    for test in tests {
        // Запускаем процесс с перехватом stdin, stdout и stderr
        let mut child = Command::new(exe_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| RunTestsError::Other(anyhow::Error::new(e)))?;

        // Передаем входные данные в stdin и закрываем его
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(test.input.as_bytes())
                .map_err(|e| RunTestsError::Other(anyhow::Error::new(e)))?;
        }

        // Забираем пайпы вывода для чтения в отдельных потоках
        let mut stdout_pipe = child.stdout.take().expect("stdout должен быть перехвачен");
        let mut stderr_pipe = child.stderr.take().expect("stderr должен быть перехвачен");

        // Потоки для чтения вывода, чтобы избежать дедлока при переполнении буфера пайпа
        let stdout_thread = std::thread::spawn(move || {
            let mut buf = Vec::new();
            stdout_pipe.read_to_end(&mut buf).map(|_| buf)
        });
        let stderr_thread = std::thread::spawn(move || {
            let mut buf = Vec::new();
            stderr_pipe.read_to_end(&mut buf).map(|_| buf)
        });

        // Ждем завершения процесса с таймаутом
        let status = child
            .wait_timeout(timeout)
            .map_err(|e| RunTestsError::Other(anyhow::Error::new(e)))?;

        match status {
            Some(_exit_status) => {
                // Процесс завершился сам, забираем вывод из потоков
                let stdout_bytes = stdout_thread
                    .join()
                    .map_err(|_| {
                        RunTestsError::Other(anyhow::anyhow!("Поток чтения stdout завершился с ошибкой"))
                    })?
                    .map_err(|e| RunTestsError::Other(anyhow::Error::new(e)))?;

                let stderr_bytes = stderr_thread
                    .join()
                    .map_err(|_| {
                        RunTestsError::Other(anyhow::anyhow!("Поток чтения stderr завершился с ошибкой"))
                    })?
                    .map_err(|e| RunTestsError::Other(anyhow::Error::new(e)))?;

                let actual_stdout = String::from_utf8_lossy(&stdout_bytes);
                let actual_stderr = String::from_utf8_lossy(&stderr_bytes);

                // Проверяем, есть ли что-то в stderr
                let has_stderr = !actual_stderr.trim().is_empty();

                // Валидация результата
                if comparator::is_output_matching(&actual_stdout, &test.expected_output) {
                    if has_stderr {
                        // Тест пройден, но есть stderr — предупреждаем
                        println!("Тест #{}: Пройден (с предупреждениями)", test.id);
                        println!("   └─ stderr:\n{}", indent_text(&actual_stderr));
                    } else {
                        println!("Тест #{}: Пройден", test.id);
                    }
                    passed_count += 1;
                } else {
                    println!("Тест #{}: Провален", test.id);
                    println!("   └─ Ожидалось:\n{}", indent_text(&test.expected_output));
                    println!("   └─ Получено:\n{}", indent_text(&actual_stdout));
                    if has_stderr {
                        println!("   └─ stderr:\n{}", indent_text(&actual_stderr));
                    }
                }
            }
            None => {
                // Таймаут: убиваем процесс
                child
                    .kill()
                    .map_err(|e| RunTestsError::Other(anyhow::Error::new(e)))?;
                child
                    .wait()
                    .map_err(|e| RunTestsError::Other(anyhow::Error::new(e)))?;

                // Потоки чтения завершатся после закрытия пайпов убитым процессом
                // Подождем их, чтобы избежать утечки потоков
                let _ = stdout_thread.join();
                let _ = stderr_thread.join();

                // Прерываем выполнение остальных тестов
                return Err(RunTestsError::Timeout(test.id));
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
