extern crate libc;
extern crate rustyline;
extern crate thiserror;

use rustyline::DefaultEditor;
use std::env;
use std::error::Error;
use std::fs::File;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;
use thiserror::Error as ThisError;

mod colors;
mod tokens;

use tokens::tokenize_commands;

#[derive(ThisError, Debug)]
enum ProgramError {
    #[error("Error processing readline input from prompt")]
    Readline(String),
}

fn main() {
    unsafe {
        libc::signal(libc::SIGINT, libc::SIG_IGN);
        libc::signal(libc::SIGQUIT, libc::SIG_IGN);
    }
    match main_loop() {
        Ok(_) => std::process::exit(0),
        Err(e) => {
            eprintln!("error: {}", e);
            std::process::exit(1);
        }
    }
}

fn main_loop() -> Result<(), Box<dyn std::error::Error>> {
    unsafe {
        libc::signal(libc::SIGINT, libc::SIG_IGN);
        libc::signal(libc::SIGQUIT, libc::SIG_IGN);
    }
    let mut last_exit_status = true;
    let mut rl = DefaultEditor::new().expect("Couldn't create editor");
    let home = match env::var("HOME") {
        Ok(v) => v,
        Err(e) => panic!("{}", e),
    };

    if rl.load_history(&format!("{}/.rush_history", home)).is_err() {
        println!("No previous history.");
        File::create(format!("{}/.rush_history", home)).expect("Couldn't create history file");
    }
    loop {
        let prompt_string = generate_prompt(last_exit_status);
        let command_string = read_command(&mut rl, prompt_string)?;
        let commands = tokenize_commands(&command_string);

        for command in commands {
            last_exit_status = true;
            for mut dependent_command in command {
                let mut is_background = false;
                if let Some(&"&") = dependent_command.last() {
                    is_background = true;
                    dependent_command.pop();
                }
                match dependent_command[0] {
                    "exit" => {
                        rl.save_history(&format!("{}/.rush_history", home))
                            .expect("Couldn't save history");
                        std::process::exit(0);
                    }
                    "cd" => {
                        last_exit_status = change_dir(dependent_command[1]);
                    }
                    _ => {
                        last_exit_status = execute_command(dependent_command, is_background);
                    }
                }
                if !last_exit_status {
                    break;
                }
            }
        }
    }
}

fn read_command(rl: &mut DefaultEditor, prompt_string: String) -> Result<String, Box<dyn Error>> {
    let mut command_string = match rl.readline(&prompt_string) {
        Ok(v) => v,
        Err(_) => {
            return Err(Box::new(ProgramError::Readline(
                "error during readline()".to_string(),
            )))
        }
    };

    // this allows for multiline commands
    while command_string.ends_with('\\') {
        command_string.pop(); // remove the trailing backslash
        let next_string = rl.readline("").unwrap();
        command_string.push_str(&next_string);
    }

    // add command to history after handling multi-line input
    rl.add_history_entry(&command_string)
        .expect("add_history_entry");
    Ok(command_string)
}

fn generate_prompt(last_exit_status: bool) -> String {
    let path = env::current_dir().unwrap();
    let prompt = format!(
        "{}RUSHING IN {}{}{}\n",
        colors::ANSI_BOLD,
        colors::ANSI_COLOR_CYAN,
        path.display(),
        colors::RESET
    );
    if last_exit_status {
        format!(
            "{}{}{}\u{2ba1}{}  ",
            prompt,
            colors::ANSI_BOLD,
            colors::GREEN,
            colors::RESET
        )
    } else {
        format!(
            "{}{}{}\u{2ba1}{}  ",
            prompt,
            colors::ANSI_BOLD,
            colors::RED,
            colors::RESET
        )
    }
}

fn execute_command(command_tokens: Vec<&str>, is_background: bool) -> bool {
    let mut command_instance = Command::new(command_tokens[0]);
    let command = command_instance.args(&command_tokens[1..]);

    unsafe {
        if let Ok(mut child) = command
            .pre_exec(|| {
                libc::signal(libc::SIGINT, libc::SIG_DFL);
                libc::signal(libc::SIGQUIT, libc::SIG_DFL);
                Result::Ok(())
            })
            .spawn()
        {
            if !is_background {
                child.wait().expect("command wasn't running").success()
            } else {
                colors::success_logger(format!("{} started!", child.id()));
                true
            }
        } else {
            colors::error_logger("Command not found!".to_string());
            false
        }
    }
}

fn change_dir(new_path: &str) -> bool {
    let new_path = Path::new(new_path);
    if let Err(err) = env::set_current_dir(new_path) {
        colors::error_logger(format!("Failed to change the directory!\n{}", err));
        return false;
    }
    true
}
