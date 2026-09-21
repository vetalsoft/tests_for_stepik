mod parser;
mod comparator;
mod compiler;
mod run_tests;

use anyhow::{Context, Result};
use parser::parse_test_file;
use std::process::ExitCode;

fn run() -> Result<ExitCode> {
    // 1. Разбор аргументов командной строки
    let args: Vec<String> = std::env::args().collect();
    
    // Если передан только путь к программе (или ничего), выводим справку и выходим
    if args.len() < 2 {
        println!("Использование: <путь_к_файлу.c> <путь_к_файлу_тестов.txt>");
        return Ok(ExitCode::SUCCESS); // Корректный выход из main() с кодом 0
    }

    let c_file_path = &args[1];
    
    // Если указан второй аргумент, берем его. Иначе формируем путь автоматически.
    let test_file_path = if args.len() >= 3 {
        args[2].clone()
    } else {
        // Замена расширения .c на .txt
        std::path::Path::new(c_file_path)
            .with_extension("txt")
            .to_string_lossy()
            .into_owned()
    };

    // 2. Компиляция C-кода
    let exe_path = match compiler::compile(c_file_path)? {
        Some(path) => path,
        None => return Ok(ExitCode::SUCCESS), // Ошибка компиляции уже выведена в stderr
    };

    // 3. Парсинг тестовых данных
    let tests = parse_test_file(&test_file_path)
        .context(format!("Ошибка при чтении файла тестов: {}", test_file_path))?;

    println!("Запуск {} тестов...\n", tests.len());

    // Переменная таймаута (в секундах)
    // TODO: Реализовать переопределение таймаута из CLI и/или конфигурации
    let watchdog: u64 = 5;

    // 4. Исполнение и проверка тестов
    match run_tests::run_tests(&exe_path, &tests, watchdog) {
        Ok((passed_count, total_count)) => {
            // 5. Итоговый отчет
            println!("\n Итог: пройдено {} из {} тестов.", passed_count, total_count);
            if passed_count == total_count {
                println!("Все тесты успешно пройдены!");
            }
        }
        Err(run_tests::RunTestsError::Timeout(test_id)) => {
            // Выходим из программы с сообщением, остальные тесты не выполняются
            return Err(anyhow::anyhow!("Тест #{}: Превышен лимит времени выполнения", test_id));
        }
        Err(e) => {
            // Системные ошибки возвращаем в main для обработки через anyhow
            return Err(e.into());
        }
    }

    Ok(ExitCode::SUCCESS)
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        Err(e) => {
            eprintln!("Ошибка: {:?}", e);
            ExitCode::FAILURE
        }
    }
}