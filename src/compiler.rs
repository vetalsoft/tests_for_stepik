use anyhow::{Context, Result};
use std::process::Command;
use tempfile::TempPath;

/// Компилирует C-файл во временный исполняемый файл.
///
/// Возвращает:
/// - `Ok(Some(path))` — компиляция успешна, путь к бинарнику
/// - `Ok(None)`       — компиляция завершилась с ошибкой (сообщение уже выведено в stderr)
/// - `Err(e)`         — системная ошибка (нет gcc, не удалось создать временный файл и т.д.)
pub fn compile(c_file_path: &str) -> Result<Option<TempPath>> {
    // 1. Создание временного файла для исполняемого бинарника
    let exe_suffix = if cfg!(windows) { ".exe" } else { "" };
    let temp_exe = tempfile::Builder::new()
        .prefix("c_test_runner_")
        .suffix(exe_suffix)
        .tempfile()
        .context("Не удалось создать временный файл для компиляции")?;
    let exe_path = temp_exe.into_temp_path();

    // 2. Компиляция C-кода
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
        return Ok(None); // Завершаем работу, так как запускать нечего
    }

    // Если компиляция успешна, но есть предупреждения в stderr
    let stderr_output = String::from_utf8_lossy(&compile_output.stderr);
    if !stderr_output.trim().is_empty() {
        eprintln!("Предупреждения компилятора (код собран, но есть замечания):");
        eprintln!("{}", stderr_output);
    }

    println!("Компиляция успешна.\n");

    Ok(Some(exe_path))
}