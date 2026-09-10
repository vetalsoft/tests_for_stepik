mod parser;
mod comparator;

use anyhow::{Context, Result};
use parser::parse_test_file;
use std::io::Write;
use std::process::{Command, Stdio};

fn main() -> Result<()> {
    // 1. Разбор аргументов командной строки
    let args: Vec<String> = std::env::args().collect();
    
    // Если передан только путь к программе (или ничего), выводим справку и выходим
    if args.len() < 2 {
        println!("Использование: <путь_к_файлу.c> <путь_к_файлу_тестов.txt>");
        return Ok(()); // Корректный выход из main() с кодом 0
    }

    let c_file_path = &args[1];
    
    // Если указан второй аргумент, берем его. Иначе формируем путь автоматически.
    let test_file_path = if args.len() >= 3 {
        args[2].clone()
    } else {
        // Безопасная замена расширения .c на .txt (работает даже с путями вроде "src/task.c")
        std::path::Path::new(c_file_path)
            .with_extension("txt")
            .to_string_lossy()
            .into_owned()
    };

    // 2. Создание временного файла для исполняемого бинарника
    let exe_suffix = if cfg!(windows) { ".exe" } else { "" };
    let temp_exe = tempfile::Builder::new()
        .prefix("c_test_runner_")
        .suffix(exe_suffix)
        .tempfile()
        .context("Не удалось создать временный файл для компиляции")?;
    
    let exe_path = temp_exe.into_temp_path();

    // 3. Компиляция C-кода
    println!("Компиляция {}...", c_file_path);
    let compile_output = Command::new("gcc") // Можно заменить на "clang" или "cc"
        .arg(c_file_path)
        .arg("-o")
        .arg(&exe_path)
        .arg("-Wall")
        .arg("-Wextra")
        .arg("-std=c11")
        .arg("-lm")
        .output()
        .context("Не удалось запустить компилятор gcc. Установлен ли он в системе?")?;

    if !compile_output.status.success() {
        eprintln!("Ошибка компиляции:");
        eprintln!("{}", String::from_utf8_lossy(&compile_output.stderr));
        return Ok(()); // Завершаем работу, так как запускать нечего
    }
    println!("Компиляция успешна.\n");

    // 4. Парсинг тестовых данных
    let tests = parse_test_file(&test_file_path)
        .context(format!("Ошибка при чтении файла тестов: {}", test_file_path))?;

    println!("Запуск {} тестов...\n", tests.len());

    // 5. Исполнение и проверка
    let mut passed_count = 0;

    for test in &tests {
        // Запускаем процесс с перехватом stdin, stdout и stderr
        let mut child = Command::new(&exe_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .context(format!("Не удалось запустить тест #{}", test.id))?;

        // Передаем входные данные в stdin
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(test.input.as_bytes())
                .context(format!("Ошибка записи данных в stdin для теста #{}", test.id))?;
        }

        // Ждем завершения и забираем вывод
        let output = child.wait_with_output()
            .context(format!("Ошибка чтения вывода для теста #{}", test.id))?;

        let actual_stdout = String::from_utf8_lossy(&output.stdout);
        let actual_stderr = String::from_utf8_lossy(&output.stderr);

        // 6. Валидация
        if comparator::is_output_matching(&actual_stdout, &test.expected_output) {
            println!("Тест #{}: Пройден", test.id);
            passed_count += 1;
        } else {
            println!("Тест #{}: Провален", test.id);
            println!("   └─ Ожидалось:\n{}", indent_text(&test.expected_output));
            println!("   └─ Получено:\n{}", indent_text(&actual_stdout));
            
            if !actual_stderr.trim().is_empty() {
                println!("   └─ Предупреждения/Ошибки (stderr):\n{}", indent_text(&actual_stderr));
            }
        }
    }

    // 7. Итоговый отчет
    println!("\n Итог: пройдено {} из {} тестов.", passed_count, tests.len());
    
    if passed_count == tests.len() {
        println!("Все тесты успешно пройдены!");
    }

    // Временный файл удалится автоматически при выходе из области видимости temp_exe,
    // но на Windows иногда требуется явное удаление, если файл заблокирован.
    // tempfile делает это максимально надежно.
    
    Ok(())
}

/// Вспомогательная функция для красивого форматирования многострочного вывода в консоли
fn indent_text(text: &str) -> String {
    text.lines()
        .map(|line| format!("      {}", line))
        .collect::<Vec<_>>()
        .join("\n")
}